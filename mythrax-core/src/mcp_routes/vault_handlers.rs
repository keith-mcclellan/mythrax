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
            let scope = args.get("scope").and_then(|v| v.as_str()).map(|s| s.to_string());
            let async_mode = args.get("async_mode").and_then(|v| v.as_bool()).unwrap_or(true);

            let scope_clone = scope.clone();
            if async_mode {
                let state_clone = state.clone();
                tokio::spawn(async move {
                    let compactor = Compactor::new();
                    let coordinator = DreamCoordinator::new();
                    let embedder = if let Some(backend) = state_clone.backend.as_any().downcast_ref::<crate::db::backend::SurrealBackend>() {
                        backend.embedder.clone()
                    } else {
                        None
                    };

                    if let Err(e) = coordinator.run_dream(&*state_clone.backend, &state_clone.store, None, embedder.clone()).await {
                        tracing::error!("Background dream run failed: {:?}", e);
                    }

                    let scope_name = scope.as_deref().unwrap_or("general");
                    if let Err(e) = compactor.compact_scope(&*state_clone.backend, &state_clone.store, scope_name, embedder).await {
                        tracing::error!("Background compact_scope failed: {:?}", e);
                    }
                    if let Err(e) = compactor.compact_global(&*state_clone.backend, &state_clone.store).await {
                        tracing::error!("Background compact_global failed: {:?}", e);
                    }
                });

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Compaction and synthesis dreaming started in the background for scope '{}'.", scope_clone.as_deref().unwrap_or("general"))
                        }
                    ]
                }))
            } else {
                let compactor = Compactor::new();
                let coordinator = DreamCoordinator::new();
                let embedder = if let Some(backend) = state.backend.as_any().downcast_ref::<crate::db::backend::SurrealBackend>() {
                    backend.embedder.clone()
                } else {
                    None
                };

                coordinator.run_dream(&*state.backend, &state.store, None, embedder.clone()).await?;

                let scope_name = scope.as_deref().unwrap_or("general");
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
                let git_output = tokio::process::Command::new("git")
                    .args(["for-each-ref", "--format=%(refname:short) %(committerdate:unix)", "refs/heads/htr_branch_*"])
                    .current_dir(&repo_root)
                    .output()
                    .await;
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
                let _ = tokio::process::Command::new("git")
                    .args(["branch", "-D", branch])
                    .current_dir(&repo_root)
                    .status()
                    .await;
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
        "bootstrap" => {
            let dry_run = args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
            let since = args.get("since").and_then(|v| v.as_str()).map(|s| s.to_string());
            let scope_str = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string();
            let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
            let async_mode = args.get("async_mode").and_then(|v| v.as_bool()).unwrap_or(true);

            if async_mode {
                let state_clone = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = run_bootstrap_internal(state_clone, dry_run, since, scope_str, force).await {
                        tracing::error!("Background bootstrap failed: {:?}", e);
                    }
                });
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "Incremental bootstrap and conversation distillation started in the background. Cognitive tasks will be injected into your prompt stream as turns progress."
                        }
                    ]
                }))
            } else {
                unsafe { std::env::set_var("MYTHRAX_BOOTSTRAPPING", "1"); }
                let report_res = run_bootstrap_internal(state.clone(), dry_run, since, scope_str, force).await;
                unsafe { std::env::remove_var("MYTHRAX_BOOTSTRAPPING"); }
                let report = report_res?;
                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": report
                        }
                    ]
                }))
            }
        }
        _ => anyhow::bail!("Invalid action for manage_vault: {}", action),
    }
}

pub async fn handle_ingest_knowledge(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    let async_mode = args.get("async_mode").and_then(|v| v.as_bool()).unwrap_or(true);
    match action {
        "bulk" => {
            let source = args.get("source").and_then(|v| v.as_str()).context("Missing source")?;
            let harness = args.get("harness").and_then(|v| v.as_str()).context("Missing harness")?;
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general");
            
            if async_mode {
                let state_clone = state.clone();
                let source_clone = source.to_string();
                let harness_clone = harness.to_string();
                let scope_clone = scope.to_string();
                tokio::spawn(async move {
                    if let Err(e) = bulk_ingest_vault(
                        &state_clone.store.vault_root,
                        std::path::Path::new(&source_clone),
                        &harness_clone,
                        &scope_clone,
                        &*state_clone.backend
                    ).await {
                        tracing::error!("Background bulk ingestion failed: {:?}", e);
                    }
                });

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "Bulk ingestion started in the background. Ingestion will progress asynchronously."
                        }
                    ]
                }))
            } else {
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

            if async_mode {
                let state_clone = state.clone();
                let source_path_clone = source_path.to_string();
                let scope_clone = scope.to_string();
                let content_clone = content;
                let surreal_backend_clone = surreal_backend.clone();
                tokio::spawn(async move {
                    let surreal_backend_arc = Arc::new(surreal_backend_clone);
                    let forge = Forge::new(surreal_backend_arc, state_clone.store.clone());
                    if let Err(e) = forge.ingest_document(&content_clone, &scope_clone, &source_path_clone).await {
                        tracing::error!("Background forge ingestion failed: {:?}", e);
                    }
                });

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Document forging started in the background for '{}'.", source_path)
                        }
                    ]
                }))
            } else {
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

pub async fn run_bootstrap_internal(
    state: ApiState,
    dry_run: bool,
    since: Option<String>,
    scope_str: String,
    force: bool,
) -> Result<String> {
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required for bootstrap")?;

    let home_dir = std::env::var("HOME").context("HOME env var not set")?;
    let brain_dir = std::path::Path::new(&home_dir).join(".gemini/antigravity/brain");
    
    let mut processed_convs = 0;
    let mut distilled_count = 0;
    let mut skipped_count = 0;
    
    if brain_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(brain_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let conversation_id = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                    if conversation_id.starts_with('.') || conversation_id == "tempmediaStorage" {
                        continue;
                    }
                    
                    let mut already_processed = false;
                    if !force {
                        let check_query = "SELECT VALUE id FROM type::record('bootstrap_state', $id) LIMIT 1;";
                        if let Ok(mut resp) = surreal_backend.db.query(check_query).bind(("id", conversation_id.to_string())).await {
                            let processed: Option<surrealdb::types::RecordId> = resp.take(0).unwrap_or(None);
                            if processed.is_some() {
                                already_processed = true;
                            }
                        }
                    }
                    
                    let transcript_path = path.join(".system_generated/logs/transcript.jsonl");
                    if !transcript_path.exists() {
                        continue;
                    }
                    
                    if let Some(ref since_ts) = since {
                        if let Ok(metadata) = std::fs::metadata(&transcript_path) {
                            if let Ok(modified) = metadata.modified() {
                                let modified_dt: chrono::DateTime<chrono::Utc> = modified.into();
                                if let Ok(since_dt) = chrono::DateTime::parse_from_rfc3339(since_ts) {
                                    if modified_dt < since_dt.with_timezone(&chrono::Utc) {
                                        skipped_count += 1;
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    
                    if already_processed && !force {
                        skipped_count += 1;
                        continue;
                    }
                    
                    if !dry_run {
                        let client = crate::llm::LLMClient::new();
                        if let Ok(distilled_list) = crate::vault::distillation::distill_transcript_file(
                            state.backend.as_ref(),
                            &client,
                            &transcript_path,
                            conversation_id,
                            &scope_str
                        ).await {
                            for distilled in distilled_list {
                                let save_query = "
                                    CREATE type::record('distilled_conversation', $id) CONTENT {
                                        conversation_id: $conversation_id,
                                        title: $title,
                                        scope: $scope,
                                        timestamp: $timestamp,
                                        decisions: $decisions,
                                        constraints_discovered: $constraints_discovered,
                                        code_changes: $code_changes,
                                        commands_run: $commands_run,
                                        errors_resolved: $errors_resolved,
                                        user_preferences: $user_preferences,
                                        summary: $summary,
                                        key_takeaways: $key_takeaways
                                    };
                                ";
                                let rec_id = format!("distilled_conversation:{}", uuid::Uuid::new_v4());
                                let _ = surreal_backend.db.query(save_query)
                                    .bind(("id", rec_id))
                                    .bind(("conversation_id", distilled.conversation_id.clone()))
                                    .bind(("title", distilled.title.clone()))
                                    .bind(("scope", distilled.scope.clone()))
                                    .bind(("timestamp", distilled.timestamp.clone()))
                                    .bind(("decisions", distilled.decisions.clone()))
                                    .bind(("constraints_discovered", distilled.constraints_discovered.clone()))
                                    .bind(("code_changes", distilled.code_changes.clone()))
                                    .bind(("commands_run", distilled.commands_run.clone()))
                                    .bind(("errors_resolved", distilled.errors_resolved.clone()))
                                    .bind(("user_preferences", distilled.user_preferences.clone()))
                                    .bind(("summary", distilled.summary.clone()))
                                    .bind(("key_takeaways", distilled.key_takeaways.clone()))
                                    .await;
                                distilled_count += 1;
                            }
                        }
                        
                        let _ = crate::vault::distillation::ingest_artifacts_in_dir(
                            state.backend.as_ref(),
                            &path,
                            conversation_id,
                            &scope_str
                        ).await;
                        
                        let upsert_query = "UPSERT type::record('bootstrap_state', $id) SET processed_at = time::now();";
                        let _ = surreal_backend.db.query(upsert_query).bind(("id", conversation_id.to_string())).await;
                    }
                    
                    processed_convs += 1;
                }
            }
        }
    }
    
    let mut wisdom_count = 0;
    if !dry_run {
        if let Ok(w_count) = crate::vault::distillation::seed_wisdom_from_rules(
            state.backend.as_ref(),
            &state.store.vault_root
        ).await {
            wisdom_count = w_count;
        }
    }
    
    let report = format!(
        "Incremental bootstrap completed:\n- Processed conversations: {}\n- Skipped/Already processed: {}\n- Distilled chunks created: {}\n- Wisdom rules seeded: {}\n- Dry-run: {}",
        processed_convs, skipped_count, distilled_count, wisdom_count, dry_run
    );
    
    Ok(report)
}
