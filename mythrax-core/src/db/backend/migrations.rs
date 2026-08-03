use crate::db::backend::SurrealBackend;
use crate::db::schema::INIT_SCHEMA;
use crate::contracts::LlmConfigResponse;
use crate::db::backend::StorageBackend;
use anyhow::{Context, Result};

impl SurrealBackend {
    pub async fn init_db(&self) -> Result<()> {
        if self.is_client_mode() {
            return Ok(());
        }
        self.db
            .query(INIT_SCHEMA)
            .await?
            .check()
            .context("Applying schemas failed")?;

        // Purge ephemeral pipeline cluster state from previous terminated runs
        let _ = self.db.query("DELETE pipeline_cluster;").await;

        // Migration: Backfill legacy episodes where node_type is None
        let migration_sql =
            "UPDATE episode SET node_type = 'agent_thought' WHERE node_type = NONE;";
        let _ = self
            .db
            .query(migration_sql)
            .await?
            .check()
            .context("Failed to run legacy episode node_type migration")?;

        let idf_check = "SELECT count() AS total FROM idf_index GROUP ALL;";
        if let Ok(mut res) = self.db.query(idf_check).await {
            let count_val: Option<Vec<serde_json::Value>> = res.take(0).unwrap_or_default();
            let mut idf_count = 0;
            if let Some(arr) = count_val {
                if let Some(first) = arr.first() {
                    if let Some(n) = first.get("total").and_then(|v| v.as_u64()) {
                        idf_count = n as usize;
                    }
                }
            }
            if idf_count == 0 {
                tracing::info!("IDF index is empty. Spawning non-blocking background backfill...");
                let self_clone = self.clone();
                tokio::spawn(async move {
                    if let Err(e) = self_clone.backfill_idf_index_db().await {
                        tracing::error!("Failed to backfill IDF index: {:?}", e);
                    }
                });
            }
        }

        let hash_check = "SELECT count() AS total FROM episode WHERE content_hash IS NONE GROUP ALL;";
        if let Ok(mut res) = self.db.query(hash_check).await {
            let count_val: Option<Vec<serde_json::Value>> = res.take(0).unwrap_or_default();
            let mut missing_count = 0;
            if let Some(arr) = count_val {
                if let Some(first) = arr.first() {
                    if let Some(n) = first.get("total").and_then(|v| v.as_u64()) {
                        missing_count = n as usize;
                    }
                }
            }
            if missing_count > 0 {
                tracing::info!("Found {} episodes missing content_hash. Spawning background backfill...", missing_count);
                let self_clone = self.clone();
                tokio::spawn(async move {
                    if let Err(e) = self_clone.backfill_content_hashes_db().await {
                        tracing::error!("Failed to backfill content hashes: {:?}", e);
                    }
                });
            }
        }

        // Initialize default configuration if config:settings does not exist
        let check_sql = "SELECT * FROM config:settings;";
        let mut response = self
            .db
            .query(check_sql)
            .await?
            .check()
            .context("Check config failed")?;
        let config_opt: Option<LlmConfigResponse> = response.take(0)?;
        if config_opt.is_none() {
            let insert_sql = "
                CREATE config:settings CONTENT {
                    active_provider: 'local',
                    model: 'mlx-community/Qwen3.6-35B-A3B-4bit',
                    cloud_provider: 'gemini',
                    is_override: false,
                    expires_at: NONE
                };
            ";
            self.db
                .query(insert_sql)
                .await?
                .check()
                .context("Insert default config failed")?;
        }

        if let Some(ref path) = self.db_path {
            let marker = path.join(".initialized");
            if let Some(parent) = marker.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(marker, "initialized");
        }

        // Automatically load profile settings from bench_data/tuned_params.json if present
        let load_tuned = std::env::var("MYTHRAX_LOAD_TUNED_PARAMS")
            .map(|v| v != "false")
            .unwrap_or(true);
        if load_tuned {
            let mut tuned_path = std::path::PathBuf::from("bench_data/tuned_params.json");
            if !tuned_path.exists() {
                tuned_path = std::path::PathBuf::from("../bench_data/tuned_params.json");
            }
            if tuned_path.exists() {
                if let Ok(content) = std::fs::read_to_string(tuned_path) {
                    if let Ok(map) = serde_json::from_str::<
                        std::collections::HashMap<String, serde_json::Value>,
                    >(&content)
                    {
                        for (k, v) in map {
                            let val_str = match v {
                                serde_json::Value::String(s) => s,
                                other => other.to_string(),
                            };
                            if let Err(e) = self.save_profile_key(&k, &val_str).await {
                                tracing::warn!(
                                    "Failed to save profile key {} during initialization: {:?}",
                                    k,
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
