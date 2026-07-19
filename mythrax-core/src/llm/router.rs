use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};
use std::process::Command;
use crate::contracts::{ModelTier, TaskArchetype, TaskProfile};
use crate::db::StorageBackend;
use anyhow::{Result, Context};
use tokio::sync::Mutex as TokioMutex;

/// Global GPU reservation lock to coalesce model loading/routing
pub static GPU_RESERVATION_LOCK: OnceLock<TokioMutex<()>> = OnceLock::new();

pub fn gpu_reservation_lock() -> &'static TokioMutex<()> {
    GPU_RESERVATION_LOCK.get_or_init(|| TokioMutex::new(()))
}

/// In-memory cache for database-stored model-to-tier mappings
static TIER_MAPPINGS: OnceLock<RwLock<HashMap<String, ModelTier>>> = OnceLock::new();

/// Cached swap usage check
struct SwapCache {
    total_mb: f64,
    used_mb: f64,
    last_checked: Instant,
}

static SWAP_CACHE: OnceLock<RwLock<Option<SwapCache>>> = OnceLock::new();

/// Reads macOS swap usage via sysctl vm.swapusage
pub fn get_swap_usage() -> Result<(f64, f64)> {
    let cache_lock = SWAP_CACHE.get_or_init(|| RwLock::new(None));
    
    if let Ok(read_guard) = cache_lock.read() {
        if let Some(ref cache) = *read_guard {
            if cache.last_checked.elapsed() < Duration::from_secs(5) {
                return Ok((cache.total_mb, cache.used_mb));
            }
        }
    }

    let output = Command::new("sysctl")
        .arg("vm.swapusage")
        .output()
        .context("Failed to run sysctl vm.swapusage")?;

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let (total_mb, used_mb) = parse_swap_usage(&stdout_str)?;

    if let Ok(mut write_guard) = cache_lock.write() {
        *write_guard = Some(SwapCache {
            total_mb,
            used_mb,
            last_checked: Instant::now(),
        });
    }

    Ok((total_mb, used_mb))
}

fn parse_swap_usage(stdout: &str) -> Result<(f64, f64)> {
    let mut total_mb = 0.0;
    let mut used_mb = 0.0;

    if let Some(total_pos) = stdout.find("total =") {
        let after_total = &stdout[total_pos + 7..];
        if let Some(m_pos) = after_total.find('M') {
            total_mb = after_total[..m_pos].trim().parse::<f64>().unwrap_or(0.0);
        }
    }

    if let Some(used_pos) = stdout.find("used =") {
        let after_used = &stdout[used_pos + 6..];
        if let Some(m_pos) = after_used.find('M') {
            used_mb = after_used[..m_pos].trim().parse::<f64>().unwrap_or(0.0);
        }
    }

    Ok((total_mb, used_mb))
}

/// Reloads model-to-tier mappings from SurrealDB settings
pub async fn reload_tier_mappings(db: &dyn StorageBackend) -> Result<()> {
    let config = db.get_llm_config().await?;
    let mut mappings = HashMap::new();

    if let Some(db_mappings) = config.model_tier_mappings {
        for (model_name, tier_str) in db_mappings {
            if let Some(tier) = parse_model_tier(&tier_str) {
                mappings.insert(model_name, tier);
            }
        }
    }

    let cache = TIER_MAPPINGS.get_or_init(|| RwLock::new(HashMap::new()));
    if let Ok(mut write_guard) = cache.write() {
        *write_guard = mappings;
    }

    Ok(())
}

fn parse_model_tier(s: &str) -> Option<ModelTier> {
    match s.to_lowercase().as_str() {
        "micro" => Some(ModelTier::Micro),
        "small" => Some(ModelTier::Small),
        "medium" => Some(ModelTier::Medium),
        "large" | "largelocal" => Some(ModelTier::Large),
        "cloud" => Some(ModelTier::Cloud),
        _ => None,
    }
}

pub async fn route_task(db: &dyn StorageBackend, profile: &TaskProfile) -> ModelTier {
    if std::env::var("MYTHRAX_DISABLE_SWAP_ROUTING").is_err() {
        if let Ok((_total_swap, used_swap)) = get_swap_usage() {
            if used_swap >= 4000.0 {
                let has_cloud_key = std::env::var("GEMINI_API_KEY").map(|s| !s.trim().is_empty()).unwrap_or(false)
                    || std::env::var("ANTHROPIC_API_KEY").map(|s| !s.trim().is_empty()).unwrap_or(false);
                if has_cloud_key {
                    tracing::warn!("Swap usage used is {:.2}MB (>= 4000MB), routing to Cloud under memory pressure", used_swap);
                    return ModelTier::Cloud;
                } else {
                    tracing::info!("Swap usage used is {:.2}MB, but no Cloud API keys configured. Staying local.", used_swap);
                }
            }
        }
    }

    let cache = TIER_MAPPINGS.get_or_init(|| RwLock::new(HashMap::new()));
    let archetype_str = match profile.archetype {
        TaskArchetype::Summarization => "summarization",
        TaskArchetype::Extraction => "extraction",
        TaskArchetype::Reasoning => "reasoning",
        TaskArchetype::Code => "code",
        TaskArchetype::Chat => "chat",
    };

    if let Ok(read_guard) = cache.read() {
        if let Some(tier) = read_guard.get(archetype_str) {
            return *tier;
        }
    }

    let db_key = format!("routing:archetype:{}", archetype_str);
    if let Ok(Some(val)) = db.get_profile_key(&db_key).await {
        if let Some(tier) = parse_model_tier(&val) {
            if let Ok(mut write_guard) = cache.write() {
                write_guard.insert(archetype_str.to_string(), tier);
            }
            return tier;
        }
    }

    let mut score = match profile.archetype {
        TaskArchetype::Summarization => 20.0,
        TaskArchetype::Extraction => 40.0,
        TaskArchetype::Chat => 50.0,
        TaskArchetype::Code => 80.0,
        TaskArchetype::Reasoning => 90.0,
    };

    if profile.latency_sensitive {
        score -= 25.0;
    }

    if let Some(tokens) = profile.estimated_tokens {
        if tokens > 8000 {
            score += 30.0;
        } else if tokens > 4000 {
            score += 15.0;
        } else if tokens > 2000 {
            score += 5.0;
        } else if tokens < 500 {
            score -= 10.0;
        }
    }

    if std::env::var("MYTHRAX_FORCE_LOCAL").is_ok() {
        if score < 30.0 {
            ModelTier::Micro
        } else if score < 55.0 {
            ModelTier::Small
        } else if score < 75.0 {
            ModelTier::Medium
        } else if score < 95.0 {
            ModelTier::Large
        } else {
            ModelTier::Cloud
        }
    } else {
        ModelTier::Cloud
    }
}
