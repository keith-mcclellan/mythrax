use clap::{Parser, Subcommand};
use anyhow::Result;
use crate::db::backend::StorageBackend;
use crate::{contracts, vault, llm};

#[derive(Parser, Debug)]
#[command(name = "mythrax", version = "0.5.0", about = "Mythrax Local Memory and Cognitive Daemon CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize the Mythrax configuration and directories
    Init {
        /// Name of the harness to configure
        harness: Option<String>,
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Harness and LLM configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Daemon operations
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Check system status (DB connection, memory, config)
    Status,
    /// Save an episode or note from the CLI
    Save {
        /// Path to the markdown file to save
        #[arg(short, long)]
        file: String,
        /// Optional scope for the saved episode
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Query the memory store
    Search {
        /// The search query text
        query: String,
        /// Optional scope to filter by
        #[arg(short, long)]
        scope: Option<String>,
        /// Maximum number of results
        #[arg(short, long, default_value_t = 5)]
        limit: usize,
        /// Include raw episodes in search results
        #[arg(short, long)]
        episodes: bool,
    },
    /// Run safety compliance audits on the active directory
    Verify {
        /// Workspace directory to audit
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// Start the MCP server over stdin/stdout
    Mcp,
    /// Forge a source document to extract rules and wiki nodes
    Forge {
        /// Path to the source file (text, markdown, or PDF)
        source_path: String,
        /// Optional scope (defaults to 'general')
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Vault management and lifecycle operations
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    /// Hypothesis-Tree Refinement (Arbor) operations
    Htr {
        #[command(subcommand)]
        action: HtrAction,
    },
    /// Recover from a crash using dual-durability journals
    Recover {
        /// The session ID to recover
        #[arg(short, long)]
        session: String,
    },
    /// Merge duplicate or conflicting rules in .mythrax-shared
    MergeVault,
    /// Install the git pre-commit hook to sanitize secrets
    InstallHook,
    /// Run secret filtering on staged files (internal pre-commit hook)
    PreCommit,
    /// Run self-healing memory calibration audit
    Audit,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Configure Antigravity harness
    Antigravity {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure Claude Code harness
    Claude {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure Cursor harness
    Cursor {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure Codex harness
    Codex {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure OpenCode harness
    Opencode {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure OpenClaw harness
    Openclaw {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure Hermes harness
    Hermes {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure LLM model and cloud provider settings
    Llm {
        /// Provider type ('local' or 'cloud')
        #[arg(short, long)]
        provider: String,
        /// Duration ('temporary' or 'permanent')
        #[arg(short, long)]
        duration: Option<String>,
        /// Model identifier (e.g. 'gemini-1.5-flash', 'mlx-community/Qwen3.6-35B-A3B-4bit')
        #[arg(short, long)]
        model: Option<String>,
        /// Cloud provider name ('gemini', 'anthropic')
        #[arg(long)]
        cloud_provider: Option<String>,
        /// API Key for cloud access
        #[arg(long)]
        api_key: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum DaemonAction {
    /// Start the Mythrax background daemon
    Start {
        /// Port to bind the daemon REST API to
        #[arg(short, long, default_value_t = 8090)]
        port: u16,
        /// Path to the Obsidian vault to watch
        #[arg(short, long)]
        vault: Option<String>,
    },
    /// Run the Mythrax background daemon in the foreground
    Run {
        /// Port to bind the daemon REST API to
        #[arg(short, long, default_value_t = 8090)]
        port: u16,
        /// Path to the Obsidian vault to watch
        #[arg(short, long)]
        vault: Option<String>,
    },
    /// Stop the running background daemon using PID file
    Stop,
}

#[derive(Subcommand, Debug)]
pub enum VaultAction {
    /// Bulk ingest logs into the memory store
    Ingest {
        /// Path to the log directory or file
        #[arg(short, long)]
        source: String,
        /// Harness type (e.g. 'antigravity', 'claude', 'cursor', etc.)
        #[arg(short, long)]
        harness: String,
        /// Optional scope
        #[arg(long)]
        scope: Option<String>,
    },
    /// Organize the vault files by renaming and resolving duplicates
    Organize,
    /// Summarize episodes and generate system wisdom rules
    Summarize {
        /// Optional scope
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Verify vault integrity and run self-healing repairs
    Verify {
        /// Fix any issues found
        #[arg(short, long)]
        fix: bool,
    },
    /// Reprocess episodes with missing vector embeddings
    Reprocess,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HtrAction {
    /// Initialize HTR session root node
    Init {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        hypothesis: String,
        #[arg(short, long, value_delimiter = ',')]
        files: Vec<String>,
    },
    /// Propose child hypotheses (Ideation)
    Ideate {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        node: String,
    },
    /// Execute hypothesis node test run
    Execute {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        node: String,
        #[arg(short, long)]
        test_command: String,
    },
    /// Backpropagate test results and evaluation insights
    Backprop {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        node: String,
    },
    /// Apply and commit the selected node's changes to the codebase
    Merge {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        node: String,
    },
    /// Run the HTR loop end-to-end for a given hypothesis and codebase files
    Run {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        hypothesis: String,
        #[arg(short, long, value_delimiter = ',')]
        files: Vec<String>,
        #[arg(short, long)]
        test_command: String,
        #[arg(long, default_value_t = 5)]
        max_steps: usize,
    },
}

// Public CLI Action implementations to allow calling from tests and binary

pub async fn handle_merge_vault() -> Result<()> {
    let workspace_root = std::env::var("MYTHRAX_WORKSPACE_ROOT")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let shared_dir = workspace_root.join(".mythrax-shared");
    if !shared_dir.exists() {
        println!("No shared vault found at: {:?}", shared_dir);
        return Ok(());
    }

    println!("Scanning shared vault for rules...");
    let mut files = Vec::new();
    fn scan_dir(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                scan_dir(&path, files)?;
            } else if path.extension().map_or(false, |ext| ext == "md") {
                files.push(path);
            }
        }
        Ok(())
    }
    let _ = scan_dir(&shared_dir, &mut files);

    use std::collections::HashMap;
    let mut rules_group: HashMap<String, Vec<(std::path::PathBuf, contracts::WisdomRule)>> = HashMap::new();

    for file_path in &files {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            let (yaml_opt, _body) = vault::markdown::parse_frontmatter(&content);
            if let Some(yaml_val) = yaml_opt {
                if let Ok(frontmatter) = serde_json::from_value::<serde_json::Value>(serde_json::to_value(yaml_val).unwrap_or_default()) {
                    if let (Some(tp), Some(ata), Some(ce), Some(pr)) = (
                        frontmatter.get("target_pattern").and_then(|v| v.as_str()),
                        frontmatter.get("action_to_avoid").and_then(|v| v.as_str()),
                        frontmatter.get("causal_explanation").and_then(|v| v.as_str()),
                        frontmatter.get("prescribed_remedy").and_then(|v| v.as_str()),
                    ) {
                        let rule = contracts::WisdomRule {
                            id: None,
                            target_pattern: tp.to_string(),
                            action_to_avoid: ata.to_string(),
                            causal_explanation: ce.to_string(),
                            prescribed_remedy: pr.to_string(),
                            tier: frontmatter.get("tier").and_then(|v| v.as_str()).unwrap_or("dynamic").to_string(),
                            scope: frontmatter.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string(),
                            vault_path: Some(file_path.to_string_lossy().to_string()),
                            embedding: None,
                            source_episodes: Vec::new(),
                            generator_name: frontmatter.get("generator_name").and_then(|v| v.as_str()).unwrap_or("manual").to_string(),
                            similarity: None,
                            utility: frontmatter.get("utility").and_then(|v| v.as_f64()).map(|u| u as f32),
                        };
                        rules_group.entry(rule.target_pattern.clone()).or_default().push((file_path.clone(), rule));
                    }
                }
            }
        }
    }

    let conflict_archive = shared_dir.join("wisdom").join("conflict_archive");
    let proposed_dir = shared_dir.join("wisdom").join("proposed");
    std::fs::create_dir_all(&conflict_archive)?;
    std::fs::create_dir_all(&proposed_dir)?;

    for (pattern, rules) in rules_group {
        if rules.len() > 1 {
            println!("Conflict detected for target_pattern: '{}'", pattern);
            let mut actions = Vec::new();
            let mut explanations = Vec::new();
            let mut remedies = Vec::new();
            let mut max_utility = 50.0f32;
            let mut max_tier = "dynamic".to_string();
            let mut merged_scope = "general".to_string();

            for (_, r) in &rules {
                actions.push(r.action_to_avoid.clone());
                explanations.push(r.causal_explanation.clone());
                remedies.push(r.prescribed_remedy.clone());
                if let Some(u) = r.utility {
                    if u > max_utility {
                        max_utility = u;
                    }
                }
                if r.tier == "skills" || r.tier == "wisdom" {
                    max_tier = r.tier.clone();
                }
                if r.scope != "general" {
                    merged_scope = r.scope.clone();
                }
            }

            actions.sort(); actions.dedup();
            explanations.sort(); explanations.dedup();
            remedies.sort(); remedies.dedup();

            let action_to_avoid = actions.join("\n- ");
            let causal_explanation = explanations.join("\n- ");
            let prescribed_remedy = remedies.join("\n- ");

            let merged_rule = contracts::WisdomRule {
                id: None,
                target_pattern: pattern.clone(),
                action_to_avoid: format!("- {}", action_to_avoid),
                causal_explanation: format!("- {}", causal_explanation),
                prescribed_remedy: format!("- {}", prescribed_remedy),
                tier: max_tier,
                scope: merged_scope,
                vault_path: None,
                embedding: None,
                source_episodes: Vec::new(),
                generator_name: "conflict-resolver".to_string(),
                similarity: None,
                utility: Some(max_utility),
            };

            let frontmatter_str = vault::watcher::format_wisdom_markdown(&merged_rule);
            let merged_content = format!(
                "{}\n> [!WARNING]\n> This rule was automatically merged from conflicting duplicates. Please review and edit manually.\n",
                frontmatter_str.trim()
            );

            let slug = pattern.replace(|c: char| !c.is_alphanumeric(), "-").to_lowercase();
            let merged_filename = format!("{}-merged.md", slug);
            let merged_path = proposed_dir.join(&merged_filename);
            std::fs::write(&merged_path, &merged_content)?;
            println!("Saved unified rule to: {:?}", merged_path);

            for (path, _) in rules {
                let filename = path.file_name().unwrap();
                let dest = conflict_archive.join(filename);
                std::fs::rename(&path, &dest)?;
                println!("[Conflict Resolution: Rules merged for pattern '{}'. Original file archived under {:?}]", pattern, dest);
            }
        }
    }

    Ok(())
}

pub fn stringify_record_id(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(obj) => {
            if let (Some(tb), Some(id)) = (obj.get("tb"), obj.get("id")) {
                let id_str = match id {
                    serde_json::Value::String(s) => s.clone(),
                    _ => id.to_string(),
                };
                format!("{}:{}", tb.as_str().unwrap_or(""), id_str)
            } else {
                v.to_string()
            }
        }
        _ => v.to_string(),
    }
}

pub async fn run_auditor(backend: &crate::db::SurrealBackend) -> Result<()> {
    println!("Starting Auditor Self-Healing Memory Calibration...");
    
    let mut response = backend.db.query("SELECT id, title, content FROM episode ORDER BY rand() LIMIT 5;").await?;
    let episodes: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
    if episodes.is_empty() {
        println!("No episodes found to audit.");
        return Ok(());
    }

    let client = llm::LLMClient::new();

    for raw in episodes {
        let ep_id = stringify_record_id(raw.get("id").unwrap_or(&serde_json::Value::Null));
        if ep_id.is_empty() {
            continue;
        }
        println!("Auditing episode: {}...", ep_id);
        
        let title = raw.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let content = raw.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let prompt = format!(
            "Content:\nTitle: {}\nBody: {}\n\n\
             Generate a short, synthetic search query (1-2 sentences) that someone might type to find this memory. \
             Output ONLY the search query, without quotes or explanation.",
            title, content
        );

        let system_prompt = "You are a calibration assistant that generates synthetic search queries.";
        let synthetic_query = match client.completion(backend, Some(system_prompt), &prompt).await {
            Ok(res) => res.trim().to_string(),
            Err(e) => {
                println!("Failed to generate synthetic query: {:?}", e);
                continue;
            }
        };

        println!("Synthetic query: '{}'", synthetic_query);

        let config_query = "SELECT VALUE similarity_threshold FROM config:settings LIMIT 1;";
        let mut resp = backend.db.query(config_query).await?;
        let current_threshold: Option<f64> = resp.take(0).ok().and_then(|v: Vec<f64>| v.into_iter().next());
        let threshold = current_threshold.unwrap_or(0.55) as f32;

        let search_res = backend.search(
            &synthetic_query,
            Some("all"),
            false,
            10,
            0,
            threshold,
            None,
            false,
            true,
            false
        ).await?;

        let found = search_res.results.iter().any(|r| r.id == ep_id);
        if !found {
            println!("Calibration mismatch: Episode '{}' not found in search results with threshold {}.", ep_id, threshold);
            let new_threshold = (threshold - 0.05).max(0.20);
            println!("Calibrating: Decreasing threshold from {} to {}.", threshold, new_threshold);
            let update_sql = "UPSERT config:settings MERGE { similarity_threshold: $threshold };";
            let _ = backend.db.query(update_sql).bind(("threshold", new_threshold)).await;
        } else {
            println!("Calibration match: Episode '{}' successfully retrieved.", ep_id);
            let new_threshold = (threshold + 0.01).min(0.85);
            let update_sql = "UPSERT config:settings MERGE { similarity_threshold: $threshold };";
            let _ = backend.db.query(update_sql).bind(("threshold", new_threshold)).await;
        }
    }

    println!("Auditor calibration complete.");
    Ok(())
}
