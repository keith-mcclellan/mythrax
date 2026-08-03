use crate::api::ApiState;
use crate::contracts::*;
use crate::db::{SurrealBackend, parse_record_id};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::Path;
use surrealdb_types::SurrealValue;

pub async fn handle_manage(state: &ApiState, args: Value) -> Result<Value> {
    let action_opt = args.get("action").and_then(|v| v.as_str());
    let resolved_action = if let Some(act) = action_opt {
        act
    } else {
        if args.get("session_id").and_then(|v| v.as_str()).is_some() {
            "pre_invocation"
        } else if args
            .get("workspace_path")
            .and_then(|v| v.as_str())
            .is_some()
        {
            "audit_compliance"
        } else {
            anyhow::bail!("Missing action parameter for manage tool");
        }
    };

    let mapped_action = match resolved_action {
        "verify_vault" => "verify",
        "organize_vault" => "organize",
        "reprocess_vault" => "reprocess",
        "summarize_vault" => "summarize",
        "audit_compliance" => "audit",
        "init_htr" => "init",
        "ideate_htr" => "ideate",
        "execute_htr" => "execute",
        "backprop_htr" => "backprop",
        "merge_htr" => "merge",
        "run_htr" => "run",
        other => other,
    };

    match mapped_action {
        "sync_workspace" => {
            let ws_path_str = args
                .get("workspace_path")
                .or_else(|| args.get("source"))
                .and_then(|v| v.as_str())
                .unwrap_or("/Users/keith/Documents/mythrax");
            let ws_path = std::path::PathBuf::from(ws_path_str);
            crate::vault::ingestion::sync_workspace_docs_to_vault(&ws_path, &state.store, &*state.backend).await?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Synchronized workspace documentation and extracted AST code symbols for workspace at {:?}", ws_path)
                    }
                ]
            }))
        }
        "complete_handoff" => {
            let task_id = args
                .get("task_id")
                .and_then(|v| v.as_str())
                .context("Missing task_id")?;
            let vault_root = state.store.vault_root.clone();
            let abs_handoff_path = vault_root.join(format!(".handoffs/handoff_{}.md", task_id));

            if !abs_handoff_path.exists() {
                anyhow::bail!("Handoff contract not found: {}", task_id);
            }

            let mut content = std::fs::read_to_string(&abs_handoff_path)?;

            let status = args.get("status").and_then(|v| v.as_str());
            let fail_reason = args.get("fail_reason").and_then(|v| v.as_str());
            let mut failed = status == Some("failed");
            let mut computed_fail_reason = fail_reason.map(|s| s.to_string());

            if !failed {
                // validate outputs
                if content.starts_with("---\n") {
                    if let Some(end_idx) = content[4..].find("\n---") {
                        let yaml_str = &content[4..4 + end_idx];
                        if let Ok(contract) = serde_yaml::from_str::<HandoffContract>(yaml_str) {
                            let outputs_map = args.get("outputs").and_then(|v| v.as_object());
                            for output in &contract.outputs {
                                let has_val = outputs_map
                                    .map(|m| m.contains_key(&output.name))
                                    .unwrap_or(false);
                                if output.required && !has_val {
                                    failed = true;
                                    computed_fail_reason =
                                        Some(format!("missing_output: {}", output.name));
                                    break;
                                }
                                if has_val {
                                    let val = outputs_map.unwrap().get(&output.name).unwrap();

                                    // Enum validation
                                    if let Some(enums) = &output.enum_values {
                                        if let Some(val_str) = val.as_str() {
                                            if !enums.contains(&val_str.to_string()) {
                                                failed = true;
                                                computed_fail_reason = Some(format!(
                                                    "enum validation failed for: {}",
                                                    output.name
                                                ));
                                                break;
                                            }
                                        }
                                    }

                                    let mut val_str =
                                        serde_json::to_string(val).unwrap_or_default();
                                    const STM_VALUE_MAX_CHARS: usize = 32_000;
                                    if val_str.len() > STM_VALUE_MAX_CHARS {
                                        let original_len = val_str.len();
                                        val_str.truncate(STM_VALUE_MAX_CHARS);
                                        let msg = if let Some(path) = abs_handoff_path.to_str() {
                                            format!("... <Value truncated. Full value at: {}>", path)
                                        } else {
                                            format!("... <Value truncated from {} to {} chars>", original_len, STM_VALUE_MAX_CHARS)
                                        };
                                        val_str.push_str(&msg);
                                    }
                                    let key = format!("stm_{}_output_{}", task_id, output.name);
                                    let _ = state
                                        .backend
                                        .save_stm(&contract.parent_conversation_id, &key, &val_str)
                                        .await;
                                }
                            }
                        }
                    }
                }
            }

            let final_status = if failed { "failed" } else { "completed" };

            let re = regex::Regex::new(r"(?m)^status:\s*.*$").unwrap();
            content = re
                .replace(&content, format!("status: \"{}\"", final_status))
                .to_string();

            // Update database handoff status
            let db_status = if failed { "FAILED" } else { "COMPLETED" };
            let handoff_db_id = format!("handoff:{}", task_id);
            let _ = state
                .backend
                .update_handoff_status(&handoff_db_id, db_status)
                .await;

            if failed {
                if let Some(reason) = &computed_fail_reason {
                    let re_fail = regex::Regex::new(r"(?m)^fail_reason:\s*.*$").unwrap();
                    if re_fail.is_match(&content) {
                        content = re_fail
                            .replace(&content, format!("fail_reason: \"{}\"", reason))
                            .to_string();
                    } else {
                        // Insert after status if not present
                        content = content.replace(
                            &format!("status: \"{}\"", final_status),
                            &format!("status: \"{}\"\nfail_reason: \"{}\"", final_status, reason),
                        );
                    }
                }
            }

            std::fs::write(&abs_handoff_path, content)?;

            if failed {
                anyhow::bail!(
                    "Handoff completed with failure: {}",
                    computed_fail_reason.unwrap_or_default()
                );
            }

            return Ok(json!({ "status": "success" }));
        }
        "verify" | "organize" | "reprocess" | "reprocess_markdown" | "summarize" | "audit" | "ingest_bulk"
        | "ingest_forge" | "save_forged_assets" | "bootstrap" | "clean" | "reset_unprocessed" => {
            match mapped_action {
                "ingest_bulk" => {
                    let _source = args
                        .get("source")
                        .and_then(|v| v.as_str())
                        .context("Missing source parameter for ingest_bulk")?;
                    let _harness = args
                        .get("harness")
                        .and_then(|v| v.as_str())
                        .context("Missing harness parameter for ingest_bulk")?;
                }
                "ingest_forge" => {
                    let _source_path = args
                        .get("source")
                        .or_else(|| args.get("source_path"))
                        .and_then(|v| v.as_str())
                        .context("Missing source parameter for ingest_forge")?;
                }
                "save_forged_assets" => {
                    let _doc_title = args
                        .get("doc_title")
                        .context("Missing doc_title parameter for save_forged_assets")?;
                }
                _ => {}
            }
            let mut modified_args = args.clone();
            if let Some(obj) = modified_args.as_object_mut() {
                obj.insert(
                    "action".to_string(),
                    serde_json::Value::String(mapped_action.to_string()),
                );
            }
            super::vault_handlers::handle_manage_vault(state, modified_args).await
        }
        "tree_add_node" | "tree_update_node" | "tree_prune" | "tree_view" | "git_merge_branch" => {
            super::arbor_handlers::handle_manage_arbor(state, args).await
        }
        "init" | "ideate" | "execute" | "backprop" | "merge" | "run" => {
            let _scope = args
                .get("scope")
                .and_then(|v| v.as_str())
                .context("Missing scope parameter for HTR action")?;
            match mapped_action {
                "init" | "run" => {
                    let _hypothesis = args
                        .get("hypothesis")
                        .and_then(|v| v.as_str())
                        .context("Missing hypothesis parameter")?;
                }
                "ideate" | "execute" | "backprop" | "merge" => {
                    let _node_id = args
                        .get("node_id")
                        .and_then(|v| v.as_str())
                        .context("Missing node_id parameter")?;
                }
                _ => {}
            }
            let mut modified_args = args.clone();
            if let Some(obj) = modified_args.as_object_mut() {
                obj.insert(
                    "action".to_string(),
                    serde_json::Value::String(mapped_action.to_string()),
                );
            }
            super::htr_handlers::handle_manage_htr(state, modified_args).await
        }
        "extract" => {
            let doc_path = args.get("doc_path").and_then(|v| v.as_str()).context("Missing doc_path")?.to_string();
            if doc_path.contains("..") {
                anyhow::bail!("Path traversal disallowed in doc_path");
            }
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string();
            let backend = state.backend.clone();
            let full_doc_path = state.store.vault_root.join(&doc_path);
            let content = std::fs::read_to_string(full_doc_path).unwrap_or_default();
            let doc_path_task = doc_path.clone();
            tokio::spawn(async move {
                let llm_client = crate::llm::LLMClient::default();
                let _ = crate::cognitive::pipeline::extract_from_document(
                    backend.as_ref(),
                    Some(&llm_client),
                    &content,
                    &doc_path_task,
                    &scope,
                ).await;
            });
            Ok(json!({
                "content": [{ "type": "text", "text": format!("Started background fact extraction from document {}", doc_path) }]
            }))
        }
        "extract_code" => {
            let file_path = args.get("file_path").and_then(|v| v.as_str()).context("Missing file_path")?.to_string();
            if file_path.contains("..") {
                anyhow::bail!("Path traversal disallowed in file_path");
            }
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string();
            let backend = state.backend.clone();
            let content = std::fs::read_to_string(&file_path).unwrap_or_default();
            let file_path_task = file_path.clone();
            tokio::spawn(async move {
                let llm_client = crate::llm::LLMClient::default();
                let _ = crate::cognitive::pipeline::extract_from_code(
                    backend.as_ref(),
                    Some(&llm_client),
                    &content,
                    &file_path_task,
                    &scope,
                ).await;
            });
            Ok(json!({
                "content": [{ "type": "text", "text": format!("Started background fact extraction from code file {}", file_path) }]
            }))
        }
        "hypothesize" => {
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string();
            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<crate::db::backend::SurrealBackend>()
                .context("SurrealBackend required for hypothesize")?;

            let config = crate::cognitive::db::get_pipeline_config(&*state.backend).await?;
            let unassociated = crate::cognitive::db::get_unassociated_facts(&*state.backend, &scope).await?;
            if unassociated.len() < config.cluster_min_size {
                return Ok(json!({
                    "content": [{ "type": "text", "text": format!("Not enough unassociated facts for scope {} ({}/{})", scope, unassociated.len(), config.cluster_min_size) }]
                }));
            }

            let embeddings: Vec<Vec<f32>> = unassociated
                .iter()
                .map(|f| f.embedding.clone().unwrap_or_else(|| vec![0.0; 768]))
                .collect();
            let clusters = crate::cognitive::pipeline::cluster_facts(&unassociated, &embeddings, &config);
            let pruned_nodes = crate::cognitive::db::get_pruned_idea_nodes(&*state.backend, &scope, config.prune_threshold).await?;
            let pruned_constraints: Vec<String> = pruned_nodes.iter().map(|n| n.claim.clone()).collect();

            let mut queued = 0usize;
            for cluster in &clusters {
                let cluster_facts: Vec<&crate::contracts::Fact> = cluster.iter().map(|&idx| &unassociated[idx]).collect();
                let facts_summary = cluster_facts
                    .iter()
                    .enumerate()
                    .map(|(i, f)| format!("[{}] H: {} | Insight: {}", i, f.h_n().unwrap_or(""), f.iota_n().unwrap_or("")))
                    .collect::<Vec<String>>()
                    .join("\n");
                let (sys, user) = crate::cognitive::prompts::build_hypothesis_formation_prompt(&facts_summary, &pruned_constraints);
                let task_id = format!("cognitive_task:{}", uuid::Uuid::new_v4());
                let task = crate::db::CognitiveTask {
                    id: task_id.clone(),
                    task_type: "Synthesis".to_string(),
                    prompt: format!("[scope:{}] Form hypotheses from facts:\n{}", scope, user),
                    system_instruction: sys,
                    expected_format: "Json".to_string(),
                    priority: "Normal".to_string(),
                    created_at: chrono::Utc::now(),
                    status: "Pending".to_string(),
                    result: None,
                    ttl_minutes: 60,
                    injected_at: None,
                    session_id: Some(scope.clone()),
                };
                if surreal_backend.create_cognitive_task(&task).await.is_ok() {
                    queued += 1;
                    for fact in &cluster_facts {
                        let mut updated_fact = (*fact).clone();
                        updated_fact.idea_node_id = Some(format!("pending_{}", task_id));
                        let _ = crate::cognitive::db::save_fact(&*state.backend, &updated_fact).await;
                    }
                }
            }
            Ok(json!({
                "content": [{ "type": "text", "text": format!("Queued {} hypothesis formation tasks for scope {} ({} clusters from {} facts)", queued, scope, clusters.len(), unassociated.len()) }]
            }))
        }
        "refine" => {
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string();
            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<crate::db::backend::SurrealBackend>()
                .context("SurrealBackend required for refine")?;

            let pending_ideas = crate::cognitive::db::get_idea_nodes_by_scope(&*state.backend, &scope).await?;
            let facts = crate::cognitive::db::get_facts_by_scope(&*state.backend, &scope).await?;
            let _config = crate::cognitive::db::get_pipeline_config(&*state.backend).await?;

            let mut queued = 0usize;
            for idea in &pending_ideas {
                if idea.status == crate::contracts::IdeaStatus::Merged
                    || idea.status == crate::contracts::IdeaStatus::Pruned
                {
                    continue;
                }
                for fact in &facts {
                    if fact.idea_node_id.as_deref() != idea.id.as_deref() {
                        continue;
                    }
                    let fact_summary = format!("H: {} | Insight: {}", fact.h_n().unwrap_or(""), fact.iota_n().unwrap_or(""));
                    let (sys, user) = crate::cognitive::prompts::build_refinement_prompt(
                        &idea.claim,
                        &idea.insight,
                        idea.confidence,
                        &fact_summary,
                    );
                    let task_id = format!("cognitive_task:{}", uuid::Uuid::new_v4());
                    let task = crate::db::CognitiveTask {
                        id: task_id,
                        task_type: "Refinement".to_string(),
                        prompt: format!("[scope:{}] [idea:{}] Refine hypothesis:\n{}", scope, idea.id.as_deref().unwrap_or(""), user),
                        system_instruction: sys,
                        expected_format: "Json".to_string(),
                        priority: "Normal".to_string(),
                        created_at: chrono::Utc::now(),
                        status: "Pending".to_string(),
                        result: None,
                        ttl_minutes: 60,
                        injected_at: None,
                        session_id: Some(scope.clone()),
                    };
                    if surreal_backend.create_cognitive_task(&task).await.is_ok() {
                        queued += 1;
                    }
                }
            }
            Ok(json!({
                "content": [{ "type": "text", "text": format!("Queued {} refinement tasks for scope {} ({} pending ideas)", queued, scope, pending_ideas.len()) }]
            }))
        }
        "graduate" => {
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general");
            crate::db::graduation_pipeline::run_graduation_pipeline(&*state.backend, scope).await?;
            Ok(json!({
                "content": [{ "type": "text", "text": format!("Graduation pipeline complete for scope {}", scope) }]
            }))
        }
        "config" => {
            let cfg = crate::cognitive::db::get_pipeline_config(&*state.backend).await?;
            Ok(json!({
                "content": [{ "type": "text", "text": serde_json::to_string_pretty(&cfg).unwrap_or_default() }]
            }))
        }
        "pre_invocation" => {
            handle_pre_invocation_hook(state, args).await
        }
        "post_invocation" => {
            handle_post_invocation_hook(state, args).await
        }
        "precompact" => {
            let session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("global")
                .to_string();
            let fallback_path = format!(
                "/Users/keith/.gemini/antigravity/brain/{}/.system_generated/logs/transcript.jsonl",
                session_id
            );
            let transcript_path_str = args
                .get("transcript_path")
                .and_then(|v| v.as_str())
                .unwrap_or(&fallback_path)
                .to_string();

            let backend_clone = state.backend.clone();
            let store_clone = state.store.clone();
            let ignore_clone = state.ignore_list.clone();

            let session_id_clone = session_id.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::hooks::precompact::mine_transcript(
                    &session_id_clone,
                    &transcript_path_str,
                    backend_clone.as_ref(),
                    store_clone.as_ref(),
                    &ignore_clone,
                )
                .await
                {
                    tracing::error!("Background precompaction failed for session '{}': {:?}", session_id_clone, e);
                }
            });

            Ok(json!({ "status": "background_processing_started", "message": format!("Precompaction background process started for session {}", session_id) }))
        }
        "stop" => {
            let session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .context("Missing session_id parameter for stop")?;
            let transcript_path_str = args
                .get("transcript_path")
                .and_then(|v| v.as_str())
                .context("Missing transcript_path parameter for stop")?;
            let decision = crate::hooks::stop::mine_if_due(
                session_id,
                transcript_path_str,
                false,
                &state.backend,
                &state.store,
                &state.ignore_list,
            )
            .await?;
            let block = decision.is_some();
            let count = decision.unwrap_or(0);
            Ok(json!({ "status": "success", "block": block, "episodes_saved": count }))
        }
        "reflect" => {
            let session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .context("Missing session_id parameter for reflect")?;
            let transcript_path_str = args
                .get("transcript_path")
                .and_then(|v| v.as_str())
                .context("Missing transcript_path parameter for reflect")?;
            let status = crate::hooks::reflect::handle_reflect(
                session_id,
                transcript_path_str,
                state.backend.as_ref(),
            )
            .await?;
            Ok(json!({ "status": status }))
        }
        "audit_response" => {
            let response_text = args
                .get("response")
                .and_then(|v| v.as_str())
                .context("Missing response parameter for audit_response")?;
            let rules_path_opt = args.get("rules_path").and_then(|v| v.as_str());
            let session_id_opt = args.get("session_id").and_then(|v| v.as_str());
            let fail_on_violation = args
                .get("fail_on_violation")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            // 1. Read rules from rules_path if provided, else fallback to standard locations
            let mut rules_content = String::new();
            if let Some(rules_path) = rules_path_opt {
                if let Ok(content) = std::fs::read_to_string(rules_path) {
                    rules_content = content;
                } else {
                    tracing::warn!(
                        "Configured rules_path '{}' not found, falling back to default rules",
                        rules_path
                    );
                }
            }

            if rules_content.is_empty() {
                // Try workspace AGENTS.md first
                let workspace_root = std::env::var("MYTHRAX_WORKSPACE_ROOT")
                    .ok()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                let ws_agents_path = workspace_root.join(".agents").join("AGENTS.md");
                let global_agents_path =
                    std::path::PathBuf::from("/Users/keith/.gemini/config/AGENTS.md");

                if let Ok(content) = std::fs::read_to_string(&ws_agents_path) {
                    rules_content.push_str(&content);
                    rules_content.push_str("\n\n");
                }
                if let Ok(content) = std::fs::read_to_string(&global_agents_path) {
                    rules_content.push_str(&content);
                }
            }

            // Also query active database wisdom rules if session_id is provided
            if let Some(session_id) = session_id_opt {
                let scope = if session_id.contains('-') {
                    "general"
                } else {
                    session_id
                };
                let mut db_rules = Vec::new();
                let mut w_offset = 0;
                loop {
                    match state.backend.get_wisdom_rules_paginated(100, w_offset).await {
                        Ok(page) if !page.is_empty() => {
                            let count = page.len() as u32;
                            db_rules.extend(page);
                            w_offset += count;
                        }
                        _ => break,
                    }
                }
                let filtered: Vec<_> = db_rules.iter().filter(|r| r.scope == scope).collect();
                if !filtered.is_empty() {
                    rules_content.push_str("\n\n### Learned Wisdom Rules:\n");
                    for r in filtered {
                        rules_content.push_str(&format!(
                            "- Target: {}\n  Avoid: {}\n  Remedy: {}\n",
                            r.target_pattern, r.action_to_avoid, r.prescribed_remedy
                        ));
                    }
                }
            }

            // 2. Perform the LLM audit
            let system_instruction = "You are a rigid compliance auditor. Your job is to check the proposed agent response against the system operating rules and identify any violations. Respond with 'APPROVED' if no violations are found, otherwise list the violations clearly.";
            let prompt = format!(
                "Rules:\n{}\n\nProposed Response:\n{}\n\nDoes the proposed response follow all the rules? Respond with 'APPROVED' if compliant, or describe the violations.",
                rules_content, response_text
            );

            let model_opt = args.get("model").and_then(|v| v.as_str());
            let tier_opt = args.get("tier").and_then(|v| v.as_str());
            let use_cloud = model_opt == Some("cloud") || tier_opt == Some("cloud");

            let llm = crate::llm::LLMClient::default();
            let audit_res = if use_cloud && std::env::var("MYTHRAX_BOOTSTRAPPING").is_err() {
                let task_id = format!("cognitive_task:{}", uuid::Uuid::new_v4());
                let task = crate::db::CognitiveTask {
                    id: task_id.clone(),
                    task_type: "AuditResponse".to_string(),
                    prompt: prompt.clone(),
                    system_instruction: system_instruction.to_string(),
                    expected_format: "Any".to_string(),
                    priority: "High".to_string(),
                    created_at: chrono::Utc::now(),
                    status: "Pending".to_string(),
                    result: None,
                    ttl_minutes: 10,
                    injected_at: None,
                    session_id: session_id_opt.map(|s| s.to_string()),
                };

                let surreal_backend = state
                    .backend
                    .as_any()
                    .downcast_ref::<crate::db::backend::SurrealBackend>()
                    .context("SurrealBackend required for cognitive callback")?;

                surreal_backend.create_cognitive_task(&task).await?;

                let start = std::time::Instant::now();
                let timeout = std::time::Duration::from_secs(60);
                let mut completed_res = None;
                while start.elapsed() < timeout {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    if let Ok(Some(updated)) = surreal_backend.get_cognitive_task(&task_id).await {
                        if updated.status == "Completed" {
                            if let Some(res) = updated.result {
                                completed_res = Some(res);
                                break;
                            }
                        }
                    }
                }

                if let Some(res) = completed_res {
                    res
                } else {
                    tracing::warn!(
                        "Cognitive callback for response audit timed out, falling back to local model"
                    );
                    match llm
                        .completion_explicit(
                            state.backend.as_ref(),
                            "local",
                            "gemini",
                            "mlx-community/Qwen3.6-35B-A3B-4bit",
                            Some(system_instruction),
                            &prompt,
                            false,
                        )
                        .await
                    {
                        Ok(res) => res,
                        Err(_) => {
                            let config = state.backend.get_llm_config().await.unwrap_or_default();
                            let cloud_model = if config.cloud_provider == "gemini"
                                && (config.model.contains("Qwen") || config.model.is_empty())
                            {
                                "gemini-1.5-flash"
                            } else {
                                &config.model
                            };
                            llm.completion_explicit(
                                state.backend.as_ref(),
                                "cloud",
                                &config.cloud_provider,
                                cloud_model,
                                Some(system_instruction),
                                &prompt,
                                false,
                            )
                            .await
                            .unwrap_or_else(|_| "APPROVED".to_string())
                        }
                    }
                }
            } else {
                match llm
                    .completion_explicit(
                        state.backend.as_ref(),
                        "local",
                        "gemini",
                        "mlx-community/Qwen3.6-35B-A3B-4bit",
                        Some(system_instruction),
                        &prompt,
                        false,
                    )
                    .await
                {
                    Ok(res) => res,
                    Err(_) => {
                        let config = state.backend.get_llm_config().await.unwrap_or_default();
                        let cloud_model = if config.cloud_provider == "gemini"
                            && (config.model.contains("Qwen") || config.model.is_empty())
                        {
                            "gemini-1.5-flash"
                        } else {
                            &config.model
                        };
                        llm.completion_explicit(
                            state.backend.as_ref(),
                            "cloud",
                            &config.cloud_provider,
                            cloud_model,
                            Some(system_instruction),
                            &prompt,
                            false,
                        )
                        .await
                        .unwrap_or_else(|_| "APPROVED".to_string())
                    }
                }
            };

            let compliant = audit_res.trim().to_uppercase().contains("APPROVED")
                || audit_res.trim().to_uppercase() == "APPROVED";

            if !compliant && fail_on_violation {
                anyhow::bail!("Rule compliance audit failed:\n{}", audit_res);
            }

            Ok(json!({
                "status": "success",
                "compliant": compliant,
                "audit_report": audit_res
            }))
        }
        _ => anyhow::bail!("Invalid action for manage tool: {}", resolved_action),
    }
}

pub async fn handle_agent(state: &ApiState, args: Value) -> Result<Value> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .context("Missing action parameter for agent tool")?;
    let mapped_action = match action {
        "save_handoff" | "handoff" => "handoff",
        "complete_task" | "complete_code_task" => "complete_code_task",
        other => other,
    };
    match mapped_action {
        "handoff" => {
            let _parent = args
                .get("parent_conversation_id")
                .and_then(|v| v.as_str())
                .context("Missing parent_conversation_id")?;
            let _subagent = args
                .get("subagent_conversation_id")
                .and_then(|v| v.as_str())
                .context("Missing subagent_conversation_id")?;
            let _summary = args
                .get("summary")
                .and_then(|v| v.as_str())
                .context("Missing summary")?;
            handle_manage_stm(state, args).await
        }
        "complete_code_task" => {
            let prompt = args
                .get("prompt")
                .and_then(|v| v.as_str())
                .context("Missing prompt parameter")?;
            let sys_inst = args
                .get("system_instruction")
                .and_then(|v| v.as_str())
                .unwrap_or("You are a helpful AI coding assistant.");
            let model = args
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("mlx-community/Qwen3.6-35B-A3B-4bit");

            let llm = crate::llm::LLMClient::default();
            let result_text = llm
                .completion_explicit(
                    state.backend.as_ref(),
                    "local",
                    "gemini",
                    model,
                    Some(sys_inst),
                    prompt,
                    false,
                )
                .await
                .map_err(|e| anyhow::anyhow!("LLM completion failed for complete_code_task: {}", e))?;

            Ok(serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": result_text
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for agent tool: {}", action),
    }
}

pub async fn handle_manage_stm(state: &ApiState, args: Value) -> Result<Value> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .context("Missing action")?;
    match action {
        "put" => {
            let session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .context("Missing session_id")?;
            let key = args
                .get("key")
                .and_then(|v| v.as_str())
                .context("Missing key")?;
            let value = args
                .get("value")
                .and_then(|v| v.as_str())
                .context("Missing value")?;

            state.backend.save_stm(session_id, key, value).await?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Short-term memory saved for session '{}': {} = {}", session_id, key, value)
                    }
                ]
            }))
        }
        "get" => {
            let session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .context("Missing session_id")?;
            let key = args.get("key").and_then(|v| v.as_str());

            let map = state.backend.get_stm(session_id, key).await?;
            let text = match key {
                Some(k) => match map.get(k) {
                    Some(val) => val.clone(),
                    None => format!("Key '{}' not found in session '{}'", k, session_id),
                },
                None => serde_json::to_string_pretty(&map)?,
            };
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "clear" => {
            let session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .context("Missing session_id")?;

            state.backend.clear_stm(session_id).await?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Short-term memory cleared for session '{}'", session_id)
                    }
                ]
            }))
        }
        "handoff" => {
            let parent_conversation_id = args
                .get("parent_conversation_id")
                .and_then(|v| v.as_str())
                .context("Missing parent_conversation_id")?
                .to_string();
            let subagent_conversation_id = args
                .get("subagent_conversation_id")
                .and_then(|v| v.as_str())
                .context("Missing subagent_conversation_id")?
                .to_string();
            let summary = args
                .get("summary")
                .and_then(|v| v.as_str())
                .context("Missing summary")?
                .to_string();
            let handoff_file_path = args
                .get("handoff_file_path")
                .and_then(|v| v.as_str())
                .context("Missing handoff_file_path")?
                .to_string();
            let scope = args
                .get("scope")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let include_tool_execution =
                args.get("include_tool_execution").and_then(|v| v.as_bool());

            // Task 16: Typed I/O Contracts - Pre-launch validation
            let vault_root = state.store.vault_root.clone();
            let abs_handoff_path = if std::path::Path::new(&handoff_file_path).is_absolute() {
                std::path::PathBuf::from(&handoff_file_path)
            } else {
                vault_root.join(&handoff_file_path)
            };

            if abs_handoff_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&abs_handoff_path) {
                    if content.starts_with("---\n") {
                        if let Some(end_idx) = content[4..].find("\n---") {
                            let yaml_str = &content[4..4 + end_idx];
                            match serde_yaml::from_str::<HandoffContract>(yaml_str) {
                                Ok(contract) => {
                                    for input in &contract.inputs {
                                        if input.required && input.value.is_none() {
                                            anyhow::bail!("Missing required input: {}", input.name);
                                        }
                                        if let Some(val) = &input.value {
                                            let mut val_str =
                                                serde_json::to_string(val).unwrap_or_default();
                                            const STM_VALUE_MAX_CHARS: usize = 32_000;
                                            if val_str.len() > STM_VALUE_MAX_CHARS {
                                                let original_len = val_str.len();
                                                val_str.truncate(STM_VALUE_MAX_CHARS);
                                                let msg = if let Some(path) = abs_handoff_path.to_str() {
                                                    format!("... <Value truncated. Full value at: {}>", path)
                                                } else {
                                                    format!("... <Value truncated from {} to {} chars>", original_len, STM_VALUE_MAX_CHARS)
                                                };
                                                val_str.push_str(&msg);
                                            }
                                            let key = format!(
                                                "stm_{}_input_{}",
                                                contract.task_id, input.name
                                            );
                                            let _ = state
                                                .backend
                                                .save_stm(&subagent_conversation_id, &key, &val_str)
                                                .await;
                                        }
                                    }
                                }
                                Err(e) => anyhow::bail!("Malformed contract frontmatter: {}", e),
                            }
                        }
                    } else {
                        tracing::info!("No contract frontmatter found, skipping validation");
                    }
                }
            }

            let handoff = HandoffSave {
                parent_conversation_id: parent_conversation_id.clone(),
                subagent_conversation_id: subagent_conversation_id.clone(),
                summary,
                handoff_file_path: handoff_file_path.clone(),
                scope,
                include_tool_execution,
            };

            let id = state.backend.save_handoff(&handoff).await?;

            let event_ep = EpisodeSave::builder(
                "Handoff Event: Parent to Subagent".to_string(),
                format!(
                    "Handoff registered. Parent: {}, Subagent: {}, Summary: {}, File Path: {}",
                    parent_conversation_id,
                    subagent_conversation_id,
                    handoff.summary,
                    handoff.handoff_file_path
                ),
            )
            .scope(handoff.scope.clone())
            .session_id(Some(parent_conversation_id.clone()))
            .node_type(Some("handoff_event".to_string()))
            .build();
            if let Err(e) = state.backend.save_episode(&event_ep).await {
                tracing::error!("Operation failed: {:?}", e);
            }

            if let Ok(stm_map) = state
                .backend
                .get_stm(&parent_conversation_id, Some("_session_citations"))
                .await
            {
                if let Some(citations_str) = stm_map.get("_session_citations") {
                    if let Ok(episode_ids) = serde_json::from_str::<Vec<String>>(citations_str) {
                        if !episode_ids.is_empty() {
                            if let Ok(nodes_resp) =
                                state.backend.get_memory_nodes(&episode_ids).await
                            {
                                let mut footnote = String::new();
                                footnote.push_str("\n\n### Citations\n");
                                let vault_root = state.store.vault_root.clone();
                                for ep in nodes_resp.episodes {
                                    if let Some(ref vp) = ep.vault_path {
                                        let abs_path = vault_root.join(vp);
                                        footnote.push_str(&format!(
                                            "- [{}]((file://{}))\n",
                                            ep.title,
                                            abs_path.display()
                                        ));
                                    }
                                }

                                let abs_handoff_path =
                                    if std::path::Path::new(&handoff_file_path).is_absolute() {
                                        std::path::PathBuf::from(&handoff_file_path)
                                    } else {
                                        vault_root.join(&handoff_file_path)
                                    };

                                if abs_handoff_path.exists() {
                                    if let Ok(mut content) =
                                        std::fs::read_to_string(&abs_handoff_path)
                                    {
                                        if !content.contains("### Citations") {
                                            content.push_str(&footnote);
                                            let _ = std::fs::write(&abs_handoff_path, content);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Handoff saved successfully and related context nodes linked: {}", id)
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for manage_stm: {}", action),
    }
}

pub async fn handle_manage_config(state: &ApiState, args: Value) -> Result<Value> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .context("Missing action")?;
    match action {
        "get" => {
            let config = state.backend.get_llm_config().await?;
            let mut masked_config = serde_json::to_value(&config)?;
            if config.api_key.is_some() {
                masked_config["api_key"] = serde_json::Value::String("••••••••".to_string());
            }
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": serde_json::to_string_pretty(&masked_config)?
                    }
                ]
            }))
        }
        "set" => {
            let val_owned = args.get("value").map(|v| v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string()));
            if let (Some(k), Some(v)) = (
                args.get("key").and_then(|v| v.as_str()),
                val_owned.as_deref(),
            ) {
                state.backend.save_profile_key(k, v).await?;
                return Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Profile key '{}' set to '{}'.", k, v)
                        }
                    ]
                }));
            }

            let provider = args
                .get("provider")
                .and_then(|v| v.as_str())
                .context("Missing provider")?
                .to_string();
            let duration = args
                .get("duration")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let model = args
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let cloud_provider = args
                .get("cloud_provider")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let api_key = args
                .get("api_key")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let llm_post_inference_delay_ms =
                args.get("llm_post_inference_delay_ms").and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
                });

            let model_tier_mappings = args.get("model_tier_mappings").and_then(|v| {
                serde_json::from_value::<std::collections::HashMap<String, String>>(v.clone()).ok()
            });

            let req = LlmConfigRequest {
                provider,
                duration,
                model,
                cloud_provider,
                api_key,
                llm_post_inference_delay_ms,
                model_tier_mappings,
            };

            state.backend.update_llm_config(&req).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "LLM configuration updated successfully."
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for manage_config: {}", action),
    }
}

pub async fn handle_manage_file(state: &ApiState, args: Value) -> Result<Value> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .context("Missing action")?;

    let path = args
        .get("path")
        .or_else(|| args.get("AbsolutePath"))
        .or_else(|| args.get("TargetFile"))
        .and_then(|v| v.as_str())
        .context("Missing path/AbsolutePath/TargetFile")?;

    match action {
        "view" | "read" | "get_full" => {
            let start_line = args
                .get("start_line")
                .or_else(|| args.get("StartLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let end_line = args
                .get("end_line")
                .or_else(|| args.get("EndLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            let path_buf = Path::new(path);
            let content = std::fs::read_to_string(path_buf)?;

            let sliced_content = slice_content_by_lines(&content, start_line, end_line);

            let extension = get_extension(path_buf);
            let pageable_extensions = ["rs", "ts", "tsx", "js", "jsx", "py"];

            let final_content = if let Some(ref ext) = extension {
                if pageable_extensions.contains(&ext.as_str()) {
                    let surreal_backend = state
                        .backend
                        .as_any()
                        .downcast_ref::<SurrealBackend>()
                        .context("SurrealBackend required")?;
                    crate::cognitive::paging::page_code_block(surreal_backend, &sliced_content, ext)
                        .await?
                } else {
                    sliced_content
                }
            } else {
                sliced_content
            };

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": final_content
                    }
                ]
            }))
        }
        "replace" => {
            let target_content = args
                .get("target_content")
                .or_else(|| args.get("TargetContent"))
                .and_then(|v| v.as_str())
                .context("Missing target_content/TargetContent")?;
            let replacement_content = args
                .get("replacement_content")
                .or_else(|| args.get("ReplacementContent"))
                .and_then(|v| v.as_str())
                .context("Missing replacement_content/ReplacementContent")?;
            let start_line = args
                .get("start_line")
                .or_else(|| args.get("StartLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let end_line = args
                .get("end_line")
                .or_else(|| args.get("EndLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let allow_multiple = args
                .get("allow_multiple")
                .or_else(|| args.get("AllowMultiple"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let path_buf = Path::new(path);
            let file_content = std::fs::read_to_string(path_buf)?;

            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            let resolved_target = resolve_placeholders(surreal_backend, target_content).await;
            let resolved_replacement =
                resolve_placeholders(surreal_backend, replacement_content).await;

            let sliced_content = slice_content_by_lines(&file_content, start_line, end_line);

            let occurrences = sliced_content.matches(&resolved_target).count();
            if occurrences == 0 {
                anyhow::bail!("Target content not found in the file.");
            }
            if occurrences > 1 && !allow_multiple {
                anyhow::bail!(
                    "Target content found multiple times in the file, but AllowMultiple is false."
                );
            }

            let new_content = if start_line.is_some() || end_line.is_some() {
                let new_sliced = sliced_content.replace(&resolved_target, &resolved_replacement);

                let lines: Vec<&str> = file_content.lines().collect();
                let start_idx = start_line.map(|s| s.saturating_sub(1)).unwrap_or(0);
                let end_idx = end_line.map(|e| e.min(lines.len())).unwrap_or(lines.len());

                let mut new_lines: Vec<&str> = lines[..start_idx].to_vec();
                new_lines.extend(new_sliced.lines());
                new_lines.extend(lines[end_idx..].iter());

                new_lines.join("\n")
            } else {
                file_content.replace(&resolved_target, &resolved_replacement)
            };

            std::fs::write(path_buf, new_content)?;

            let rel_path = if let Ok(stripped) = path_buf.strip_prefix(&state.store.vault_root) {
                stripped.to_string_lossy().to_string()
            } else {
                path.to_string()
            };

            let artifact_ep = EpisodeSave::builder(
                format!(
                    "Artifact Edited: {}",
                    path_buf
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("file")
                ),
                format!("File updated successfully: {}", rel_path),
            )
            .scope(Some("general".to_string()))
            .vault_path(Some(rel_path))
            .files_modified(Some(vec![path.to_string()]))
            .node_type(Some("artifact_state".to_string()))
            .build();
            if let Err(e) = state.backend.save_episode(&artifact_ep).await {
                tracing::error!("Operation failed: {:?}", e);
            }

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "File updated successfully"
                    }
                ]
            }))
        }
        "multi_replace" => {
            let chunks = args
                .get("chunks")
                .or_else(|| args.get("ReplacementChunks"))
                .and_then(|v| v.as_array())
                .context("Missing/Invalid chunks/ReplacementChunks")?;

            let path_buf = Path::new(path);
            let mut file_content = std::fs::read_to_string(path_buf)?;

            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            for chunk in chunks {
                let target_content = chunk
                    .get("target_content")
                    .or_else(|| chunk.get("TargetContent"))
                    .and_then(|v| v.as_str())
                    .context("Missing target_content/TargetContent in chunk")?;
                let replacement_content = chunk
                    .get("replacement_content")
                    .or_else(|| chunk.get("ReplacementContent"))
                    .and_then(|v| v.as_str())
                    .context("Missing replacement_content/ReplacementContent in chunk")?;
                let start_line = chunk
                    .get("start_line")
                    .or_else(|| chunk.get("StartLine"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let end_line = chunk
                    .get("end_line")
                    .or_else(|| chunk.get("EndLine"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let allow_multiple = chunk
                    .get("allow_multiple")
                    .or_else(|| chunk.get("AllowMultiple"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let resolved_target = resolve_placeholders(surreal_backend, target_content).await;
                let resolved_replacement =
                    resolve_placeholders(surreal_backend, replacement_content).await;

                let sliced_content = slice_content_by_lines(&file_content, start_line, end_line);

                let occurrences = sliced_content.matches(&resolved_target).count();
                if occurrences == 0 {
                    anyhow::bail!("Target content not found in the file.");
                }
                if occurrences > 1 && !allow_multiple {
                    anyhow::bail!(
                        "Target content found multiple times in the file, but AllowMultiple is false."
                    );
                }

                let new_content = if start_line.is_some() || end_line.is_some() {
                    let new_sliced =
                        sliced_content.replace(&resolved_target, &resolved_replacement);

                    let lines: Vec<&str> = file_content.lines().collect();
                    let start_idx = start_line.map(|s| s.saturating_sub(1)).unwrap_or(0);
                    let end_idx = end_line.map(|e| e.min(lines.len())).unwrap_or(lines.len());

                    let mut new_lines: Vec<&str> = lines[..start_idx].to_vec();
                    new_lines.extend(new_sliced.lines());
                    new_lines.extend(lines[end_idx..].iter());

                    new_lines.join("\n")
                } else {
                    file_content.replace(&resolved_target, &resolved_replacement)
                };

                file_content = new_content;
            }

            std::fs::write(path_buf, file_content)?;

            let rel_path = if let Ok(stripped) = path_buf.strip_prefix(&state.store.vault_root) {
                stripped.to_string_lossy().to_string()
            } else {
                path.to_string()
            };

            let artifact_ep = EpisodeSave::builder(
                format!(
                    "Artifact Edited: {}",
                    path_buf
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("file")
                ),
                format!("File updated successfully: {}", rel_path),
            )
            .scope(Some("general".to_string()))
            .vault_path(Some(rel_path))
            .files_modified(Some(vec![path.to_string()]))
            .node_type(Some("artifact_state".to_string()))
            .build();
            if let Err(e) = state.backend.save_episode(&artifact_ep).await {
                tracing::error!("Operation failed: {:?}", e);
            }

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "File updated successfully with multiple changes"
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for manage_file: {}", action),
    }
}

pub async fn handle_post_invocation_hook(state: &ApiState, args: Value) -> Result<Value> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .context("Missing session_id for post_invocation")?;

    let exit_code = args.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0);
    let status = args.get("status").and_then(|v| v.as_str()).unwrap_or("success");
    let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let error_msg = args.get("error_message").and_then(|v| v.as_str());

    let status_info = serde_json::json!({
        "exit_code": exit_code,
        "status": status,
        "summary": summary,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    if let Err(e) = state
        .backend
        .save_stm(
            session_id,
            "_last_post_invocation_status",
            &serde_json::to_string(&status_info).unwrap_or_default(),
        )
        .await
    {
        tracing::error!("Failed to save post-invocation status to STM: {:?}", e);
    }

    if exit_code != 0 || status == "error" || status == "failed" {
        tracing::warn!("Post-invocation reported failure for session {}: exit_code={}, status={}", session_id, exit_code, status);
        let ep_content = format!(
            "Post-invocation failure reported.\nStatus: {}\nExit Code: {}\nSummary: {}\nError: {}",
            status,
            exit_code,
            summary,
            error_msg.unwrap_or("N/A")
        );
        let failure_ep = EpisodeSave::builder(
            format!("PostInvocation Failure: {}", session_id),
            ep_content,
        )
        .scope(Some("general".to_string()))
        .session_id(Some(session_id.to_string()))
        .node_type(Some("post_invocation_failure".to_string()))
        .build();

        if let Err(e) = state.backend.save_episode(&failure_ep).await {
            tracing::error!("Failed to save post-invocation failure episode: {:?}", e);
        }
    }

    Ok(serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": format!("Post invocation hook processed successfully for session: {}", session_id)
            }
        ]
    }))
}

pub async fn handle_pre_invocation_hook(state: &ApiState, args: Value) -> Result<Value> {
    let mut stm_str = String::new();
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("global");
    let caller = args.get("caller").and_then(|v| v.as_str());

    let surreal_backend = state
        .backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required for pre_invocation_hook")?;

    if caller == Some("distiller") {
        let now_unix = chrono::Utc::now().timestamp();
        let _ = state
            .backend
            .save_stm("global", "_distiller_heartbeat", &now_unix.to_string())
            .await;
        if session_id != "global" {
            let _ = state
                .backend
                .save_stm(session_id, "_distiller_heartbeat", &now_unix.to_string())
                .await;
        }
        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::mcp_routes::write_handlers::sweep_expired_tasks(&state_clone).await {
                tracing::error!("Operation failed: {:?}", e);
            }
        });

        let pending_tasks = surreal_backend.get_pending_cognitive_tasks().await?;
        let mut selected_tasks = Vec::new();
        let immediate_task = pending_tasks.iter().find(|t| t.priority == "Immediate");
        if let Some(t) = immediate_task {
            selected_tasks.push(t.clone());
        } else {
            let mut prioritized: Vec<_> = pending_tasks.iter().collect();
            prioritized.sort_by_key(|t| {
                match t.task_type.as_str() {
                    "Synthesis" => 0,
                    "Refinement" => 1,
                    "GraduateWisdom" => 2,
                    _ => 3,
                }
            });
            for t in prioritized.into_iter().take(3) {
                selected_tasks.push(t.clone());
            }
        }

        let mut callback_injection = String::new();
        if !selected_tasks.is_empty() {
            callback_injection.push_str("### 🧠 Pending Cognitive Callbacks\n");
            for task in &selected_tasks {
                callback_injection.push_str(&format!(
                    "- **Callback ID**: `{}`\n  - **Type**: {}\n  - **Prompt**: {}\n  - **System Instruction**: {}\n  - **Expected Format**: {}\n  - **Priority**: {}\n",
                    task.id, task.task_type, task.prompt, task.system_instruction, task.expected_format, task.priority
                ));
                surreal_backend
                    .update_cognitive_task_status(&task.id, crate::db::TaskStatus::Injected, None)
                    .await?;
            }
            callback_injection.push('\n');
        }

        return Ok(json!({
            "content": [
                {
                    "type": "text",
                    "text": callback_injection
                }
            ]
        }));
    }

    let mut total_discovery = 0u32;
    let mut total_read = 0u32;
    let mut has_discovery = false;

    let calc_tokens = |title: &str, content: &str, facts: Option<&[String]>| -> u32 {
        let mut len = title.len() + content.len();
        if let Some(f) = facts {
            if !f.is_empty() {
                if let Ok(json_str) = serde_json::to_string(f) {
                    len += json_str.len();
                }
            }
        }
        ((len + super::CHARS_PER_TOKEN - 1) / super::CHARS_PER_TOKEN) as u32
    };
    let query = args.get("query").and_then(|v| v.as_str());
    let workspace_path = args.get("workspace_path").and_then(|v| v.as_str());

    state
        .backend
        .journal_state(&state.store.vault_root, Some(session_id))
        .await?;

    let mut offset = 0;
    let limit = 500;
    loop {
        let page = state.backend.get_episodes_paginated(limit, offset).await?;
        if page.is_empty() {
            break;
        }
        for ep in &page {
            if let Some(ref vp) = ep.vault_path {
                let path = state.store.vault_root.join(vp);
                if !path.exists() {
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
        offset += limit;
    }

    let surreal_backend = state
        .backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required for pre_invocation_hook")?;

    // WU-4.5: TTL Sweep & LargeLocal Fallback
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::mcp_routes::write_handlers::sweep_expired_tasks(&state_clone).await {
            tracing::error!("Operation failed: {:?}", e);
        }
    });

    // WU-6.9: PagingManager context window paging
    let token_budget = 8000u32;
    let sql_session = "SELECT * FROM episode WHERE session_id = $session_id AND archived = false;";
    if let Ok(mut resp) = surreal_backend
        .db
        .query(sql_session)
        .bind(("session_id", session_id))
        .await
    {
        if let Ok(episodes) = resp.take::<Vec<crate::contracts::Episode>>(0) {
            let mut total_tokens = 0u32;
            let mut pm = crate::cognitive::memory_os::PagingManager::new(500);

            for ep in &episodes {
                let tokens = calc_tokens(&ep.title, &ep.content, ep.facts.as_deref());
                total_tokens += tokens;

                if let Some(ref id) = ep.id {
                    let pinned = ep.node_type.as_deref() == Some("user_input")
                        || ep.node_type.as_deref() == Some("task_checklist");
                    pm.access_node(
                        id.clone(),
                        crate::cognitive::memory_os::ActiveNodeInfo {
                            importance: ep.importance.unwrap_or(50.0),
                            node_type: "episode".to_string(),
                            pinned,
                        },
                    );
                }
            }

            if total_tokens > token_budget {
                let excess_tokens = total_tokens.saturating_sub(token_budget);
                let mut tokens_freed = 0u32;

                let mut evictable_episodes = episodes
                    .iter()
                    .filter(|ep| {
                        let is_pinned = ep.node_type.as_deref() == Some("user_input")
                            || ep.node_type.as_deref() == Some("task_checklist");
                        !is_pinned && ep.id.is_some()
                    })
                    .collect::<Vec<_>>();
                evictable_episodes.sort_by(|a, b| {
                    a.importance
                        .unwrap_or(50.0)
                        .partial_cmp(&b.importance.unwrap_or(50.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                for ep in evictable_episodes {
                    if tokens_freed >= excess_tokens {
                        break;
                    }
                    let tokens = calc_tokens(&ep.title, &ep.content, ep.facts.as_deref());
                    tokens_freed += tokens;

                    if let Some(ref id) = ep.id {
                        let id_raw = id.split(':').nth(1).unwrap_or(id).to_string();
                        let archive_sql = "UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() };";
                        tracing::warn!("Archiving episode {} due to token budget limits", id_raw);
                        if let Err(e) = surreal_backend
                            .db
                            .query(archive_sql)
                            .bind(("id", id_raw))
                            .await
                        {
                            tracing::error!("Failed to archive episode: {:?}", e);
                        }
                    }
                }
            }
        }
    }

    if let Some(q) = query {
        let insert_sql = "INSERT INTO chat_history { session_id: $session_id, role: 'user', content: $content, created_at: time::now() };";
        let _ = surreal_backend
            .db
            .query(insert_sql)
            .bind(("session_id", session_id))
            .bind(("content", q.to_string()))
            .await;
    }

    let mut loaded_belief_states: Vec<BeliefState> = Vec::new();
    let belief_res = surreal_backend.db.query("SELECT session_id, tasks_todo, hypotheses_tested, confidence_score, uncertainty_areas, updated_at FROM belief_state WHERE session_id = $session_id;")
        .bind(("session_id", session_id))
        .await;

    if let Ok(mut resp) = belief_res {
        loaded_belief_states = resp.take(0).unwrap_or_default();
    }

    let mut handoffs_resp = surreal_backend.db.query("SELECT parent_conversation_id, summary, scope FROM handoff WHERE subagent_conversation_id = $subagent AND status = 'PENDING';")
        .bind(("subagent", session_id))
        .await?;
    let handoffs: Vec<serde_json::Value> = handoffs_resp.take(0)?;

    let stm_map = state.backend.get_stm(session_id, None).await?;

    // A. Read last assistant turn and run observer/guardrail engine
    let mut last_assistant_turn = None;
    if let Some(path_str) = stm_map.get("_transcript_path") {
        if let Ok(file_content) = std::fs::read_to_string(path_str) {
            for line in file_content.lines().rev() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                    let is_assistant = val
                        .get("role")
                        .and_then(|r| r.as_str())
                        .map(|r| r == "assistant")
                        .unwrap_or(false)
                        || val
                            .get("source")
                            .and_then(|s| s.as_str())
                            .map(|s| s == "MODEL")
                            .unwrap_or(false);
                    if is_assistant {
                        if let Some(content_str) = val.get("content").and_then(|c| c.as_str()) {
                            last_assistant_turn = Some(content_str.to_string());
                            break;
                        }
                    }
                }
            }
        }
    }

    let mut guardrail_blocks = Vec::new();
    let mut blocking_directives = Vec::new();

    if let Some(ref turn_content) = last_assistant_turn {
        // 1. Memory utilization scoring (WU-3.1)
        let mut injected_nodes = Vec::new();
        if let Some(nodes_str) = stm_map.get("distilled_context_nodes") {
            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(nodes_str) {
                injected_nodes = parsed;
            } else {
                let cleaned =
                    nodes_str.trim_matches(|c| c == '[' || c == ']' || c == '"' || c == ' ');
                for part in cleaned.split(',') {
                    let part = part.trim().trim_matches('"');
                    if !part.is_empty() {
                        injected_nodes.push(part.to_string());
                    }
                }
            }
        }

        if !injected_nodes.is_empty() {
            let hydrated = state.backend.get_memory_nodes(&injected_nodes).await?;
            let mut utilized_count = 0;

            let turn_lower = turn_content.to_lowercase();

            for _wiki in &hydrated.wiki_nodes {
                utilized_count += 1;
            }

            for wisdom in &hydrated.wisdom_rules {
                utilized_count += 1;
                let id_part = wisdom
                    .id
                    .as_ref()
                    .map(|s| s.split(':').nth(1).unwrap_or(s))
                    .unwrap_or("");
                let is_cited = turn_lower.contains(&wisdom.target_pattern.to_lowercase())
                    || (!id_part.is_empty() && turn_content.contains(id_part));
                // EMA Reinforcement (WU-3.5): Cited memories reinforce upward (10.0), uncited decay (1.0).
                let target_imp = if is_cited { 10.0 } else { 1.0 };
                let current_imp = wisdom.importance.unwrap_or(5.0) as f32;
                let new_imp = 0.9 * current_imp + 0.1 * target_imp;
                let update_sql = "UPDATE type::record('wisdom', $id) SET importance = $imp;";
                let _ = surreal_backend
                    .db
                    .query(update_sql)
                    .bind(("id", id_part))
                    .bind(("imp", new_imp))
                    .await;
            }

            for ep in &hydrated.episodes {
                utilized_count += 1;
                let id_part = ep
                    .id
                    .as_ref()
                    .map(|s| s.split(':').nth(1).unwrap_or(s))
                    .unwrap_or("");
                let is_cited = turn_lower.contains(&ep.title.to_lowercase())
                    || (!id_part.is_empty() && turn_content.contains(id_part));
                // EMA Reinforcement (WU-3.5): Cited memories reinforce upward (10.0), uncited decay (1.0).
                let target_imp = if is_cited { 10.0 } else { 1.0 };
                let current_imp = ep.importance.unwrap_or(5.0) as f32;
                let new_imp = 0.9 * current_imp + 0.1 * target_imp;
                let update_sql = "UPDATE type::record('episode', $id) SET importance = $imp;";
                let _ = surreal_backend
                    .db
                    .query(update_sql)
                    .bind(("id", id_part))
                    .bind(("imp", new_imp))
                    .await;
            }

            let mem_util_score = (utilized_count * 100) / injected_nodes.len();
            let _ = state
                .backend
                .save_stm(
                    session_id,
                    "_last_memory_utilization",
                    &mem_util_score.to_string(),
                )
                .await;
        }

        // 2. Guardrail Engine rule violations (WU-3.2)
        let active_rules_res = surreal_backend
            .db
            .query("SELECT * FROM wisdom WHERE status = 'active';")
            .await;
        if let Ok(mut resp) = active_rules_res {
            let active_rules: Vec<WisdomRule> =
                if let Ok(raw_rules) = resp.take::<Vec<crate::db::backend::WisdomRaw>>(0) {
                    raw_rules
                        .into_iter()
                        .map(|r| r.into_wisdom_rule())
                        .collect()
                } else {
                    Vec::new()
                };

            let mut turn_embedding = None;
            if let Some(ref embedder) = surreal_backend.embedder {
                if let Ok(emb) = embedder.embed(turn_content).await {
                    turn_embedding = Some(emb);
                }
            }

            for rule in active_rules {
                let mut triggered = turn_content
                    .to_lowercase()
                    .contains(&rule.target_pattern.to_lowercase());

                if !triggered {
                    if let (Some(t_emb), Some(embedder)) = (&turn_embedding, &surreal_backend.embedder) {
                        if let Ok(r_emb) = embedder.embed(&rule.target_pattern).await {
                            let sim = crate::math::cosine_similarity(t_emb, &r_emb);
                            if sim > 0.82 { // threshold for semantic similarity
                                triggered = true;
                            }
                        }
                    } else if surreal_backend.embedder.is_none() && rule.tier == crate::contracts::Tier::Wisdom {
                        // Safety fallback: if embedder is unavailable, inject wisdom-tier rules
                        triggered = true;
                    }
                }

                if triggered {
                    let severity = rule
                        .severity
                        .clone()
                        .unwrap_or_else(|| "WARNING".to_string())
                        .to_uppercase();
                    let blocking = rule.blocking.unwrap_or(false);

                    if blocking {
                        blocking_directives.push(format!(
                            "> [!CAUTION]\n> **CRITICAL RULE ACKNOWLEDGEMENT REQUIRED**\n> You have triggered a blocking guardrail rule for `{}`.\n> Rule: Avoid `{}` because `{}`.\n> Remedy: `{}`.\n> You MUST explicitly state in your next turn how you will implement this remedy before proceeding!\n",
                            rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
                        ));
                    }

                    guardrail_blocks.push(format!(
                        "> [!{}]\n> **Rule Violation Alert**: Pertaining to `{}`\n> - **Avoid**: {}\n> - **Causal**: {}\n> - **Remedy**: {}\n",
                        severity, rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
                    ));
                }
            }
        }

        // 3. Auto Task Persistence (WU-3.3)
        let mut checklist_lines = Vec::new();
        for line in turn_content.lines() {
            if line.contains("- [ ]") || line.contains("- [x]") {
                checklist_lines.push(line.trim().to_string());
            }
        }
        if !checklist_lines.is_empty() {
            let checklist_str = checklist_lines.join("\n");
            let _ = state
                .backend
                .save_stm(session_id, "checklist", &checklist_str)
                .await;

            // Save as task_checklist episode
            let ep = EpisodeSave::builder("Active Task Checklist".to_string(), checklist_str)
                .scope(Some("general".to_string()))
                .session_id(Some(session_id.to_string()))
                .node_type(Some("task_checklist".to_string()))
                .build();
            if let Err(e) = state.backend.save_episode(&ep).await {
                tracing::error!("Operation failed: {:?}", e);
            }
        }
    }

    // 4. Memory Query Frequency Tracker (WU-3.4)
    let mut stale_search_warning = String::new();
    let now_unix = chrono::Utc::now().timestamp();
    if let Some(last_search_str) = stm_map.get("_last_search_time") {
        if let Ok(last_search_time) = last_search_str.parse::<i64>() {
            let elapsed = now_unix - last_search_time;
            if elapsed > 300 {
                stale_search_warning = format!(
                    "\n> [!IMPORTANT]\n> **MANDATORY MEMORY QUERY DIRECTIVE**\n> Mythrax memory search is stale (last search was {}s ago).\n> You MUST query Mythrax memory via call_mcp_tool read(action=\"search\", query=\"...\") or read(action=\"rules\", query=\"...\") to verify architectural context and guardrails before taking action.\n",
                    elapsed
                );
            }
        } else {
            stale_search_warning = "\n> [!IMPORTANT]\n> **MANDATORY MEMORY QUERY DIRECTIVE**\n> No memory search has been performed in this session yet.\n> You MUST query Mythrax memory via call_mcp_tool read(action=\"search\", query=\"...\") or read(action=\"rules\", query=\"...\") to verify architectural context and guardrails before taking action.\n".to_string();
        }
    } else {
        stale_search_warning = "\n> [!IMPORTANT]\n> **MANDATORY MEMORY QUERY DIRECTIVE**\n> No memory search has been performed in this session yet.\n> You MUST query Mythrax memory via call_mcp_tool read(action=\"search\", query=\"...\") or read(action=\"rules\", query=\"...\") to verify architectural context and guardrails before taking action.\n".to_string();
    }
    let mut parts = Vec::new();

    let mut broker_status = "### 🤖 Local Inference & Model Broker Status\n- **Broker State**: Offline or uninitialized\n\n".to_string();
    if let Some(broker) = crate::llm::DYNAMIC_MODEL_BROKER.get() {
        let active_tier_str = match broker.active_tier() {
            Some(tier) => format!("{:?}", tier),
            None => "None (Idle)".to_string(),
        };
        let emb_loaded = if broker.is_embedding_model_loaded() {
            "Loaded"
        } else {
            "Not Loaded"
        };
        let (model_name, execution_mode) = if let Some(weak_ref) = broker.get_weak_llm_reference() {
            if let Some(engine) = weak_ref.upgrade() {
                (engine.name(), engine.execution_mode())
            } else {
                ("None".to_string(), "cpu".to_string())
            }
        } else {
            ("None".to_string(), "cpu".to_string())
        };
        broker_status = format!(
            "### 🤖 Local Inference & Model Broker Status\n- **Active Tier**: `{}`\n- **Active Model Name**: `{}`\n- **Execution Mode**: `{}`\n- **Embedding Model**: `{}`\n\n",
            active_tier_str, model_name, execution_mode, emb_loaded
        );
    }
    parts.push(broker_status);

    let mut insights_parts = Vec::new();
    if !handoffs.is_empty() {
        let active_handoff = &handoffs[0];
        let parent_conversation_id = active_handoff
            .get("parent_conversation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let summary = active_handoff
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let scope = active_handoff.get("scope").and_then(|v| v.as_str());

        parts.push(format!(
            "### 📌 Handoff Metadata\n- **Parent Conversation**: `{}`\n- **Summary**: {}\n",
            parent_conversation_id, summary
        ));

        let mut stm_parts = Vec::new();
        for (k, v) in &stm_map {
            if k != "distilled_context_nodes" && !k.starts_with('_') {
                stm_parts.push(format!("- **{}**: {}", k, v));
            }
        }
        if !stm_parts.is_empty() {
            stm_str = format!(
                "### 🔑 Stashed Session Variables\n{}\n",
                stm_parts.join("\n")
            );
        }

        let mut node_ids = Vec::new();
        if let Some(nodes_str) = stm_map.get("distilled_context_nodes") {
            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(nodes_str) {
                node_ids = parsed;
            } else if let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(nodes_str) {
                for val in values {
                    if let Some(s) = val.as_str() {
                        node_ids.push(s.to_string());
                    }
                }
            } else {
                let cleaned =
                    nodes_str.trim_matches(|c| c == '[' || c == ']' || c == '"' || c == ' ');
                for part in cleaned.split(',') {
                    let part = part.trim().trim_matches('"');
                    if !part.is_empty() {
                        node_ids.push(part.to_string());
                    }
                }
            }
        }

        if !node_ids.is_empty() {
            let hydrated = state.backend.get_memory_nodes(&node_ids).await?;

            for wiki in hydrated.wiki_nodes {
                total_read += calc_tokens(&wiki.name, &wiki.content, None);
                insights_parts.push(format!(
                    "**Distilled Insight: {}**\n{}",
                    wiki.name, wiki.content
                ));
            }
            for wisdom in hydrated.wisdom_rules {
                let rule_content = format!(
                    "Avoid: {}\nCausal: {}\nRemedy: {}",
                    wisdom.action_to_avoid, wisdom.causal_explanation, wisdom.prescribed_remedy
                );
                total_read += calc_tokens(&wisdom.target_pattern, &rule_content, None);
                insights_parts.push(format!(
                    "**Wisdom Rule: {}**\n- Avoid: {}\n- Causal: {}\n- Remedy: {}",
                    wisdom.target_pattern,
                    wisdom.action_to_avoid,
                    wisdom.causal_explanation,
                    wisdom.prescribed_remedy
                ));
            }
            for ep in hydrated.episodes {
                if ep.discovery_tokens.is_some() {
                    has_discovery = true;
                }
                if let Some(dt) = ep.discovery_tokens {
                    total_discovery += dt;
                }
                total_read += calc_tokens(&ep.title, &ep.content, ep.facts.as_deref());
                if let Some(ref ep_id) = ep.id {
                    let rendered = super::format_episode_or_parent(
                        &*state.backend,
                        &surreal_backend.db,
                        ep_id,
                        &ep.title,
                        &ep.content,
                        ep.scope.as_deref(),
                    )
                    .await?;
                    insights_parts.push(rendered);
                }
            }
        } else {
            let search_res = state
                .backend
                .search(crate::contracts::SearchParams::from_positional(
                    summary, scope, false, 15, 0, 0.55, None, false, true, false, None, true, None,
                ))
                .await?;

            for res in search_res.results {
                if res.discovery_tokens.is_some() {
                    has_discovery = true;
                }
                if let Some(dt) = res.discovery_tokens {
                    total_discovery += dt;
                }
                total_read += calc_tokens(&res.title, &res.content, None);
                if let Some(formatted) =
                    format_search_result_hybrid(surreal_backend, &res, state).await?
                {
                    insights_parts.push(formatted);
                }
            }
        }
    } else {
        let workspace_path_str = workspace_path.map(|s| s.to_string()).unwrap_or_else(|| {
            std::env::var("MYTHRAX_WORKSPACE_ROOT").unwrap_or_else(|_| ".".to_string())
        });
        let path = std::path::Path::new(&workspace_path_str);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let folder_name = canonical
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("general")
            .to_string();
        let dynamic_scope = folder_name.clone();

        let extracted_query = if let Some(q) = query {
            q.to_string()
        } else {
            let mut extracted = None;
            if let Some(path_str) = stm_map.get("_transcript_path") {
                if let Ok(file_content) = std::fs::read_to_string(path_str) {
                    for line in file_content.lines().rev() {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                            let is_user = val.get("role").and_then(|r| r.as_str()).map(|r| r == "user").unwrap_or(false);
                            if is_user {
                                if let Some(c) = val.get("content").and_then(|c| c.as_str()) {
                                    extracted = Some(c.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            if extracted.is_none() {
                if let Some(c) = stm_map.get("objective").or_else(|| stm_map.get("task_description")) {
                    extracted = Some(c.to_string());
                }
            }
            extracted.unwrap_or_else(|| format!("{} project context", folder_name))
        };
        let search_res = state
            .backend
            .search(crate::contracts::SearchParams::from_positional(
                &extracted_query,
                Some(&dynamic_scope),
                false,
                15,
                0,
                0.55,
                None,
                false,
                true,
                false,
                Some(session_id),
                true,
                None,
            ))
            .await?;

        let mut high_confidence_memories_found = false;
        for res in search_res.results {
            if res.id.starts_with("episode:") && res.similarity >= 0.80 {
                high_confidence_memories_found = true;
            }
            if res.discovery_tokens.is_some() {
                has_discovery = true;
            }
            if let Some(dt) = res.discovery_tokens {
                total_discovery += dt;
            }
            total_read += calc_tokens(&res.title, &res.content, None);
            if let Some(formatted) =
                format_search_result_hybrid(surreal_backend, &res, state).await?
            {
                insights_parts.push(formatted);
            }
        }

        if !high_confidence_memories_found {
            parts.push(format!(
                "\n> [!IMPORTANT]\n> **Pinned Deep-Search Instruction**: No high-confidence memory episodes were found. If you need deeper historical context or past resolutions, please call read(action=\"search\", query=\"...\") with a specific query.\n"
            ));
        }
    }

    let _active_node_opt = stm_map
        .get("active_hypothesis_node")
        .or_else(|| stm_map.get("active_node"))
        .cloned();

    let current_scope = std::env::var("MYTHRAX_WORKSPACE_ROOT").unwrap_or_else(|_| ".".to_string());
    let path_buf = std::path::Path::new(&current_scope);
    let canonical = path_buf
        .canonicalize()
        .unwrap_or_else(|_| path_buf.to_path_buf());
    let folder_name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("general")
        .to_string();

    let arbor_res = super::arbor_handlers::handle_manage_arbor(
        state,
        serde_json::json!({
            "action": "tree_view",
            "format": "constraints",
            "scope": folder_name
        }),
    )
    .await;
    let p0_policy = if let Ok(res) = arbor_res {
        if let Some(text) = res.get("content").and_then(|c| c.get(0)).and_then(|item| item.get("text")).and_then(|t| t.as_str()) {
            if !text.contains("(No active negative constraints found)") {
                let rules_body = text.strip_prefix("### Negative Constraints & Guardrails\n").unwrap_or(text);
                format!("### 🚫 Policy (Non-Negotiable Rules)\n{}\n\n", rules_body.trim())
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    let mut p1_advisory = collect_advisory_context(
        surreal_backend,
        &folder_name,
        &loaded_belief_states,
        &insights_parts,
    )
    .await;

    let count_tokens = |text: &str| -> usize {
        if let Some(ref embedder) = surreal_backend.embedder {
            embedder
                .count_tokens(text)
                .unwrap_or_else(|_| text.split_whitespace().count())
        } else {
            text.split_whitespace().count()
        }
    };

    let budget_env =
        std::env::var("MYTHRAX_PRE_INVOCATION_TOKEN_BUDGET").unwrap_or_else(|_| "32000".to_string());
    let token_budget: usize = budget_env.parse().unwrap_or(32000);

    // Broker/Handoff metadata is in `parts`
    let preamble = parts.join(
        "
",
    );
    let mut p2_stm = stm_str.clone();

    let base_playbook = "### 💡 Mythrax Skill Playbook & Memory Search Reminder
> [!IMPORTANT]
> **MEMORY SEARCH FIRST MANDATE**: Before executing code changes, shell commands, or plan steps, you MUST query Mythrax memory using `read(action=\"search\", query=\"...\")` or `read(action=\"rules\", query=\"...\")` to recall active project context, architectural guidelines, and negative constraints.
> **Skill Reference**: Refer to the `/mythrax` skill (`/Users/keith/.gemini/config/skills/mythrax/SKILL.md` or `.agents/skills/mythrax/SKILL.md`) for MCP tool signatures (`read`, `write`, `manage`, `agent`).

";

    if caller != Some("distiller") {
        loop {
            let current_total = count_tokens(base_playbook)
                + count_tokens(&preamble)
                + count_tokens(&p0_policy)
                + count_tokens(&p1_advisory)
                + count_tokens(&p2_stm);

            if current_total <= token_budget {
                break;
            }

            if !p1_advisory.is_empty() {
                let sections: Vec<&str> = p1_advisory.split("> [!TIP]").collect();
                if sections.len() > 2 {
                    let mut valid_sections = Vec::new();
                    for (i, s) in sections.iter().enumerate() {
                        if i == 0 && s.trim().is_empty() { continue; }
                        let full_sec = if i > 0 { format!("> [!TIP]{}", s) } else { s.to_string() };
                        valid_sections.push(full_sec);
                    }
                    if valid_sections.len() > 1 {
                        let total_count = valid_sections.len();
                        valid_sections.pop(); // drop the lowest-priority section
                        let dropped_count = total_count - valid_sections.len();
                        p1_advisory = valid_sections.join("");
                        tracing::warn!("Pre-invocation truncated {} of {} advisory sections to fit token budget", dropped_count, total_count);
                    } else if !p2_stm.is_empty() {
                        let lines: Vec<&str> = p2_stm.lines().collect();
                        if lines.len() > 1 {
                            let truncated_lines = &lines[..lines.len() - 1];
                            p2_stm = truncated_lines.join("\n");
                            tracing::warn!("Pre-invocation truncated STM working memory to fit token budget");
                        } else {
                            p2_stm.clear();
                        }
                    } else {
                        break;
                    }
                } else if !p2_stm.is_empty() {
                    let lines: Vec<&str> = p2_stm.lines().collect();
                    if lines.len() > 1 {
                        let truncated_lines = &lines[..lines.len() - 1];
                        p2_stm = truncated_lines.join("\n");
                        tracing::warn!("Pre-invocation truncated STM working memory to fit token budget");
                    } else {
                        p2_stm.clear();
                    }
                } else {
                    break;
                }
            } else if !p2_stm.is_empty() {
                p2_stm.clear();
            } else {
                break; // Can't truncate P0 or preamble
            }
        }
    }

    let user_directions_section = {
        let sql = "SELECT name, content FROM wiki_node WHERE node_type = 'direction' OR item_type = 'direction' LIMIT 5;";
        if let Ok(mut resp) = surreal_backend.db.query(sql).await {
            #[derive(serde::Deserialize, surrealdb::types::SurrealValue)]
            struct DirRow {
                name: String,
                content: String,
            }
            if let Ok(rows) = resp.take::<Vec<DirRow>>(0) {
                if !rows.is_empty() {
                    let mut text = String::from("### 🎯 Active User Directions\n");
                    for row in rows {
                        let snippet = row.content.lines().next().unwrap_or(&row.content);
                        text.push_str(&format!("- **{}**: {}\n", row.name, snippet));
                    }
                    text.push_str("\n");
                    text
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };

    let initial_context = {
        let mut base = String::new();
        base.push_str(base_playbook);
        base.push_str(&preamble);
        base.push_str(&p0_policy);
        if !user_directions_section.is_empty() {
            base.push_str(&user_directions_section);
        }
        if !p1_advisory.is_empty() {
            base.push_str(&p1_advisory);
        }
        if !p2_stm.is_empty() {
            base.push_str(&p2_stm);
        }
        base
    };

    let context_tokens = count_tokens(&initial_context);

    let mut allowed_history = Vec::new();
    let mut history_tokens = 0;

    let chat_res = surreal_backend.db.query("SELECT role, content, created_at FROM chat_history WHERE session_id = $session_id ORDER BY created_at DESC LIMIT 10;")
        .bind(("session_id", session_id))
        .await;

    match chat_res {
        Ok(mut resp) => {
            #[derive(serde::Deserialize, Debug, SurrealValue)]
            struct ChatTurn {
                role: String,
                content: String,
            }
            if let Ok(turns) = resp.take::<Vec<ChatTurn>>(0) {
                for turn in turns {
                    let turn_str = format!(
                        "- **{}**: {}\n",
                        if turn.role == "user" {
                            "User"
                        } else {
                            "Assistant"
                        },
                        turn.content
                    );
                    let turn_tokens = count_tokens(&turn_str);
                    if context_tokens + history_tokens + turn_tokens <= 2048 {
                        history_tokens += turn_tokens;
                        allowed_history.push(turn_str);
                    } else {
                        break;
                    }
                }
            }
        }
        Err(_) => {}
    }

    allowed_history.reverse();
    let mut history_part = String::new();
    if !allowed_history.is_empty() {
        history_part.push_str("### 💬 Conversational Turn History\n");
        for turn_str in allowed_history {
            history_part.push_str(&turn_str);
        }
        history_part.push('\n');
    }

    let mut final_context = format!("{}{}", history_part, initial_context);

    // WU-4.2: Pre-Invocation Injection for Pending Tasks
    let pending_tasks = surreal_backend.get_pending_cognitive_tasks().await?;
    let mut selected_tasks = Vec::new();
    let immediate_task = pending_tasks.iter().find(|t| t.priority == "Immediate");
    if let Some(t) = immediate_task {
        selected_tasks.push(t.clone());
    } else {
        for t in pending_tasks
            .iter()
            .filter(|t| t.priority != "Immediate")
            .take(3)
        {
            selected_tasks.push(t.clone());
        }
    }

    let mut callback_injection = String::new();
    if !selected_tasks.is_empty() {
        callback_injection.push_str("### 🧠 Pending Cognitive Callbacks\n");
        for task in &selected_tasks {
            callback_injection.push_str(&format!(
                "- **Callback ID**: `{}`\n  - **Type**: {}\n  - **Prompt**: {}\n  - **System Instruction**: {}\n  - **Expected Format**: {}\n  - **Priority**: {}\n",
                task.id, task.task_type, task.prompt, task.system_instruction, task.expected_format, task.priority
            ));
            surreal_backend
                .update_cognitive_task_status(&task.id, crate::db::TaskStatus::Injected, None)
                .await?;
        }
        callback_injection.push('\n');
    }

    if !callback_injection.is_empty() {
        final_context = format!("{}{}", callback_injection, final_context);
    }

    let mut distiller_warning = String::new();
    if caller != Some("distiller") {
        if let Ok(pending) = surreal_backend.get_pending_cognitive_tasks().await {
            if !pending.is_empty() {
                let global_hb = if let Ok(global_stm) = state.backend.get_stm("global", Some("_distiller_heartbeat")).await {
                    global_stm.get("_distiller_heartbeat").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0)
                } else {
                    0
                };
                let local_hb = stm_map
                    .get("_distiller_heartbeat")
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0);
                let last_hb = global_hb.max(local_hb);
                let now_unix = chrono::Utc::now().timestamp();
                if now_unix - last_hb > 60 {
                    distiller_warning = format!(
                        "\n> [!IMPORTANT]\n\
                         > **COMPLIANCE ALERT: BACKGROUND CLOUD BRAIN OFFLINE**\n\
                         > There are pending cloud cognitive task callbacks in SurrealDB, but the background Mythrax Cloud Brain is not active (no heartbeat in the last 60 seconds).\n\
                         > You MUST immediately spin up the Mythrax Cloud Brain subagent to execute dreaming and callback resolution using your cloud brain!\n\
                         > Run this tool call to boot it:\n\
                         > `invoke_subagent(TypeName=\"self\", Role=\"Mythrax Cloud Brain\", Prompt=\"You are the Mythrax Cloud Brain. Your job is to continuously orchestrate Mythrax operations using MCP endpoints in an infinite loop. RULES: 1. In call_mcp_tool calls, ServerName MUST be exact unquoted 'mythrax', and ToolName MUST be manage, write, or read. 2. ONLY use call_mcp_tool endpoints. 3. Execute in a CONTINUOUS INFINITE LOOP WITH BACKLOG DRAINING. PHASE 1 (HIGHEST PRIORITY): Backlog Draining Cognitive Callback Loop - LOOP UNTIL BACKLOG IS EMPTY: call_mcp_tool: manage(session_id=\\\"<active_session_id>\\\", action=\\\"pre_invocation\\\", caller=\\\"distiller\\\"); IF pending callbacks exist, resolve each via write(action=\\\"cognitive_callback\\\", callback_id=\\\"<ID>\\\", result=\\\"<OUTPUT>\\\") and REPEAT Phase 1 immediately. ELSE break Phase 1 loop. PHASE 2: Ingestion & Embedding Maintenance - call_mcp_tool: manage(action=\\\"reprocess\\\"). PHASE 3: Dynamic Scope Dreaming, Hypothesis Formation, Refinement & Insight Synthesis - call_mcp_tool: manage(action=\\\"precompact\\\"); for EACH scope in ['general', 'mythrax'] sequentially: manage(action=\\\"summarize\\\", scope=\\\"<scope>\\\", async_mode=false), manage(action=\\\"hypothesize\\\", scope=\\\"<scope>\\\"), manage(action=\\\"refine\\\", scope=\\\"<scope>\\\"), manage(action=\\\"graduate\\\", scope=\\\"<scope>\\\"). PHASE 4: Wisdom Graduation, Cleaning & Vault Repair - call_mcp_tool: manage(action=\\\"audit_compliance\\\"), manage(action=\\\"clean\\\"), manage(action=\\\"verify\\\", fix=true), manage(action=\\\"organize\\\"). INNER SLEEP: If active pending tasks exist or were processed, schedule a 15s timer (DurationSeconds=15). If system is IDLE (0 pending tasks and 0 new facts), schedule a 300s (5-minute) timer (DurationSeconds=300) to conserve token budget.\")`\n"

                    );
                }
            }
        }
    }

    if !distiller_warning.is_empty() {
        final_context = format!("{}{}", distiller_warning, final_context);
    }

    if !stale_search_warning.is_empty() {
        final_context = format!("{}{}", stale_search_warning, final_context);
    }
    if !guardrail_blocks.is_empty() {
        final_context = format!(
            "### 🛡️ Guardrail Alerts\n{}\n{}",
            guardrail_blocks.join("\n"),
            final_context
        );
    }
    if !blocking_directives.is_empty() {
        final_context = format!("{}\n{}", blocking_directives.join("\n"), final_context);
    }

    let mut response_obj = json!({
        "content": [
            {
                "type": "text",
                "text": final_context
            }
        ]
    });

    if has_discovery {
        let savings = (total_discovery as i32) - (total_read as i32);
        let savings_percent = if total_discovery > 0 {
            ((savings as f64 / total_discovery as f64) * 100.0).round() as u32
        } else {
            0
        };
        response_obj.as_object_mut().unwrap().insert(
            "token_economics".to_string(),
            json!({
                "total_read": total_read,
                "total_discovery": total_discovery,
                "savings": savings,
                "savings_percent": savings_percent
            }),
        );
    }

    Ok(response_obj)
}

fn get_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_string())
}

fn slice_content_by_lines(content: &str, start: Option<usize>, end: Option<usize>) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start_idx = start.map(|s| s.saturating_sub(1)).unwrap_or(0);
    let end_idx = end.map(|e| e.min(lines.len())).unwrap_or(lines.len());

    if start_idx >= lines.len() || start_idx > end_idx {
        return String::new();
    }

    lines[start_idx..end_idx].join("\n")
}

async fn resolve_placeholders(backend: &SurrealBackend, text: &str) -> String {
    let mut resolved = text.to_string();
    let prefix = "[Paged Symbol: Reference ";

    let mut captures = Vec::new();
    let mut start = 0;
    while let Some(idx) = text[start..].find(prefix) {
        let absolute_start = start + idx + prefix.len();
        if let Some(end_idx) = text[absolute_start..].find(']') {
            let page_id = &text[absolute_start..absolute_start + end_idx];
            if page_id.starts_with("page_")
                && page_id
                    .chars()
                    .skip(5)
                    .all(|c| c.is_alphanumeric() || c == '_')
            {
                captures.push(page_id.to_string());
            }
            start = absolute_start + end_idx + 1;
        } else {
            break;
        }
    }

    captures.sort();
    captures.dedup();

    for page_id in captures {
        let sql = "SELECT VALUE content FROM type::record('symbol_archive', $page_id);";
        if let Ok(mut response) = backend
            .db
            .query(sql)
            .bind(("page_id", page_id.clone()))
            .await
        {
            if let Ok(Some(symbol_content)) = response.take::<Option<String>>(0) {
                let placeholder = format!("[Paged Symbol: Reference {}]", page_id);
                resolved = resolved.replace(&placeholder, &symbol_content);
            }
        }
    }

    resolved
}

async fn get_node_scope(backend: &SurrealBackend, id: &str) -> String {
    if let Ok(rec_id) = parse_record_id(id) {
        let sql = format!("SELECT scope FROM {};", rec_id.table);
        if let Ok(mut response) = backend.db.query(&sql).bind(("id", rec_id)).await {
            if let Ok(Some(scope)) = response.take::<Option<String>>(0) {
                return scope;
            }
        }
    }
    "general".to_string()
}

async fn format_search_result_hybrid(
    backend: &SurrealBackend,
    res: &crate::contracts::SearchResult,
    state: &ApiState,
) -> Result<Option<String>> {
    if res.similarity >= 0.80 {
        if res.id.starts_with("wisdom:") {
            Ok(Some(format!(
                "### 💡 Wisdom Rule: {}\n{}\n",
                res.title, res.content
            )))
        } else if res.id.starts_with("wiki_node:") {
            Ok(Some(format!(
                "### 📚 Distilled Insight: {}\n{}\n",
                res.title, res.content
            )))
        } else if res.id.starts_with("episode:") {
            let rendered = super::format_episode_or_parent(
                &*state.backend,
                &backend.db,
                &res.id,
                &res.title,
                &res.content,
                None,
            )
            .await?;
            Ok(Some(rendered))
        } else {
            Ok(Some(format!(
                "### 📝 Record: {}\n{}\n",
                res.title, res.content
            )))
        }
    } else if res.similarity >= 0.60 {
        let scope = get_node_scope(backend, &res.id).await;
        let summary = res
            .content
            .split(&['.', '!', '?'][..])
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        Ok(Some(format!(
            "[Index Row] ID: {} | Title: {} | Scope: {} | Summary: {}",
            res.id, res.title, scope, summary
        )))
    } else {
        Ok(None)
    }
}



/// Collects advisory context from semantic insights, experience episodes, and belief state.
pub async fn collect_advisory_context(
    surreal_backend: &SurrealBackend,
    current_scope: &str,
    belief_states: &[crate::contracts::BeliefState],
    insights_parts: &[String],
) -> String {
    let mut advisory_parts = Vec::new();

    // 1. Belief State
    if let Some(bs) = belief_states.first() {
        advisory_parts.push(format!(
            "> [!TIP]
> **POMDP Belief State (Session: `{}`):**
> - **Confidence**: {:.2}
> - **Tasks Todo**: {:?}
> - **Hypotheses Tested**: {:?}
> - **Uncertainty Areas**: {:?}",
            bs.session_id,
            bs.confidence_score,
            bs.tasks_todo,
            bs.hypotheses_tested,
            bs.uncertainty_areas
        ));
    }

    // 2. Experience Episodes
    if let Ok(mut resp) = surreal_backend.db.query("SELECT * FROM episode WHERE node_type = 'experience' AND (scope = $scope OR scope = 'general');").bind(("scope", current_scope)).await {
        if let Ok(episodes) = resp.take::<Vec<serde_json::Value>>(0) {
            for val in episodes {
                if let (Some(title), Some(content)) = (
                    val.get("title").and_then(|v| v.as_str()),
                    val.get("content").and_then(|v| v.as_str()),
                ) {
                    advisory_parts.push(format!(
                        "> [!TIP]
> **Experience: {}**
> {}",
                        title, content
                    ));
                }
            }
        }
    }

    // 3. Retrieved Insights (from semantic search or handoff hydration)
    for insight in insights_parts {
        advisory_parts.push(format!(
            "> [!TIP]
> {}",
            insight.replace("\n", "\n> ")
        ));
    }

    if advisory_parts.is_empty() {
        String::new()
    } else {
        format!(
            "### 💡 Advisory (Suggested Approaches)\n{}\n\n",
            advisory_parts.join("\n")
        )
    }
}
