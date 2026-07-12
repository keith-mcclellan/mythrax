use serde_json::{json, Value};
use anyhow::{Result, Context};
use std::sync::Arc;
use crate::api::ApiState;
use crate::db::SurrealBackend;
use surrealdb_types::SurrealValue;
use crate::contracts::*;
use crate::cognitive::compactor::Compactor;
use crate::cognitive::synthesis::DreamCoordinator;
use crate::cognitive::forge::Forge;
use crate::vault::ingestion::bulk_ingest_vault;
use crate::verify::run_workspace_audit;

pub async fn handle_manage_vault(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    match action {
        "ingest_bulk" | "ingest_forge" | "save_forged_assets" => {
            let mut modified_args = args.clone();
            let new_action = match action {
                "ingest_bulk" => "bulk",
                "ingest_forge" => "forge",
                _ => "save_forged_assets",
            };
            if let Some(obj) = modified_args.as_object_mut() {
                obj.insert("action".to_string(), serde_json::Value::String(new_action.to_string()));
            }
            handle_ingest_knowledge(state, modified_args).await
        }
        "verify" => {
            let fix = args.get("fix").and_then(|v| v.as_bool()).unwrap_or(false);
            
            let synced_count = crate::vault::operations::sync_vault_to_db(&state.backend, &state.store).await?;
            
            let all_eps = state.backend.get_all_episodes().await?;
            let mut missing_count = 0;
            for ep in &all_eps {
                if let Some(ref vp) = ep.vault_path {
                    let path = state.store.vault_root.join(vp);
                    if !path.exists() {
                        missing_count += 1;
                        if fix {
                            let save = EpisodeSave::builder(ep.title.clone(), ep.content.clone())
                                .scope(ep.scope.clone())
                                .vault_path(Some(vp.clone()))
                                .source_episode(ep.source_episode.clone())
                                .node_type(ep.node_type.clone())
                                .build();
                            let markdown = crate::vault::watcher::format_episode_markdown(&save);
                            state.store.write_file(vp, &markdown)?;
                        }
                    }
                }
            }
            
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Vault integrity verification complete. Checked {} episodes. Missing files: {}. Fixed: {}. Synced from vault to DB: {} files.", all_eps.len(), missing_count, fix && missing_count > 0, synced_count)
                    }
                ]
            }))
        }
        "reprocess" => {
            let all_eps = state.backend.get_all_episodes().await?;
            let mut count = 0;
            for ep in all_eps {
                if ep.embedding.is_none() {
                    let save = EpisodeSave::builder(ep.title.clone(), ep.content.clone())
                        .scope(ep.scope.clone())
                        .vault_path(ep.vault_path.clone())
                        .source_episode(ep.source_episode.clone())
                        .node_type(ep.node_type.clone())
                        .build();
                    state.backend.save_episode(&save).await?;
                    count += 1;
                }
            }
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Reprocessed {} episodes with missing vector embeddings.", count)
                    }
                ]
            }))
        }
        "summarize" => {
            let scope = args.get("scope").and_then(|v| v.as_str());
            let compactor = Compactor::new();
            let coordinator = DreamCoordinator::new();
            let embedder = if let Some(backend) = state.backend.as_any().downcast_ref::<crate::db::backend::SurrealBackend>() {
                backend.embedder.clone()
            } else {
                None
            };

            coordinator.run_dream(&*state.backend, &state.store, None, embedder.clone()).await?;

            let scope_name = scope.unwrap_or("general");
            compactor.compact_scope(&*state.backend, &state.store, scope_name, embedder).await?;
            compactor.compact_global(&*state.backend, &state.store).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Compaction and synthesis dreaming completed successfully for scope '{}'.", scope_name)
                    }
                ]
            }))
        }
        "audit" => {
            let workspace_path_str = args.get("workspace_path").and_then(|v| v.as_str()).unwrap_or(".");
            let path = std::path::Path::new(workspace_path_str);
            let audit_results = run_workspace_audit(path).await;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!(
                            "Audit Results:\n- Search History Clean: {}\n- Daemon Health OK: {}\nViolations/Errors details: {:#?}",
                            audit_results.search_history_ok,
                            audit_results.daemon_ok,
                            audit_results
                        )
                    }
                ]
            }))
        }
        "clean" => {
            let dry_run = args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
            let confirm = args.get("confirm").and_then(|v| v.as_bool()).unwrap_or(false);

            // 1. Get stale sessions
            let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;
            
            // Query all distinct sessions from short_term_memory
            let sessions_query = "SELECT session_id FROM short_term_memory GROUP BY session_id;";
            let mut response = surreal_backend.db.query(sessions_query).await?.check()?;
            #[derive(serde::Deserialize, surrealdb_types::SurrealValue, Debug)]
            struct SessionRecord {
                session_id: String,
            }
            let session_records: Vec<SessionRecord> = response.take(0)?;
            
            let mut stale_sessions = Vec::new();
            let now = chrono::Utc::now();
            let stale_threshold = chrono::Duration::days(30);

            for rec in session_records {
                if let Ok(Some(last_activity)) = state.backend.get_session_last_activity(&rec.session_id).await {
                    if now - last_activity > stale_threshold {
                        stale_sessions.push((rec.session_id.clone(), last_activity));
                    }
                }
            }

            // 2. Get stale git branches
            let repo_root = crate::store::get_workspace_root()
                .unwrap_or_else(|| {
                    let mut p = state.store.vault_root.clone();
                    while p.parent().is_some() {
                        if p.join(".git").exists() {
                            return p;
                        }
                        p = p.parent().unwrap().to_path_buf();
                    }
                    state.store.vault_root.clone()
                });

            let mut stale_branches = Vec::new();
            if repo_root.join(".git").exists() {
                let git_output = std::process::Command::new("git")
                    .args(["for-each-ref", "--format=%(refname:short) %(committerdate:unix)", "refs/heads/htr_branch_*"])
                    .current_dir(&repo_root)
                    .output();
                if let Ok(output) = git_output {
                    let stdout_str = String::from_utf8_lossy(&output.stdout);
                    for line in stdout_str.lines() {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() == 2 {
                            let branch_name = parts[0];
                            if let Ok(timestamp) = parts[1].parse::<i64>() {
                                let commit_time = chrono::DateTime::from_timestamp(timestamp, 0)
                                    .unwrap_or_default();
                                if now.timestamp() - timestamp > 30 * 86400 {
                                    stale_branches.push((branch_name.to_string(), commit_time));
                                }
                            }
                        }
                    }
                }
            }

            // 3. Dry-run summary
            if dry_run || !confirm {
                let mut text = "Dry-Run Summary:\n".to_string();
                text.push_str("Stale Sessions:\n");
                for (sess, dt) in &stale_sessions {
                    text.push_str(&format!("- {} (last active: {})\n", sess, dt.to_rfc3339()));
                }
                if stale_sessions.is_empty() {
                    text.push_str("  None\n");
                }
                text.push_str("Stale Git Branches:\n");
                for (branch, dt) in &stale_branches {
                    text.push_str(&format!("- {} (last commit: {})\n", branch, dt.to_rfc3339()));
                }
                if stale_branches.is_empty() {
                    text.push_str("  None\n");
                }
                
                if !confirm {
                    text.push_str("\nTo proceed, run the clean command again with confirm=true.\n");
                }

                return Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ]
                }));
            }

            // 4. Perform actual cleanup
            for (sess, _) in &stale_sessions {
                let _ = state.backend.clear_stm(sess).await;
                let delete_queries = format!("
                    DELETE FROM chat_history WHERE session_id = '{}';
                    DELETE FROM belief_state WHERE session_id = '{}';
                    DELETE FROM handoff WHERE parent_conversation_id = '{}' OR subagent_conversation_id = '{}';
                ", sess, sess, sess, sess);
                let _ = surreal_backend.db.query(&delete_queries).await;
            }

            for (branch, _) in &stale_branches {
                let _ = std::process::Command::new("git")
                    .args(["branch", "-D", branch])
                    .current_dir(&repo_root)
                    .status();
            }

            let text = format!(
                "Cleanup Completed:\n- Deleted {} stale sessions from SurrealDB.\n- Deleted {} stale htr_branch_* git branches.",
                stale_sessions.len(),
                stale_branches.len()
            );

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for manage_vault: {}", action),
    }
}

pub async fn handle_ingest_knowledge(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    match action {
        "bulk" => {
            let source = args.get("source").and_then(|v| v.as_str()).context("Missing source")?;
            let harness = args.get("harness").and_then(|v| v.as_str()).context("Missing harness")?;
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general");
            
            let (count, errors) = bulk_ingest_vault(
                &state.store.vault_root,
                std::path::Path::new(source),
                harness,
                scope,
                &*state.backend
            ).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Ingested {} logs successfully. Errors: {:?}", count, errors)
                    }
                ]
            }))
        }
        "forge" => {
            let source_path = args.get("source").or_else(|| args.get("source_path")).and_then(|v| v.as_str()).context("Missing source")?;
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general");

            let source_path_buf = std::path::PathBuf::from(source_path);
            let content = if source_path_buf.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("pdf")) {
                crate::cognitive::forge::extract_pdf_text(&source_path_buf)?
            } else {
                std::fs::read_to_string(&source_path_buf)?
            };

            let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required for forge")?;

            let surreal_backend_arc = Arc::new(surreal_backend.clone());

            let forge = Forge::new(surreal_backend_arc, state.store.clone());
            forge.ingest_document(&content, scope, source_path).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Successfully forged source document '{}'", source_path)
                    }
                ]
            }))
        }
        "save_forged_assets" => {
            let batch: ForgedSectionBatch = serde_json::from_value(args.clone())
                .context("Failed to parse ForgedSectionBatch arguments")?;
            state.backend.save_forged_section(&batch).await?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Successfully saved forged assets for document '{}'", batch.doc_title)
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for ingest_knowledge: {}", action),
    }
}
