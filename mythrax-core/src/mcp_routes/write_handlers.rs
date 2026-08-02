use super::*;
use crate::api::ApiState;
use crate::contracts::{Entity, EpisodeSave, ThoughtNode};
use crate::db::SurrealBackend;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::sync::Arc;
use surrealdb::types::SurrealValue;

pub async fn handle_write(state: &ApiState, mut args: Value) -> Result<Value> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .context("Missing action parameter")?
        .to_string();
    let mapped_action = match action.as_str() {
        "replace" | "edit_file" => "replace",
        "multi_replace" | "multi_edit_file" => "multi_replace",
        "save" | "save_episode" => "save",
        "feedback" | "record_feedback" => "feedback",
        "put" | "put_short_term" => "put",
        "clear" | "clear_short_term" => "clear",
        "save_forged_assets" => "save_forged_assets",
        "ingest_bulk" => "ingest_bulk",
        "ingest_forge" => "ingest_forge",
        "set" | "set_config" => "set",
        "cognitive_callback" => "cognitive_callback",
        "save_wisdom" => "save_wisdom",
        other => other,
    };
    if let Some(obj) = args.as_object_mut() {
        obj.insert(
            "action".to_string(),
            serde_json::Value::String(mapped_action.to_string()),
        );
    }

    match mapped_action {
        "replace" => {
            let _path = args
                .get("path")
                .or_else(|| args.get("AbsolutePath"))
                .or_else(|| args.get("TargetFile"))
                .and_then(|v| v.as_str())
                .context("Missing path/AbsolutePath/TargetFile")?;
            let _target_content = args
                .get("target_content")
                .or_else(|| args.get("TargetContent"))
                .and_then(|v| v.as_str())
                .context("Missing target_content/TargetContent")?;
            let _replacement_content = args
                .get("replacement_content")
                .or_else(|| args.get("ReplacementContent"))
                .and_then(|v| v.as_str())
                .context("Missing replacement_content/ReplacementContent")?;
            super::manage_handlers::handle_manage_file(state, args).await
        }
        "multi_replace" => {
            let _path = args
                .get("path")
                .or_else(|| args.get("AbsolutePath"))
                .or_else(|| args.get("TargetFile"))
                .and_then(|v| v.as_str())
                .context("Missing path/AbsolutePath/TargetFile")?;
            let _chunks = args
                .get("chunks")
                .and_then(|v| v.as_array())
                .context("Missing chunks array parameter")?;
            super::manage_handlers::handle_manage_file(state, args).await
        }
        "save" => {
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .context("Missing title")?;
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .context("Missing content")?;
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general");
            let node_type = args
                .get("node_type")
                .or_else(|| args.get("type"))
                .or_else(|| args.get("item_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("experience");

            if node_type == "direction" || node_type == "insight" {
                let slug = title
                    .to_lowercase()
                    .replace(' ', "_")
                    .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
                let vault_path = format!("wiki/{}/directions/{}_{}.md", scope, slug, node_type);
                let wiki_node = crate::contracts::WikiNode {
                    id: None,
                    name: format!("{}/{}", scope, slug),
                    content: content.to_string(),
                    scope: scope.to_string(),
                    vault_path: Some(vault_path.clone()),
                    node_type: Some(node_type.to_string()),
                    item_type: Some(node_type.to_string()),
                    ..Default::default()
                };
                let id = state.backend.save_wiki_node(&wiki_node).await?;
                let _ = state.store.write_file(&vault_path, content);
                return Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Saved {} node '{}' with ID: {}", node_type, title, id)
                        }
                    ]
                }));
            }

            handle_record_memory(state, args).await
        }
        "feedback" => {
            let _episode_id = args
                .get("episode_id")
                .and_then(|v| v.as_str())
                .context("Missing episode_id")?;
            let _success = args
                .get("success")
                .and_then(|v| v.as_bool())
                .context("Missing success")?;
            handle_record_memory(state, args).await
        }
        "thought" => {
            let _content = args
                .get("content")
                .and_then(|v| v.as_str())
                .context("Missing content")?;
            handle_record_memory(state, args).await
        }
        "put" => {
            let _session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .context("Missing session_id")?;
            let _key = args
                .get("key")
                .and_then(|v| v.as_str())
                .context("Missing key")?;
            let _value = args
                .get("value")
                .and_then(|v| v.as_str())
                .context("Missing value")?;
            super::manage_handlers::handle_manage_stm(state, args).await
        }
        "clear" => {
            let _session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .context("Missing session_id")?;
            super::manage_handlers::handle_manage_stm(state, args).await
        }
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
            super::manage_handlers::handle_manage_stm(state, args).await
        }
        "set" => {
            if args.get("key").is_none() && args.get("provider").is_none() {
                anyhow::bail!("Missing provider or key parameter for set action");
            }
            super::manage_handlers::handle_manage_config(state, args).await
        }
        "save_forged_assets" | "ingest_bulk" | "ingest_forge" => {
            super::vault_handlers::handle_manage_vault(state, args).await
        }
        "cognitive_callback" => handle_cognitive_callback(state, args).await,
        "save_wisdom" => {
            let target_pattern = args.get("target_pattern").and_then(|v| v.as_str()).context("Missing target_pattern")?.to_string();
            let action_to_avoid = args.get("action_to_avoid").and_then(|v| v.as_str()).context("Missing action_to_avoid")?.to_string();
            let causal_explanation = args.get("causal_explanation").and_then(|v| v.as_str()).context("Missing causal_explanation")?.to_string();
            let prescribed_remedy = args.get("prescribed_remedy").and_then(|v| v.as_str()).context("Missing prescribed_remedy")?.to_string();
            let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string();
            
            let tier_str = args.get("tier").and_then(|v| v.as_str()).unwrap_or("wisdom");
            let tier = tier_str.parse::<crate::contracts::Tier>().unwrap_or(crate::contracts::Tier::Wisdom);
            
            let rule_path = crate::cognitive::pipeline::resolve_rule_path(&scope, &action_to_avoid);
            
            let rule = crate::contracts::WisdomRule {
                id: None,
                target_pattern,
                action_to_avoid,
                causal_explanation,
                prescribed_remedy,
                tier,
                scope,
                vault_path: Some(rule_path.clone()),
                generator_name: "Manual".to_string(),
                ..Default::default()
            };
            
            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required to save wisdom_rule")?;
            
            let markdown = crate::vault::watcher::format_wisdom_markdown(&rule);
            state.store.write_file(&rule_path, &markdown)?;
            
            let id = surreal_backend.save_wisdom_rule(&rule).await?;
            
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Wisdom rule saved successfully: {}", id)
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for write tool: {}", action),
    }
}

pub async fn handle_record_memory(state: &ApiState, args: Value) -> Result<Value> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("save");
    match action {
        "save" => {
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .context("Missing title")?
                .to_string();
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .context("Missing content")?
                .to_string();
            let scope = args
                .get("scope")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let vault_path = args
                .get("vault_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let task_id = args
                .get("task_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let node_type = args
                .get("node_type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let mut entities = vec![];
            if let Some(arr) = args.get("entities").and_then(|v| v.as_array()) {
                for item in arr {
                    let entity: Entity = serde_json::from_value(item.clone())?;
                    entities.push(entity);
                }
            }

            let episode = EpisodeSave::builder(title, content.clone())
                .entities(entities)
                .scope(scope.clone())
                .vault_path(vault_path)
                .session_id(session_id)
                .task_id(task_id)
                .node_type(node_type)
                .build();

            let id = crate::vault::watcher::save_episode_bidirectional(
                &episode,
                state.backend.as_ref(),
                &state.store,
                &state.ignore_list,
            )
            .await?;

            let content_lower = content.to_lowercase();
            let correction_indicators = [
                "you forgot",
                "incorrect",
                "that was a mistake",
                "that's wrong",
                "you made a mistake",
                "wrong choice",
                "not right",
                "should have",
                "didn't run",
                "actually that didn't work",
                "we need to fix",
                "revert",
                "fail",
                "error occurred",
            ];
            let has_correction = correction_indicators
                .iter()
                .any(|&ind| content_lower.contains(ind));

            if has_correction {
                if let Some(surreal_backend) =
                    state.backend.as_any().downcast_ref::<SurrealBackend>()
                {
                    let backend_clone = Arc::new(surreal_backend.clone());
                    let store_clone = state.store.clone();
                    let content_clone = content.clone();
                    let scope_clone = scope.clone();
                    let source_ep_id = Some(id.clone());
                    tokio::spawn(async move {
                        if let Err(e) = run_llm_critic(
                            backend_clone,
                            store_clone,
                            content_clone,
                            scope_clone,
                            source_ep_id,
                        )
                        .await
                        {
                            tracing::error!("Error running LLM critic: {:?}", e);
                        }
                    });
                }
            }

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Episode saved successfully: {}", id)
                    }
                ]
            }))
        }
        "feedback" => {
            let id = args
                .get("id")
                .or_else(|| args.get("episode_id"))
                .and_then(|v| v.as_str())
                .context("Missing id")?;
            let success = args
                .get("success")
                .and_then(|v| v.as_bool())
                .context("Missing success")?;
            state.backend.record_feedback(id, success).await?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "Feedback recorded successfully."
                    }
                ]
            }))
        }
        "thought" => {
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .context("Missing title")?
                .to_string();
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .context("Missing content")?
                .to_string();
            let scope = args
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("general")
                .to_string();

            let thought_uuid = uuid::Uuid::new_v4().to_string();
            let relative_path = format!("wiki/thoughts/thought_{}.md", thought_uuid);

            let thought = ThoughtNode {
                id: None,
                title,
                content,
                scope,
                vault_path: Some(relative_path.clone()),
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            let mut yaml_val = serde_json::Map::new();
            yaml_val.insert("title".to_string(), serde_json::json!(thought.title));
            yaml_val.insert("scope".to_string(), serde_json::json!(thought.scope));
            yaml_val.insert(
                "created_at".to_string(),
                serde_json::json!(thought.created_at),
            );
            let yaml_str = serde_yaml::to_string(&yaml_val).unwrap_or_default();
            let markdown = format!("---\n{}\n---\n{}", yaml_str.trim(), thought.content);

            state.store.write_file(&relative_path, &markdown)?;

            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required to save thought_node")?;
            let id = surreal_backend.save_thought_node(&thought).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Thought node saved successfully: {}", id)
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for record_memory: {}", action),
    }
}

pub async fn run_llm_critic(
    backend: Arc<crate::db::SurrealBackend>,
    store: Arc<crate::store::MarkdownStore>,
    content: String,
    scope: Option<String>,
    source_episode_id: Option<String>,
) -> Result<()> {
    let allow_cloud_fallback = match backend
        .db
        .query("SELECT allow_cloud_fallback FROM config:settings;")
        .await
    {
        Ok(mut resp) => {
            if let Ok(Some(val)) = resp.take::<Option<serde_json::Value>>(0) {
                val.get("allow_cloud_fallback")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
            } else {
                true
            }
        }
        Err(_) => true,
    };

    let system_instruction = "You are a systems critic. Analyze the dialog context/episode content showing a mistake, and extract a structured Wisdom Rule to prevent this mistake in the future. Focus on Causal failure modes and non-negotiable remedies. Respond ONLY with a JSON object.";
    let prompt = format!(
        "Analyze the following dialog context and extract a single Wisdom Rule to prevent the mistake:\n\n\
        {}\n\n\
        Respond ONLY with a JSON object containing exactly these fields:\n\
        - target_pattern (context or trigger pattern, e.g., 'git merge conflict')\n\
        - action_to_avoid (the specific incorrect action taken, causal failure mode, e.g., 'manually editing conflict markers without tools')\n\
        - causal_explanation (why it is bad / what happens, causal failure mode, e.g., 'leads to malformed syntax and compiler failures')\n\
        - prescribed_remedy (what to do instead, non-negotiable remedy, e.g., 'use git merge-tool or structured 3-way merge library')\n\n\
        Ensure it is a valid JSON object.",
        content
    );

    let llm = crate::llm::LLMClient::default();
    let response_text = match llm
        .completion_explicit(
            &*backend,
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
        Err(e) => {
            if allow_cloud_fallback {
                tracing::warn!("Local LLM critic failed, falling back to cloud: {:?}", e);
                let config = backend.get_llm_config().await?;
                let cloud_model = if config.cloud_provider == "gemini"
                    && (config.model.contains("Qwen") || config.model.is_empty())
                {
                    "gemini-1.5-flash"
                } else {
                    &config.model
                };
                llm.completion_explicit(
                    &*backend,
                    "cloud",
                    &config.cloud_provider,
                    cloud_model,
                    Some(system_instruction),
                    &prompt,
                    false,
                )
                .await?
            } else {
                return Err(e);
            }
        }
    };

    #[derive(serde::Deserialize, serde::Serialize, Debug)]
    struct CriticWisdom {
        target_pattern: String,
        action_to_avoid: String,
        causal_explanation: String,
        prescribed_remedy: String,
    }

    let critic_wisdom: CriticWisdom = serde_json::from_str(&response_text).or_else(|_| {
        let cleaned = crate::llm::strip_code_fences(&response_text);
        serde_json::from_str(&cleaned)
    })?;

    let active_scope = scope.unwrap_or_else(|| {
        std::env::var("MYTHRAX_ACTIVE_SCOPE").unwrap_or_else(|_| "general".to_string())
    });

    let rule_path = crate::cognitive::pipeline::resolve_rule_path(
        &active_scope,
        &critic_wisdom.action_to_avoid,
    );

    let rule_save = crate::contracts::WisdomRule {
        id: None,
        target_pattern: critic_wisdom.target_pattern,
        action_to_avoid: critic_wisdom.action_to_avoid,
        causal_explanation: critic_wisdom.causal_explanation,
        prescribed_remedy: critic_wisdom.prescribed_remedy,
        tier: crate::contracts::Tier::Project,
        scope: active_scope,
        vault_path: Some(rule_path.clone()),
        embedding: None,
        source_episodes: source_episode_id.clone().into_iter().collect(),
        generator_name: "LlmCritic".to_string(),
        similarity: None,
        utility: Some(50.0),
        status: None,
        superseded_at: None,
        superseded_by: None,
        rule_type: None,

        ..Default::default()
    };

    let markdown = crate::vault::watcher::format_wisdom_markdown(&rule_save);
    store.write_file(&rule_path, &markdown)?;
    let rule_id = backend.save_wisdom_rule(&rule_save).await?;

    if let Some(ref src_id) = source_episode_id {
        if let (Ok(from_thing), Ok(to_thing)) = (
            crate::db::parse_record_id(src_id),
            crate::db::parse_record_id(&rule_id),
        ) {
            let relate_sql = "RELATE $from -> relates_to -> $to UNIQUE CONTENT {
                relation: 'corrects',
                created_at: time::now(),
                confidence: 1.0
            };";
            let _ = backend
                .db
                .query(relate_sql)
                .bind(("from", from_thing))
                .bind(("to", to_thing))
                .await;
        }
    }

    Ok(())
}

pub async fn handle_cognitive_callback(state: &ApiState, args: Value) -> Result<Value> {
    let callback_id = args
        .get("callback_id")
        .or_else(|| args.get("id"))
        .and_then(|v| v.as_str())
        .context("Missing callback_id")?;
    let result = args
        .get("result")
        .or_else(|| args.get("output"))
        .and_then(|v| v.as_str())
        .context("Missing result")?;

    let surreal_backend = state
        .backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required for cognitive_callback")?;

    let task_opt = surreal_backend.get_cognitive_task(callback_id).await?;
    let task = match task_opt {
        Some(t) => t,
        None => anyhow::bail!("Task not found: {}", callback_id),
    };

    if task.status != "Injected" && task.status != "Expired" {
        anyhow::bail!("Invalid task status for callback: {}", task.status);
    }

    if task.expected_format.starts_with("Json") {
        if let Err(e) = serde_json::from_str::<serde_json::Value>(result) {
            anyhow::bail!("Invalid JSON format: {:?}", e);
        }
    }

    surreal_backend
        .update_cognitive_task_status(
            callback_id,
            crate::db::TaskStatus::Completed,
            Some(result.to_string()),
        )
        .await?;

    // Dynamic Episode Title Resolution & Renaming Pass
    if task.task_type == "Extraction" || task.system_instruction.contains("title generator") || task.prompt.contains("Generate a concise title") {
        let clean_title = result
            .lines()
            .next()
            .unwrap_or(result)
            .split(":")
            .last()
            .unwrap_or(result)
            .trim()
            .trim_matches('"');

        if !clean_title.is_empty() {
            if let Some(ref session_id) = task.session_id {
                let query = "SELECT id, vault_path, name FROM episode WHERE session_id = $session_id OR id = $session_id OR vault_path CONTAINS $session_id;";
                if let Ok(mut resp) = surreal_backend.db.query(query).bind(("session_id", session_id.clone())).await {
                    #[derive(serde::Deserialize, surrealdb::types::SurrealValue)]
                    struct EpRow {
                        id: surrealdb::types::RecordId,
                        vault_path: Option<String>,
                        name: String,
                    }
                    if let Ok(rows) = resp.take::<Vec<EpRow>>(0) {
                        for row in rows {
                            let update_sql = "UPDATE $ep_id SET name = $new_name;";
                            let _ = surreal_backend
                                .db
                                .query(update_sql)
                                .bind(("ep_id", row.id))
                                .bind(("new_name", clean_title.to_string()))
                                .await;
                        }
                    }
                }
            }
        }
    }

    // Downstream WikiNode Synthesis Persistence
    // Extract scope from [scope:...] prompt prefix if present, fall back to session_id.
    let scope_from_prompt: Option<String> = task.prompt.strip_prefix("[scope:").and_then(|rest| {
        rest.split_once(']').map(|(s, _)| s.trim().to_string())
    });
    let scope_name = scope_from_prompt.as_deref()
        .or(task.session_id.as_deref())
        .unwrap_or("general");
    if task.prompt.contains("Form hypotheses") || task.task_type == "Synthesis" || task.task_type == "Refinement"
        || task.prompt.contains("insight") || task.prompt.contains("direction") || task.prompt.contains("wisdom") {
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(result).or_else(|_| {
            let cleaned = crate::llm::strip_code_fences(result);
            serde_json::from_str::<serde_json::Value>(&cleaned)
        }) {
            if let Some(arr) = json_val.as_array().or_else(|| json_val.get("hypotheses").and_then(|h| h.as_array())).or_else(|| json_val.get("insights").and_then(|h| h.as_array())) {
                for item in arr {
                    let title = item.get("title").or_else(|| item.get("claim")).and_then(|v| v.as_str()).unwrap_or("Untitled");
                    let content = item.get("insight").or_else(|| item.get("content")).or_else(|| item.get("reasoning")).and_then(|v| v.as_str()).unwrap_or("");
                    let title_lower = title.to_lowercase();
                    let content_lower = content.to_lowercase();
                    let node_type = if task.prompt.contains("direction")
                        || title_lower.contains("always")
                        || title_lower.contains("never")
                        || title_lower.contains("make sure")
                        || title_lower.contains("do not")
                        || title_lower.contains("don't")
                        || title_lower.contains("must")
                        || title_lower.contains("use launchctl")
                        || content_lower.contains("always")
                        || content_lower.contains("never")
                        || content_lower.contains("must be")
                        || content_lower.contains("do not")
                    {
                        "direction"
                    } else if task.prompt.contains("wisdom") {
                        "rule"
                    } else {
                        "insight"
                    };
                    let slug = title.to_lowercase().replace(' ', "_").replace(|c: char| !c.is_alphanumeric() && c != '_', "");
                    let vault_path = if node_type == "rule" {
                        format!("wisdom/general/{}_rule.md", slug)
                    } else if node_type == "direction" {
                        format!("wiki/{}/directions/{}_direction.md", scope_name, slug)
                    } else {
                        format!("wiki/{}/{}_{}.md", scope_name, slug, node_type)
                    };
                    let wiki_node = crate::contracts::WikiNode {
                        id: None,
                        name: format!("{}/{}", scope_name, slug),
                        content: content.to_string(),
                        scope: scope_name.to_string(),
                        vault_path: Some(vault_path),
                        node_type: Some(node_type.to_string()),
                        ..Default::default()
                    };
                    let _ = surreal_backend.save_wiki_node(&wiki_node).await;
                }
            }
        }
    }

    let state_opt = surreal_backend.get_pipeline_state(callback_id).await?;
    if let Some(serialized_state) = state_opt {
        resume_pipeline_continuation(state, callback_id, &serialized_state, result).await?;
    }

    Ok(json!({ "status": "success", "callback_id": callback_id }))
}

async fn resume_pipeline_continuation(
    state: &ApiState,
    callback_id: &str,
    serialized_state: &str,
    result: &str,
) -> Result<()> {
    let state_val: serde_json::Value = serde_json::from_str(serialized_state)?;

    if let Some(target_file) = state_val.get("target_file").and_then(|v| v.as_str()) {
        let temp_file = format!("{}.tmp", target_file);
        std::fs::write(&temp_file, result)?;
        std::fs::rename(&temp_file, target_file)?;
    }

    let worktree_path = state_val.get("worktree_path").and_then(|v| v.as_str());
    let candidate_branch = state_val.get("candidate_branch").and_then(|v| v.as_str());

    if let (Some(path), Some(branch)) = (worktree_path, candidate_branch) {
        let _ = tokio::process::Command::new("git")
            .args(&["checkout", "-B", branch])
            .current_dir(path)
            .output()
            .await;
        let _ = tokio::process::Command::new("git")
            .args(&["reset", "--hard", "HEAD"])
            .current_dir(path)
            .output()
            .await;
    }

    if let Some(surreal_backend) = state.backend.as_any().downcast_ref::<SurrealBackend>() {
        surreal_backend.delete_pipeline_state(callback_id).await?;
    }

    Ok(())
}

pub async fn sweep_expired_tasks(state: &ApiState) -> Result<()> {
    let surreal_backend = state
        .backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required for sweep_expired_tasks")?;

    let expired = surreal_backend.get_injected_tasks_older_than_ttl().await?;
    for task in expired {
        surreal_backend
            .update_cognitive_task_status(&task.id, crate::db::TaskStatus::Expired, None)
            .await?;

        let config = surreal_backend.get_llm_config().await?;
        let sys_instr = if task.system_instruction.is_empty() {
            None
        } else {
            Some(task.system_instruction.as_str())
        };

        let response_text = crate::llm::LLMClient::default()
            .completion_explicit(
                surreal_backend,
                "local",
                &config.cloud_provider,
                &config.model,
                sys_instr,
                &task.prompt,
                false,
            )
            .await?;

        surreal_backend
            .update_cognitive_task_status(
                &task.id,
                crate::db::TaskStatus::Expired,
                Some(response_text.clone()),
            )
            .await?;

        let state_opt = surreal_backend.get_pipeline_state(&task.id).await?;
        if let Some(serialized_state) = state_opt {
            if let Err(e) =
                resume_pipeline_continuation(state, &task.id, &serialized_state, &response_text)
                    .await
            {
                tracing::error!(
                    "Failed to resume pipeline for expired task {}: {:?}",
                    task.id,
                    e
                );
            }
        }
    }
    Ok(())
}
