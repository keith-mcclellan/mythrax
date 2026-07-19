use serde_json::{json, Value};
use anyhow::{Result, Context};
use std::path::Path;
use crate::api::ApiState;
use crate::db::{SurrealBackend, parse_record_id};
use crate::contracts::*;
use surrealdb_types::SurrealValue;

pub async fn handle_manage(state: &ApiState, args: Value) -> Result<Value> {
    let action_opt = args.get("action").and_then(|v| v.as_str());
    let resolved_action = if let Some(act) = action_opt {
        act
    } else {
        if args.get("session_id").and_then(|v| v.as_str()).is_some() {
            "pre_invocation"
        } else if args.get("workspace_path").and_then(|v| v.as_str()).is_some() {
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
        "verify" | "organize" | "reprocess" | "summarize" | "audit" | "ingest_bulk" | "ingest_forge" | "save_forged_assets" | "bootstrap" | "clean" => {
            match mapped_action {
                "ingest_bulk" => {
                    let _source = args.get("source").and_then(|v| v.as_str()).context("Missing source parameter for ingest_bulk")?;
                    let _harness = args.get("harness").and_then(|v| v.as_str()).context("Missing harness parameter for ingest_bulk")?;
                }
                "ingest_forge" => {
                    let _source_path = args.get("source").or_else(|| args.get("source_path")).and_then(|v| v.as_str()).context("Missing source parameter for ingest_forge")?;
                }
                "save_forged_assets" => {
                    let _doc_title = args.get("doc_title").context("Missing doc_title parameter for save_forged_assets")?;
                }
                _ => {}
            }
            let mut modified_args = args.clone();
            if let Some(obj) = modified_args.as_object_mut() {
                obj.insert("action".to_string(), serde_json::Value::String(mapped_action.to_string()));
            }
            super::vault_handlers::handle_manage_vault(state, modified_args).await
        }
        "init" | "ideate" | "execute" | "backprop" | "merge" | "run" => {
            let _scope = args.get("scope").and_then(|v| v.as_str()).context("Missing scope parameter for HTR action")?;
            match mapped_action {
                "init" | "run" => {
                    let _hypothesis = args.get("hypothesis").and_then(|v| v.as_str()).context("Missing hypothesis parameter")?;
                }
                "ideate" | "execute" | "backprop" | "merge" => {
                    let _node_id = args.get("node_id").and_then(|v| v.as_str()).context("Missing node_id parameter")?;
                }
                _ => {}
            }
            let mut modified_args = args.clone();
            if let Some(obj) = modified_args.as_object_mut() {
                obj.insert("action".to_string(), serde_json::Value::String(mapped_action.to_string()));
            }
            super::htr_handlers::handle_manage_htr(state, modified_args).await
        }
        "pre_invocation" => {
            let _session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id parameter for pre_invocation")?;
            handle_pre_invocation_hook(state, args).await
        }
        "precompact" => {
            let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id parameter for precompact")?;
            let transcript_path_str = args.get("transcript_path").and_then(|v| v.as_str()).context("Missing transcript_path parameter for precompact")?;
            let count = crate::hooks::precompact::mine_transcript(
                session_id,
                transcript_path_str,
                state.backend.as_ref(),
                state.store.as_ref(),
                &state.ignore_list,
            ).await?;
            Ok(json!({ "status": "success", "episodes_saved": count }))
        }
        "stop" => {
            let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id parameter for stop")?;
            let transcript_path_str = args.get("transcript_path").and_then(|v| v.as_str()).context("Missing transcript_path parameter for stop")?;
            let decision = crate::hooks::stop::mine_if_due(
                session_id,
                transcript_path_str,
                false,
                &state.backend,
                &state.store,
                &state.ignore_list,
            ).await?;
            let block = decision.is_some();
            let count = decision.unwrap_or(0);
            Ok(json!({ "status": "success", "block": block, "episodes_saved": count }))
        }
        "reflect" => {
            let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id parameter for reflect")?;
            let transcript_path_str = args.get("transcript_path").and_then(|v| v.as_str()).context("Missing transcript_path parameter for reflect")?;
            let status = crate::hooks::reflect::handle_reflect(
                session_id,
                transcript_path_str,
                state.backend.as_ref(),
            ).await?;
            Ok(json!({ "status": status }))
        }
        "audit_response" => {
            let response_text = args.get("response").and_then(|v| v.as_str()).context("Missing response parameter for audit_response")?;
            let rules_path_opt = args.get("rules_path").and_then(|v| v.as_str());
            let session_id_opt = args.get("session_id").and_then(|v| v.as_str());
            let fail_on_violation = args.get("fail_on_violation").and_then(|v| v.as_bool()).unwrap_or(true);
            
            // 1. Read rules from rules_path if provided, else fallback to standard locations
            let mut rules_content = String::new();
            if let Some(rules_path) = rules_path_opt {
                if let Ok(content) = std::fs::read_to_string(rules_path) {
                    rules_content = content;
                } else {
                    tracing::warn!("Configured rules_path '{}' not found, falling back to default rules", rules_path);
                }
            }
            
            if rules_content.is_empty() {
                // Try workspace AGENTS.md first
                let workspace_root = std::env::var("MYTHRAX_WORKSPACE_ROOT")
                    .ok()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                let ws_agents_path = workspace_root.join(".agents").join("AGENTS.md");
                let global_agents_path = std::path::PathBuf::from("/Users/keith/.gemini/config/AGENTS.md");
                
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
                if let Ok(db_rules) = state.backend.get_all_wisdom_rules().await {
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
                
                let surreal_backend = state.backend.as_any().downcast_ref::<crate::db::backend::SurrealBackend>()
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
                    tracing::warn!("Cognitive callback for response audit timed out, falling back to local model");
                    match llm.completion_explicit(
                        state.backend.as_ref(),
                        "local",
                        "gemini",
                        "mlx-community/Qwen3.6-35B-A3B-4bit",
                        Some(system_instruction),
                        &prompt,
                        false,
                    ).await {
                        Ok(res) => res,
                        Err(_) => {
                            let config = state.backend.get_llm_config().await.unwrap_or_default();
                            let cloud_model = if config.cloud_provider == "gemini" && (config.model.contains("Qwen") || config.model.is_empty()) {
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
                            ).await.unwrap_or_else(|_| "APPROVED".to_string())
                        }
                    }
                }
            } else {
                match llm.completion_explicit(
                    state.backend.as_ref(),
                    "local",
                    "gemini",
                    "mlx-community/Qwen3.6-35B-A3B-4bit",
                    Some(system_instruction),
                    &prompt,
                    false,
                ).await {
                    Ok(res) => res,
                    Err(_) => {
                        let config = state.backend.get_llm_config().await.unwrap_or_default();
                        let cloud_model = if config.cloud_provider == "gemini" && (config.model.contains("Qwen") || config.model.is_empty()) {
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
                        ).await.unwrap_or_else(|_| "APPROVED".to_string())
                    }
                }
            };

            let compliant = audit_res.trim().to_uppercase().contains("APPROVED") || audit_res.trim().to_uppercase() == "APPROVED";
            
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
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action parameter for agent tool")?;
    let mapped_action = match action {
        "complete_task" => "complete_code_task",
        "save_handoff" => "handoff",
        other => other,
    };
    match mapped_action {
        "complete_code_task" => {
            let _prompt = args.get("prompt").and_then(|v| v.as_str()).context("Missing prompt parameter for agent:complete_code_task")?;
            handle_complete_code_task(state, args).await
        }
        "handoff" => {
            let _parent = args.get("parent_conversation_id").and_then(|v| v.as_str()).context("Missing parent_conversation_id")?;
            let _subagent = args.get("subagent_conversation_id").and_then(|v| v.as_str()).context("Missing subagent_conversation_id")?;
            let _summary = args.get("summary").and_then(|v| v.as_str()).context("Missing summary")?;
            handle_manage_stm(state, args).await
        }
        _ => anyhow::bail!("Invalid action for agent tool: {}", action),
    }
}

pub async fn handle_manage_stm(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    match action {
        "put" => {
            let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
            let key = args.get("key").and_then(|v| v.as_str()).context("Missing key")?;
            let value = args.get("value").and_then(|v| v.as_str()).context("Missing value")?;

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
            let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
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
            let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;

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
            let parent_conversation_id = args.get("parent_conversation_id").and_then(|v| v.as_str()).context("Missing parent_conversation_id")?.to_string();
            let subagent_conversation_id = args.get("subagent_conversation_id").and_then(|v| v.as_str()).context("Missing subagent_conversation_id")?.to_string();
            let summary = args.get("summary").and_then(|v| v.as_str()).context("Missing summary")?.to_string();
            let handoff_file_path = args.get("handoff_file_path").and_then(|v| v.as_str()).context("Missing handoff_file_path")?.to_string();
            let scope = args.get("scope").and_then(|v| v.as_str()).map(|s| s.to_string());
            let include_tool_execution = args.get("include_tool_execution").and_then(|v| v.as_bool());

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
                format!("Handoff registered. Parent: {}, Subagent: {}, Summary: {}, File Path: {}", parent_conversation_id, subagent_conversation_id, handoff.summary, handoff.handoff_file_path),
            )
            .scope(handoff.scope.clone())
            .session_id(Some(parent_conversation_id.clone()))
            .node_type(Some("handoff_event".to_string()))
            .build();
            let _ = state.backend.save_episode(&event_ep).await;

            if let Ok(stm_map) = state.backend.get_stm(&parent_conversation_id, Some("_session_citations")).await {
                if let Some(citations_str) = stm_map.get("_session_citations") {
                    if let Ok(episode_ids) = serde_json::from_str::<Vec<String>>(citations_str) {
                        if !episode_ids.is_empty() {
                            if let Ok(nodes_resp) = state.backend.get_memory_nodes(&episode_ids).await {
                                let mut footnote = String::new();
                                footnote.push_str("\n\n### Citations\n");
                                let vault_root = state.store.vault_root.clone();
                                for ep in nodes_resp.episodes {
                                    if let Some(ref vp) = ep.vault_path {
                                        let abs_path = vault_root.join(vp);
                                        footnote.push_str(&format!("- [{}]((file://{}))\n", ep.title, abs_path.display()));
                                    }
                                }
                                
                                let abs_handoff_path = if std::path::Path::new(&handoff_file_path).is_absolute() {
                                    std::path::PathBuf::from(&handoff_file_path)
                                } else {
                                    vault_root.join(&handoff_file_path)
                                };

                                if abs_handoff_path.exists() {
                                    if let Ok(mut content) = std::fs::read_to_string(&abs_handoff_path) {
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
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
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
            let provider = args.get("provider").and_then(|v| v.as_str()).context("Missing provider")?.to_string();
            let duration = args.get("duration").and_then(|v| v.as_str()).map(|s| s.to_string());
            let model = args.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
            let cloud_provider = args.get("cloud_provider").and_then(|v| v.as_str()).map(|s| s.to_string());
            let api_key = args.get("api_key").and_then(|v| v.as_str()).map(|s| s.to_string());
            let llm_post_inference_delay_ms = args.get("llm_post_inference_delay_ms")
                .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())));

            let model_tier_mappings = args.get("model_tier_mappings")
                .and_then(|v| serde_json::from_value::<std::collections::HashMap<String, String>>(v.clone()).ok());

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
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    
    let path = args.get("path")
        .or_else(|| args.get("AbsolutePath"))
        .or_else(|| args.get("TargetFile"))
        .and_then(|v| v.as_str())
        .context("Missing path/AbsolutePath/TargetFile")?;

    match action {
        "view" => {
            let start_line = args.get("start_line")
                .or_else(|| args.get("StartLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let end_line = args.get("end_line")
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
                    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                        .context("SurrealBackend required")?;
                    crate::cognitive::paging::page_code_block(surreal_backend, &sliced_content, ext).await?
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
            let target_content = args.get("target_content")
                .or_else(|| args.get("TargetContent"))
                .and_then(|v| v.as_str())
                .context("Missing target_content/TargetContent")?;
            let replacement_content = args.get("replacement_content")
                .or_else(|| args.get("ReplacementContent"))
                .and_then(|v| v.as_str())
                .context("Missing replacement_content/ReplacementContent")?;
            let start_line = args.get("start_line")
                .or_else(|| args.get("StartLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let end_line = args.get("end_line")
                .or_else(|| args.get("EndLine"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let allow_multiple = args.get("allow_multiple")
                .or_else(|| args.get("AllowMultiple"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let path_buf = Path::new(path);
            let file_content = std::fs::read_to_string(path_buf)?;

            let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            let resolved_target = resolve_placeholders(surreal_backend, target_content).await;
            let resolved_replacement = resolve_placeholders(surreal_backend, replacement_content).await;

            let sliced_content = slice_content_by_lines(&file_content, start_line, end_line);
            
            let occurrences = sliced_content.matches(&resolved_target).count();
            if occurrences == 0 {
                anyhow::bail!("Target content not found in the file.");
            }
            if occurrences > 1 && !allow_multiple {
                anyhow::bail!("Target content found multiple times in the file, but AllowMultiple is false.");
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
                format!("Artifact Edited: {}", path_buf.file_name().and_then(|s| s.to_str()).unwrap_or("file")),
                format!("File updated successfully: {}", rel_path),
            )
            .scope(Some("general".to_string()))
            .vault_path(Some(rel_path))
            .files_modified(Some(vec![path.to_string()]))
            .node_type(Some("artifact_state".to_string()))
            .build();
            let _ = state.backend.save_episode(&artifact_ep).await;

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
            let chunks = args.get("chunks")
                .or_else(|| args.get("ReplacementChunks"))
                .and_then(|v| v.as_array())
                .context("Missing/Invalid chunks/ReplacementChunks")?;

            let path_buf = Path::new(path);
            let mut file_content = std::fs::read_to_string(path_buf)?;

            let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            for chunk in chunks {
                let target_content = chunk.get("target_content")
                    .or_else(|| chunk.get("TargetContent"))
                    .and_then(|v| v.as_str())
                    .context("Missing target_content/TargetContent in chunk")?;
                let replacement_content = chunk.get("replacement_content")
                    .or_else(|| chunk.get("ReplacementContent"))
                    .and_then(|v| v.as_str())
                    .context("Missing replacement_content/ReplacementContent in chunk")?;
                let start_line = chunk.get("start_line")
                    .or_else(|| chunk.get("StartLine"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let end_line = chunk.get("end_line")
                    .or_else(|| chunk.get("EndLine"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let allow_multiple = chunk.get("allow_multiple")
                    .or_else(|| chunk.get("AllowMultiple"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let resolved_target = resolve_placeholders(surreal_backend, target_content).await;
                let resolved_replacement = resolve_placeholders(surreal_backend, replacement_content).await;

                let sliced_content = slice_content_by_lines(&file_content, start_line, end_line);
                
                let occurrences = sliced_content.matches(&resolved_target).count();
                if occurrences == 0 {
                    anyhow::bail!("Target content not found in the file.");
                }
                if occurrences > 1 && !allow_multiple {
                    anyhow::bail!("Target content found multiple times in the file, but AllowMultiple is false.");
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

                file_content = new_content;
            }

            std::fs::write(path_buf, file_content)?;

            let rel_path = if let Ok(stripped) = path_buf.strip_prefix(&state.store.vault_root) {
                stripped.to_string_lossy().to_string()
            } else {
                path.to_string()
            };

            let artifact_ep = EpisodeSave::builder(
                format!("Artifact Edited: {}", path_buf.file_name().and_then(|s| s.to_str()).unwrap_or("file")),
                format!("File updated successfully: {}", rel_path),
            )
            .scope(Some("general".to_string()))
            .vault_path(Some(rel_path))
            .files_modified(Some(vec![path.to_string()]))
            .node_type(Some("artifact_state".to_string()))
            .build();
            let _ = state.backend.save_episode(&artifact_ep).await;

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

pub async fn handle_pre_invocation_hook(state: &ApiState, args: Value) -> Result<Value> {
    let mut stm_str = String::new();
    let session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
    let caller = args.get("caller").and_then(|v| v.as_str());

    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required for pre_invocation_hook")?;

    if caller == Some("distiller") {
        let now_unix = chrono::Utc::now().timestamp();
        let _ = state.backend.save_stm(session_id, "_distiller_heartbeat", &now_unix.to_string()).await;
        let _ = crate::mcp_routes::write_handlers::sweep_expired_tasks(state).await;

        let pending_tasks = surreal_backend.get_pending_cognitive_tasks().await?;
        let mut selected_tasks = Vec::new();
        let immediate_task = pending_tasks.iter().find(|t| t.priority == "Immediate");
        if let Some(t) = immediate_task {
            selected_tasks.push(t.clone());
        } else {
            for t in pending_tasks.iter().filter(|t| t.priority != "Immediate").take(3) {
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
                surreal_backend.update_cognitive_task_status(&task.id, crate::db::TaskStatus::Injected, None).await?;
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

    state.backend.journal_state(&state.store.vault_root, Some(session_id)).await?;

    let all_eps = state.backend.get_all_episodes().await?;
    for ep in &all_eps {
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

    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required for pre_invocation_hook")?;

    // WU-4.5: TTL Sweep & LargeLocal Fallback
    let _ = crate::mcp_routes::write_handlers::sweep_expired_tasks(state).await;

    // WU-6.9: PagingManager context window paging
    let token_budget = 8000u32;
    let sql_session = "SELECT * FROM episode WHERE session_id = $session_id AND archived = false;";
    if let Ok(mut resp) = surreal_backend.db.query(sql_session).bind(("session_id", session_id)).await {
        if let Ok(episodes) = resp.take::<Vec<crate::contracts::Episode>>(0) {
            let mut total_tokens = 0u32;
            let mut pm = crate::cognitive::memory_os::PagingManager::new(500);

            for ep in &episodes {
                let tokens = calc_tokens(&ep.title, &ep.content, ep.facts.as_deref());
                total_tokens += tokens;
                
                if let Some(ref id) = ep.id {
                    let pinned = ep.node_type.as_deref() == Some("user_input") || ep.node_type.as_deref() == Some("task_checklist");
                    pm.access_node(id.clone(), crate::cognitive::memory_os::ActiveNodeInfo {
                        importance: ep.importance.unwrap_or(50.0),
                        node_type: "episode".to_string(),
                        pinned,
                    });
                }
            }

            if total_tokens > token_budget {
                let excess_tokens = total_tokens.saturating_sub(token_budget);
                let mut tokens_freed = 0u32;
                
                let mut evictable_episodes = episodes.iter()
                    .filter(|ep| {
                        let is_pinned = ep.node_type.as_deref() == Some("user_input") || ep.node_type.as_deref() == Some("task_checklist");
                        !is_pinned && ep.id.is_some()
                    })
                    .collect::<Vec<_>>();
                evictable_episodes.sort_by(|a, b| a.importance.unwrap_or(50.0).partial_cmp(&b.importance.unwrap_or(50.0)).unwrap_or(std::cmp::Ordering::Equal));

                for ep in evictable_episodes {
                    if tokens_freed >= excess_tokens {
                        break;
                    }
                    let tokens = calc_tokens(&ep.title, &ep.content, ep.facts.as_deref());
                    tokens_freed += tokens;

                    if let Some(ref id) = ep.id {
                        let id_raw = id.split(':').nth(1).unwrap_or(id).to_string();
                        let archive_sql = "UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() };";
                        let _ = surreal_backend.db.query(archive_sql).bind(("id", id_raw)).await;
                    }
                }
            }
        }
    }

    if let Some(q) = query {
        let insert_sql = "INSERT INTO chat_history { session_id: $session_id, role: 'user', content: $content, created_at: time::now() };";
        let _ = surreal_backend.db.query(insert_sql)
            .bind(("session_id", session_id))
            .bind(("content", q.to_string()))
            .await;
    }

    let mut capabilities_wisdom_part = String::new();
    let capabilities_res = surreal_backend.db.query("SELECT * FROM wisdom WHERE tier = 'permanent';").await;
    if let Ok(mut resp) = capabilities_res {
        let rules: Vec<WisdomRule> = resp.take(0).unwrap_or_default();
        if !rules.is_empty() {
            let mut rule_strings = Vec::new();
            for r in rules {
                rule_strings.push(format!(
                    "- **Rule on {}**:\n  - **Avoid**: {}\n  - **Remedy**: {}",
                    r.target_pattern, r.action_to_avoid, r.prescribed_remedy
                ));
            }
            capabilities_wisdom_part = format!(
                "### 🛠️ Mythrax Capabilities & Tool Wisdom\n{}\n\n",
                rule_strings.join("\n")
            );
        }
    }

    let mut belief_part = String::new();
    let belief_res = surreal_backend.db.query("SELECT session_id, tasks_todo, hypotheses_tested, confidence_score, uncertainty_areas, updated_at FROM belief_state WHERE session_id = $session_id;")
        .bind(("session_id", session_id))
        .await;
    
    if let Ok(mut resp) = belief_res {
        let belief_states: Vec<BeliefState> = resp.take(0).unwrap_or_default();
        if let Some(bs) = belief_states.first() {
            belief_part = format!(
                "### 🧠 POMDP Belief State\n- **Session**: `{}`\n- **Confidence**: {:.2}\n- **Tasks Todo**: {:?}\n- **Hypotheses Tested**: {:?}\n- **Uncertainty Areas**: {:?}\n\n",
                bs.session_id, bs.confidence_score, bs.tasks_todo, bs.hypotheses_tested, bs.uncertainty_areas
            );
        }
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
                    let is_assistant = val.get("role").and_then(|r| r.as_str()).map(|r| r == "assistant").unwrap_or(false)
                        || val.get("source").and_then(|s| s.as_str()).map(|s| s == "MODEL").unwrap_or(false);
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
                let cleaned = nodes_str.trim_matches(|c| c == '[' || c == ']' || c == '"' || c == ' ');
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
            
            for wiki in &hydrated.wiki_nodes {
                let is_util = turn_content.to_lowercase().contains(&wiki.name.to_lowercase());
                if is_util { utilized_count += 1; }
            }
            
            for wisdom in &hydrated.wisdom_rules {
                let is_util = turn_content.to_lowercase().contains(&wisdom.target_pattern.to_lowercase());
                if is_util { utilized_count += 1; }
                // EMA Reinforcement (WU-3.5)
                let target_imp = if is_util { 10.0 } else { 1.0 };
                let current_imp = wisdom.importance.unwrap_or(5.0) as f32;
                let new_imp = 0.9 * current_imp + 0.1 * target_imp;
                let update_sql = "UPDATE type::record('wisdom', $id) SET importance = $imp;";
                let id_part = wisdom.id.as_ref().map(|s| s.split(':').nth(1).unwrap_or(s)).unwrap_or("");
                let _ = surreal_backend.db.query(update_sql).bind(("id", id_part)).bind(("imp", new_imp)).await;
            }

            for ep in &hydrated.episodes {
                let is_util = turn_content.to_lowercase().contains(&ep.title.to_lowercase());
                if is_util { utilized_count += 1; }
                // EMA Reinforcement (WU-3.5)
                let target_imp = if is_util { 10.0 } else { 1.0 };
                let current_imp = ep.importance.unwrap_or(5.0) as f32;
                let new_imp = 0.9 * current_imp + 0.1 * target_imp;
                let update_sql = "UPDATE type::record('episode', $id) SET importance = $imp;";
                let id_part = ep.id.as_ref().map(|s| s.split(':').nth(1).unwrap_or(s)).unwrap_or("");
                let _ = surreal_backend.db.query(update_sql).bind(("id", id_part)).bind(("imp", new_imp)).await;
            }

            let mem_util_score = (utilized_count * 100) / injected_nodes.len();
            let _ = state.backend.save_stm(session_id, "_last_memory_utilization", &mem_util_score.to_string()).await;
        }

        // 2. Guardrail Engine rule violations (WU-3.2)
        let active_rules_res = surreal_backend.db.query("SELECT * FROM wisdom WHERE status = 'active';").await;
        if let Ok(mut resp) = active_rules_res {
            let active_rules: Vec<WisdomRule> = if let Ok(raw_rules) = resp.take::<Vec<crate::db::backend::WisdomRaw>>(0) {
                raw_rules.into_iter().map(|r| r.into_wisdom_rule()).collect()
            } else {
                Vec::new()
            };
            for rule in active_rules {
                if turn_content.to_lowercase().contains(&rule.target_pattern.to_lowercase()) {
                    let severity = rule.severity.clone().unwrap_or_else(|| "WARNING".to_string()).to_uppercase();
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
            let _ = state.backend.save_stm(session_id, "checklist", &checklist_str).await;
            
            // Save as task_checklist episode
            let ep = EpisodeSave::builder("Active Task Checklist".to_string(), checklist_str)
                .scope(Some("general".to_string()))
                .session_id(Some(session_id.to_string()))
                .node_type(Some("task_checklist".to_string()))
                .build();
            let _ = state.backend.save_episode(&ep).await;
        }
    }

    // 4. Memory Query Frequency Tracker (WU-3.4)
    let mut stale_search_warning = String::new();
    let now_unix = chrono::Utc::now().timestamp();
    if let Some(last_search_str) = stm_map.get("_last_search_time") {
        if let Ok(last_search_time) = last_search_str.parse::<i64>() {
            let elapsed = now_unix - last_search_time;
            if elapsed > 300 {
                stale_search_warning = format!("\n> [!WARNING]\n> Warning: Memory searches are stale. Last search was {} seconds ago. Consider running a search query to pull relevant context.\n", elapsed);
            }
        } else {
            stale_search_warning = "\n> [!WARNING]\n> Warning: Memory searches are stale. No search has been performed in this session yet.\n".to_string();
        }
    } else {
        stale_search_warning = "\n> [!WARNING]\n> Warning: Memory searches are stale. No search has been performed in this session yet.\n".to_string();
    }
    let mut parts = Vec::new();

    let mut broker_status = "### 🤖 Local Inference & Model Broker Status\n- **Broker State**: Offline or uninitialized\n\n".to_string();
    if let Some(broker) = crate::llm::DYNAMIC_MODEL_BROKER.get() {
        let active_tier_str = match broker.active_tier() {
            Some(tier) => format!("{:?}", tier),
            None => "None (Idle)".to_string(),
        };
        let emb_loaded = if broker.is_embedding_model_loaded() { "Loaded" } else { "Not Loaded" };
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

    if !handoffs.is_empty() {
        let active_handoff = &handoffs[0];
        let parent_conversation_id = active_handoff.get("parent_conversation_id").and_then(|v| v.as_str()).unwrap_or("");
        let summary = active_handoff.get("summary").and_then(|v| v.as_str()).unwrap_or("");
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
            stm_str = format!("### 🔑 Stashed Session Variables\n{}\n", stm_parts.join("\n"));
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
                let cleaned = nodes_str.trim_matches(|c| c == '[' || c == ']' || c == '"' || c == ' ');
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
            parts.push("## Hydrated Context Nodes\n".to_string());
            for wiki in hydrated.wiki_nodes {
                total_read += calc_tokens(&wiki.name, &wiki.content, None);
                parts.push(format!("### 📚 Distilled Insight: {}\nScope: {}\n{}\n", wiki.name, wiki.scope, wiki.content));
            }
            for wisdom in hydrated.wisdom_rules {
                let rule_content = format!("Avoid: {}\nCausal: {}\nRemedy: {}", wisdom.action_to_avoid, wisdom.causal_explanation, wisdom.prescribed_remedy);
                total_read += calc_tokens(&wisdom.target_pattern, &rule_content, None);
                parts.push(format!(
                    "### 💡 Wisdom Rule: {}\n- **Avoid**: {}\n- **Causal**: {}\n- **Remedy**: {}\n",
                    wisdom.target_pattern, wisdom.action_to_avoid, wisdom.causal_explanation, wisdom.prescribed_remedy
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
                    let rendered = super::format_episode_or_parent(&*state.backend, &surreal_backend.db, ep_id, &ep.title, &ep.content, ep.scope.as_deref()).await?;
                    parts.push(rendered);
                }
            }
        } else {
            let search_res = state.backend.search(crate::contracts::SearchParams::from_positional(
                summary,
                scope,
                false,
                15,
                0,
                0.55,
                None,
                false,
                true,
                false,
                None,
                true,
                None,
            )).await?;

            parts.push("## Retrieved Semantic Context\n".to_string());
            for res in search_res.results {
                if res.discovery_tokens.is_some() {
                    has_discovery = true;
                }
                if let Some(dt) = res.discovery_tokens {
                    total_discovery += dt;
                }
                total_read += calc_tokens(&res.title, &res.content, None);
                if let Some(formatted) = format_search_result_hybrid(surreal_backend, &res, state).await? {
                    parts.push(formatted);
                }
            }
        }
    } else {
        let workspace_path_str = workspace_path.map(|s| s.to_string())
            .unwrap_or_else(|| std::env::var("MYTHRAX_WORKSPACE_ROOT").unwrap_or_else(|_| ".".to_string()));
        let path = std::path::Path::new(&workspace_path_str);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let folder_name = canonical.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("general")
            .to_string();
        let dynamic_scope = folder_name;

        let search_query = query.unwrap_or("general context");
        let search_res = state.backend.search(crate::contracts::SearchParams::from_positional(
            search_query,
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
        )).await?;

        parts.push(format!("## Retrieved Semantic Context (Scope: `{}`)\n", dynamic_scope));
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
            if let Some(formatted) = format_search_result_hybrid(surreal_backend, &res, state).await? {
                parts.push(formatted);
            }
        }

        if !high_confidence_memories_found {
            parts.push(format!(
                "\n> [!IMPORTANT]\n> **Pinned Deep-Search Instruction**: No high-confidence memory episodes were found. If you need deeper historical context or past resolutions, please call read(action=\"search\", query=\"...\") with a specific query.\n"
            ));
        }
    }

    let active_node_opt = stm_map.get("active_hypothesis_node")
        .or_else(|| stm_map.get("active_node"))
        .cloned();

    if let Some(active_node_id) = active_node_opt {
        let mut hyp_res = surreal_backend.db.query("SELECT * FROM hypothesis_node WHERE node_id = $node_id;")
            .bind(("node_id", active_node_id.as_str()))
            .await?;
        let hyp_nodes: Vec<HypothesisNode> = hyp_res.take(0)?;
        if let Some(hyp_node) = hyp_nodes.first() {
            if let Some(ref parent_id) = hyp_node.parent_id {
                let mut siblings_res = surreal_backend.db.query("SELECT * FROM hypothesis_node WHERE parent_id = $parent_id AND node_id != $node_id AND (status = 'failed' OR status = 'pruned');")
                    .bind(("parent_id", parent_id.as_str()))
                    .bind(("node_id", active_node_id.as_str()))
                    .await?;
                let siblings: Vec<HypothesisNode> = siblings_res.take(0)?;
                if !siblings.is_empty() {
                    let mut constraint_parts = Vec::new();
                    constraint_parts.push("\n### ⚠️ Arbor HTR Negative Constraints".to_string());
                    constraint_parts.push("The following sibling hypotheses have failed or been pruned in this HTR execution. Avoid repeating these approaches:".to_string());
                    for sib in siblings {
                        let status_label = if sib.status == "failed" { "FAILED" } else { "PRUNED" };
                        let reason = sib.result.or(sib.insight).unwrap_or_else(|| "No failure details recorded".to_string());
                        constraint_parts.push(format!("- **Approach to Avoid**: `{}` (Status: {})\n  - **Reason**: {}", sib.hypothesis, status_label, reason));
                    }
                    parts.push(constraint_parts.join("\n"));
                }
            }
        }
    }


    let count_tokens = |text: &str| -> usize {
        if let Some(ref embedder) = surreal_backend.embedder {
            embedder.count_tokens(text).unwrap_or_else(|_| text.split_whitespace().count())
        } else {
            text.split_whitespace().count()
        }
    };

    let budget_env = std::env::var("MYTHRAX_PRE_INVOCATION_TOKEN_BUDGET").unwrap_or_else(|_| "3000".to_string());
    let token_budget: usize = budget_env.parse().unwrap_or(3000);
    
    // P0: Pruned hypotheses & conflict nodes
    let mut p0_pruned_part = String::new();
    let current_scope = std::env::var("MYTHRAX_WORKSPACE_ROOT").unwrap_or_else(|_| ".".to_string());
    let path = std::path::Path::new(&current_scope);
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let folder_name = canonical.file_name().and_then(|n| n.to_str()).unwrap_or("general").to_string();
    
    let sql_pruned = "SELECT * FROM wisdom WHERE rule_type = 'pruned_hypothesis' AND status = 'active' AND (scope = $scope OR scope = 'general') LIMIT 5;";
    if let Ok(mut resp) = surreal_backend.db.query(sql_pruned).bind(("scope", folder_name)).await {
        if let Ok(raw_vals) = resp.take::<Vec<serde_json::Value>>(0) {
            if !raw_vals.is_empty() {
                p0_pruned_part.push_str("### ⛔ Known Failed Approaches\n");
                for val in raw_vals {
                    if let (Some(pat), Some(avoid), Some(remedy)) = (
                        val.get("target_pattern").and_then(|v| v.as_str()),
                        val.get("action_to_avoid").and_then(|v| v.as_str()),
                        val.get("prescribed_remedy").and_then(|v| v.as_str()),
                    ) {
                        p0_pruned_part.push_str(&format!("- **Pattern**: {}\n  **Avoid**: {}\n  **Remedy**: {}\n", pat, avoid, remedy));
                    }
                }
                p0_pruned_part.push('\n');
            }
        }
    }

    let mut p0_conflict_part = String::new();
    if let Ok(mut resp) = surreal_backend.db.query("SELECT * FROM episode WHERE node_type = 'conflict';").await {
        if let Ok(raw_vals) = resp.take::<Vec<serde_json::Value>>(0) {
            if !raw_vals.is_empty() {
                p0_conflict_part.push_str("### ⚠️ Known Knowledge Boundaries / Conflicts\n");
                for val in raw_vals {
                    if let (Some(title), Some(content)) = (
                        val.get("title").and_then(|v| v.as_str()),
                        val.get("content").and_then(|v| v.as_str()),
                    ) {
                        p0_conflict_part.push_str(&format!("- **{}**: {}\n", title, content));
                    }
                }
                p0_conflict_part.push('\n');
            }
        }
    }
    
    let p0_combined = format!("{}{}", p0_pruned_part, p0_conflict_part);
    
    // Start truncating based on priority: P3 -> P2 -> P1, preserve P0
    // Distiller payload is excluded from budget
    let mut p3_belief = belief_part.clone();
    let mut p2_stm = stm_str.clone(); // From earlier refactor
    let mut p1_wisdom = capabilities_wisdom_part.clone();
    
    let joined_context = parts.join("\n");
    
    let base_playbook = "### 💡 Mythrax Skill Playbook Reminder\n> [!IMPORTANT]\n> **Always load and refer to the `/mythrax` skill** (defined globally at `/Users/keith/.gemini/config/skills/mythrax/SKILL.md` or locally in the workspace at `.agents/skills/mythrax/SKILL.md`) to understand the consolidated MCP tools reference (`read`, `write`, `manage`, `agent`), agent handoff protocols, and virtual paging rules.\n\n";

    if caller != Some("distiller") {
        loop {
            let current_total = count_tokens(base_playbook)
                + count_tokens(&p0_combined)
                + count_tokens(&p1_wisdom)
                + count_tokens(&p2_stm)
                + count_tokens(&p3_belief)
                + count_tokens(&joined_context);

            if current_total <= token_budget {
                break;
            }

            if !p3_belief.is_empty() {
                p3_belief.clear();
            } else if !p2_stm.is_empty() {
                p2_stm.clear();
            } else if !p1_wisdom.is_empty() {
                p1_wisdom.clear();
            } else {
                break; // Can't truncate P0 or other parts
            }
        }
    }
    
    let initial_context = {
        let mut base = String::new();
        base.push_str(base_playbook);
        base.push_str(&p0_combined);
        if !p3_belief.is_empty() {
            base.push_str(&p3_belief);
        }
        if !p2_stm.is_empty() {
            base.push_str(&p2_stm);
        }
        if !p1_wisdom.is_empty() {
            base.push_str(&p1_wisdom);
        }
        base.push_str(&joined_context);
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
                    let turn_str = format!("- **{}**: {}\n", if turn.role == "user" { "User" } else { "Assistant" }, turn.content);
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
        for t in pending_tasks.iter().filter(|t| t.priority != "Immediate").take(3) {
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
            surreal_backend.update_cognitive_task_status(&task.id, crate::db::TaskStatus::Injected, None).await?;
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
                let last_hb = stm_map.get("_distiller_heartbeat")
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0);
                let now_unix = chrono::Utc::now().timestamp();
                if now_unix - last_hb > 60 {
                    distiller_warning = format!(
                        "\n> [!IMPORTANT]\n\
                         > **COMPLIANCE ALERT: BACKGROUND DISTILLER OFFLINE**\n\
                         > There are pending cloud cognitive task callbacks in SurrealDB, but the background Transcript Distiller is not active (no heartbeat in the last 60 seconds).\n\
                         > You MUST immediately spin up the distiller subagent to execute these tasks using your cloud brain!\n\
                         > Run this tool call to boot it:\n\
                         > `invoke_subagent(TypeName=\"self\", Role=\"Transcript Distiller\", Prompt=\"You are the Cloud Distiller. Your job is to run a loop to fetch pending cognitive tasks from the Mythrax daemon and solve them using your cloud brain. Please do the following: 1. Call call_mcp_tool with ServerName='mythrax', ToolName='manage', Arguments={{session_id: \\\"{}\\\", action: \\\"pre_invocation\\\", caller: \\\"distiller\\\"}}. 2. If the output is saved to a file, read the file. Look for the section '### 🧠 Pending Cognitive Callbacks'. 3. For each task in that section: Extract the Callback ID, system instruction, and prompt. Using your cloud brain, generate the output. Call call_mcp_tool write cognitive_callback... 4. Repeat. If no tasks are returned for 3 consecutive attempts, report back that the bootstrap process is complete.\")`\n",
                        session_id
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
        final_context = format!("### 🛡️ Guardrail Alerts\n{}\n{}", guardrail_blocks.join("\n"), final_context);
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
            })
        );
    }

    Ok(response_obj)
}

pub async fn handle_complete_code_task(state: &ApiState, args: Value) -> Result<Value> {
    let prompt = args.get("prompt").and_then(|v| v.as_str()).context("Missing prompt")?;
    let system_instruction = args.get("system_instruction").and_then(|v| v.as_str());
    let model_override = args.get("model").and_then(|v| v.as_str());
    let mut enable_thinking = args.get("enable_thinking").and_then(|v| v.as_bool()).unwrap_or(false);

    let lower_prompt = prompt.to_lowercase();
    if prompt.trim_start().starts_with("/think") || lower_prompt.contains("enable thinking") || lower_prompt.contains("with thinking") {
        enable_thinking = true;
    }

    let config = state.backend.get_llm_config().await?;
    let model = model_override.unwrap_or(&config.model);

    let client = crate::llm::LLMClient::default();
    let response = client.completion_explicit(
        state.backend.as_ref(),
        "local",
        &config.cloud_provider,
        model,
        system_instruction,
        prompt,
        enable_thinking,
    ).await?;

    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": response
            }
        ]
    }))
}

fn get_extension(path: &Path) -> Option<String> {
    path.extension().and_then(|ext| ext.to_str()).map(|s| s.to_string())
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
            if page_id.starts_with("page_") && page_id.chars().skip(5).all(|c| c.is_alphanumeric() || c == '_') {
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
        if let Ok(mut response) = backend.db.query(sql).bind(("page_id", page_id.clone())).await {
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
            Ok(Some(format!("### 💡 Wisdom Rule: {}\n{}\n", res.title, res.content)))
        } else if res.id.starts_with("wiki_node:") {
            Ok(Some(format!("### 📚 Distilled Insight: {}\n{}\n", res.title, res.content)))
        } else if res.id.starts_with("episode:") {
            let rendered = super::format_episode_or_parent(&*state.backend, &backend.db, &res.id, &res.title, &res.content, None).await?;
            Ok(Some(rendered))
        } else {
            Ok(Some(format!("### 📝 Record: {}\n{}\n", res.title, res.content)))
        }
    } else if res.similarity >= 0.60 {
        let scope = get_node_scope(backend, &res.id).await;
        let summary = res.content.split(&['.', '!', '?'][..])
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
