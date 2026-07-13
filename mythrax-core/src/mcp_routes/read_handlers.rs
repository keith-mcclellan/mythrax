use super::*;
use serde_json::{json, Value};
use anyhow::{Result, Context};
use crate::api::ApiState;
use crate::db::SurrealBackend;
use crate::cognitive::paging::intercept_and_restore_symbols;
use surrealdb_types::SurrealValue;

pub async fn handle_read(state: &ApiState, mut args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action parameter")?.to_string();
    let mapped_action = match action.as_str() {
        "view" | "view_file" => "view",
        "search" | "search_memory" => "search",
        "search_index" => "search_index",
        "rules" | "search_wisdom" => "rules",
        "nodes" | "get_memory_nodes" => "nodes",
        "query_symbolic" => "query_symbolic",
        "timeline" => "timeline",
        "get_full" => "get_full",
        "root" | "get_vault_root" => "root",
        "get" | "get_short_term" | "get_config" => "get",
        "search_by_concept" => "search_by_concept",
        "diff_sessions" => "diff_sessions",
        other => other,
    };
    if let Some(obj) = args.as_object_mut() {
        obj.insert("action".to_string(), serde_json::Value::String(mapped_action.to_string()));
    }

    match mapped_action {
        "view" => {
            let _path = args.get("path")
                .or_else(|| args.get("AbsolutePath"))
                .or_else(|| args.get("TargetFile"))
                .and_then(|v| v.as_str())
                .context("Missing path/AbsolutePath/TargetFile")?;
            super::manage_handlers::handle_manage_file(state, args).await
        }
        "search" | "search_index" => {
            let _query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            handle_query_memory(state, args).await
        }
        "rules" => {
            let _query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            handle_query_memory(state, args).await
        }
        "nodes" => {
            let node_ids_val = args.get("node_ids").context("Missing node_ids")?;
            let _node_ids_arr = node_ids_val.as_array().context("node_ids must be an array")?;
            handle_query_memory(state, args).await
        }
        "query_symbolic" => {
            let _node_id = args.get("node_id").and_then(|v| v.as_str()).context("Missing node_id")?;
            handle_query_memory(state, args).await
        }
        "timeline" => {
            if args.get("anchor_id").and_then(|v| v.as_str()).is_none() && args.get("query").and_then(|v| v.as_str()).is_none() {
                anyhow::bail!("Either anchor_id or query must be provided for timeline");
            }
            handle_query_memory(state, args).await
        }
        "get_full" => {
            if args.get("ids").and_then(|v| v.as_array()).is_none() && args.get("node_ids").and_then(|v| v.as_array()).is_none() {
                anyhow::bail!("Missing ids or node_ids array parameter");
            }
            handle_query_memory(state, args).await
        }
        "root" => {
            handle_query_memory(state, args).await
        }
        "get" => {
            if action == "get_short_term" || (action == "get" && (args.get("key").and_then(|v| v.as_str()).is_some() || args.get("session_id").and_then(|v| v.as_str()).is_some())) {
                let _session_id = args.get("session_id").and_then(|v| v.as_str()).context("Missing session_id")?;
                let _key = args.get("key").and_then(|v| v.as_str()).context("Missing key")?;
                super::manage_handlers::handle_manage_stm(state, args).await
            } else {
                super::manage_handlers::handle_manage_config(state, args).await
            }
        }
        "search_by_concept" => {
            let concept = args.get("concept").and_then(|v| v.as_str()).context("Missing concept")?;
            let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;
            search_by_concept_db(surreal_backend, concept).await
        }
        "diff_sessions" => {
            let session_a = args.get("session_a").and_then(|v| v.as_str()).context("Missing session_a")?;
            let session_b = args.get("session_b").and_then(|v| v.as_str()).context("Missing session_b")?;
            let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;
            diff_sessions_db(surreal_backend, session_a, session_b).await
        }
        _ => anyhow::bail!("Invalid action for read tool: {}", action),
    }
}

pub async fn handle_query_memory(state: &ApiState, args: Value) -> Result<Value> {
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required")?;
    let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("search");
    match action {
        "search" => {
            let query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            let scope = args.get("scope").and_then(|v| v.as_str());
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let threshold = args.get("threshold").and_then(|v| v.as_f64()).map(|t| t as f32).unwrap_or(0.55);
            let token_budget = args.get("token_budget").and_then(|v| v.as_u64()).map(|t| t as usize);
            let allow_downward = args.get("allow_downward").and_then(|v| v.as_bool()).unwrap_or(false);
            let include_episodes = args.get("include_episodes").and_then(|v| v.as_bool()).unwrap_or(false);
            let include_artifacts = args.get("include_artifacts").and_then(|v| v.as_bool()).unwrap_or(false);
            let session_id = args.get("session_id").and_then(|v| v.as_str());
            let include_archived = args.get("include_archived").and_then(|v| v.as_bool()).unwrap_or(true);
            let temporal_anchor = args.get("temporal_anchor").and_then(|v| v.as_str());

            let search_res = state.backend.search(crate::contracts::SearchParams::from_positional(
                query,
                scope,
                false,
                limit,
                offset,
                threshold,
                token_budget,
                allow_downward,
                include_episodes,
                include_artifacts,
                session_id,
                include_archived,
                temporal_anchor,
            )).await?;
            
            if let Some(sess_id) = session_id {
                let mut cited_ids = Vec::new();
                for r in &search_res.results {
                    if r.tier == crate::contracts::Tier::Session {
                        cited_ids.push(r.id.clone());
                    }
                }
                if !cited_ids.is_empty() {
                    let mut existing_citations = Vec::new();
                    if let Ok(stm_map) = state.backend.get_stm(sess_id, Some("_session_citations")).await {
                        if let Some(existing_str) = stm_map.get("_session_citations") {
                            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(existing_str) {
                                existing_citations = parsed;
                            }
                        }
                    }
                    existing_citations.extend(cited_ids);
                    existing_citations.sort();
                    existing_citations.dedup();
                    if let Ok(serialized) = serde_json::to_string(&existing_citations) {
                        let _ = state.backend.save_stm(sess_id, "_session_citations", &serialized).await;
                    }
                }
            }

            let stripped_results: Vec<Value> = search_res.results.into_iter().map(|mut r| {
                r.embedding = None;
                let mut v = serde_json::to_value(&r).unwrap();
                strip_nulls(&mut v);
                v
            }).collect();

            let mut text = serde_json::to_string_pretty(&stripped_results)?;
            if search_res.has_more {
                let remainder = search_res.total_matches.saturating_sub(offset + limit);
                text.push_str(&format!(
                    "\n\n=== PAGINATION NOTICE: There are {} more matching memories. To retrieve the next page, call read(action=\"search\", offset={}, limit={}). ===",
                    remainder, search_res.next_offset, limit
                ));
            }

            if let Some(ref omitted) = search_res.omitted_ids {
                if !omitted.is_empty() {
                    text.push_str(&format!(
                        "\n\n=== BUDGET NOTICE: The following record IDs were omitted due to token budget limits ({} tokens):\n{:?} ===",
                        token_budget.unwrap_or(0), omitted
                    ));
                }
            }

            text = intercept_and_restore_symbols(surreal_backend, &text).await;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "search_index" => {
            let query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            let scope = args.get("scope").and_then(|v| v.as_str());
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let threshold = args.get("threshold").and_then(|v| v.as_f64()).map(|t| t as f32).unwrap_or(0.55);
            let token_budget = args.get("token_budget").and_then(|v| v.as_u64()).map(|t| t as usize);
            let allow_downward = args.get("allow_downward").and_then(|v| v.as_bool()).unwrap_or(false);
            let session_id = args.get("session_id").and_then(|v| v.as_str());
            let include_archived = args.get("include_archived").and_then(|v| v.as_bool()).unwrap_or(true);
            let temporal_anchor = args.get("temporal_anchor").and_then(|v| v.as_str());

            let search_res = state.backend.search(crate::contracts::SearchParams::from_positional(
                query,
                scope,
                false,
                limit,
                offset,
                threshold,
                token_budget,
                allow_downward,
                true,
                false,
                session_id,
                include_archived,
                temporal_anchor,
            )).await?;

            let mut index_rows = Vec::new();
            for r in search_res.results {
                if r.tier == crate::contracts::Tier::Session {
                    let subtitle = make_subtitle(&r.content);
                    index_rows.push(crate::contracts::IndexRow {
                        id: r.id,
                        title: r.title,
                        subtitle,
                        similarity: r.similarity,
                    });
                }
            }

            let text = serde_json::to_string_pretty(&index_rows)?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "timeline" => {
            let anchor_id = args.get("anchor_id").and_then(|v| v.as_str());
            let query = args.get("query").and_then(|v| v.as_str());
            let depth_before = args.get("depth_before").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
            let depth_after = args.get("depth_after").and_then(|v| v.as_u64()).unwrap_or(3) as usize;

            let resolved_anchor_id = if let Some(id) = anchor_id {
                id.to_string()
            } else if let Some(q) = query {
                let search_res = state.backend.search(crate::contracts::SearchParams::from_positional(
                    q,
                    None,
                    false,
                    1,
                    0,
                    0.0,
                    None,
                    false,
                    true,
                    false,
                    None,
                    true,
                    None,
                )).await?;
                let best = search_res.results.first().context("No matching anchor episode found for query")?;
                best.id.clone()
            } else {
                anyhow::bail!("Either anchor_id or query must be provided for timeline");
            };

            let anchor_record = crate::db::backend::parse_record_id(&resolved_anchor_id)?;
            
            #[derive(serde::Deserialize, Debug, surrealdb_types::SurrealValue)]
            struct AnchorRow {
                created_at: chrono::DateTime<chrono::Utc>,
            }

            let mut response = surreal_backend.db.query("SELECT created_at FROM $id;")
                .bind(("id", anchor_record))
                .await?;
            let anchor_rows: Vec<AnchorRow> = response.take(0)?;
            let anchor_row = anchor_rows.into_iter().next().context("Anchor episode not found in database")?;
            let anchor_time = anchor_row.created_at;

            #[derive(serde::Deserialize, Debug, surrealdb_types::SurrealValue)]
            struct EpisodeQueryResult {
                id: surrealdb::types::RecordId,
                title: String,
                content: String,
                created_at: chrono::DateTime<chrono::Utc>,
            }

            let mut response_before = surreal_backend.db.query("SELECT id, title, content, created_at FROM episode WHERE created_at < $created_at ORDER BY created_at DESC LIMIT $limit;")
                .bind(("created_at", anchor_time))
                .bind(("limit", depth_before))
                .await?;
            let mut before_rows: Vec<EpisodeQueryResult> = response_before.take(0)?;
            before_rows.reverse();

            let mut response_after = surreal_backend.db.query("SELECT id, title, content, created_at FROM episode WHERE created_at > $created_at ORDER BY created_at ASC LIMIT $limit;")
                .bind(("created_at", anchor_time))
                .bind(("limit", depth_after))
                .await?;
            let after_rows: Vec<EpisodeQueryResult> = response_after.take(0)?;

            let mut index_rows = Vec::new();
            for r in before_rows {
                let subtitle = make_subtitle(&r.content);
                index_rows.push(crate::contracts::IndexRow {
                    id: crate::db::backend::format_record_id(&r.id),
                    title: r.title,
                    subtitle,
                    similarity: 0.0,
                });
            }
            for r in after_rows {
                let subtitle = make_subtitle(&r.content);
                index_rows.push(crate::contracts::IndexRow {
                    id: crate::db::backend::format_record_id(&r.id),
                    title: r.title,
                    subtitle,
                    similarity: 0.0,
                });
            }

            let text = serde_json::to_string_pretty(&index_rows)?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "get_full" => {
            let ids = if let Some(ids_val) = args.get("ids").and_then(|v| v.as_array()) {
                ids_val.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
            } else if let Some(node_ids_val) = args.get("node_ids").and_then(|v| v.as_array()) {
                node_ids_val.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
            } else {
                anyhow::bail!("Missing ids or node_ids array parameter");
            };

            let hydrated = state.backend.get_memory_nodes(&ids).await?;
            
            let mut results = Vec::new();
            const MAX_HYDRATION_CHARS: usize = 10000;
            for ep in hydrated.episodes {
                let content = if ep.content.chars().count() > MAX_HYDRATION_CHARS {
                    let truncated_len = ep.content.chars().count() - MAX_HYDRATION_CHARS;
                    let truncated: String = ep.content.chars().take(MAX_HYDRATION_CHARS).collect();
                    format!("{}... [truncated {} chars]", truncated, truncated_len)
                } else {
                    ep.content.clone()
                };
                results.push(crate::contracts::SearchResult {
                    id: ep.id.clone().unwrap_or_default(),
                    title: ep.title.clone(),
                    content,
                    similarity: 1.0,
                    utility: ep.utility.unwrap_or(0.0),
                    tier: crate::contracts::Tier::Session,
                    embedding: None,
                    vault_path: ep.vault_path.clone(),
                    source_episode: ep.source_episode.clone(),
                    discovery_tokens: ep.discovery_tokens,
                    related_nodes: None,
                    ..Default::default()
                });
            }
            for wiki in hydrated.wiki_nodes {
                let content = if wiki.content.chars().count() > MAX_HYDRATION_CHARS {
                    let truncated_len = wiki.content.chars().count() - MAX_HYDRATION_CHARS;
                    let truncated: String = wiki.content.chars().take(MAX_HYDRATION_CHARS).collect();
                    format!("{}... [truncated {} chars]", truncated, truncated_len)
                } else {
                    wiki.content.clone()
                };
                results.push(crate::contracts::SearchResult {
                    id: wiki.id.clone().unwrap_or_default(),
                    title: wiki.name.clone(),
                    content,
                    similarity: 1.0,
                    utility: 0.0,
                    tier: crate::contracts::Tier::Project,
                    embedding: None,
                    vault_path: wiki.vault_path.clone(),
                    source_episode: None,
                    discovery_tokens: None,
                    related_nodes: None,
                    ..Default::default()
                });
            }
            for rule in hydrated.wisdom_rules {
                let raw_content = format!(
                    "Avoid: {}\nCausal: {}\nRemedy: {}",
                    rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
                );
                let content = if raw_content.chars().count() > MAX_HYDRATION_CHARS {
                    let truncated_len = raw_content.chars().count() - MAX_HYDRATION_CHARS;
                    let truncated: String = raw_content.chars().take(MAX_HYDRATION_CHARS).collect();
                    format!("{}... [truncated {} chars]", truncated, truncated_len)
                } else {
                    raw_content
                };
                results.push(crate::contracts::SearchResult {
                    id: rule.id.clone().unwrap_or_default(),
                    title: rule.target_pattern.clone(),
                    content,
                    similarity: rule.similarity.unwrap_or(1.0),
                    utility: rule.utility.unwrap_or(0.0) as f32,
                    tier: crate::contracts::Tier::Wisdom,
                    embedding: None,
                    vault_path: rule.vault_path.clone(),
                    source_episode: None,
                    discovery_tokens: None,
                    related_nodes: None,
                    ..Default::default()
                });
            }

            let text = serde_json::to_string_pretty(&results)?;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "rules" => {
            let query = args.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            let tier = args.get("tier").and_then(|v| v.as_str()).and_then(|t| t.parse::<crate::contracts::Tier>().ok());
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let threshold = args.get("threshold").and_then(|v| v.as_f64()).map(|t| t as f32).unwrap_or(0.55);

            let search_res = state.backend.get_wisdom(query, tier, limit, offset, threshold).await?;
            let stripped_results: Vec<Value> = search_res.results.into_iter().map(|mut r| {
                r.embedding = None;
                let mut v = serde_json::to_value(&r).unwrap();
                strip_nulls(&mut v);
                v
            }).collect();

            let mut text = serde_json::to_string_pretty(&stripped_results)?;
            if search_res.has_more {
                let remainder = search_res.total_matches.saturating_sub(offset + limit);
                text.push_str(&format!(
                    "\n\n=== PAGINATION NOTICE: There are {} more matching wisdom rules. To retrieve the next page, call read(action=\"rules\", offset={}, limit={}). ===",
                    remainder, search_res.next_offset, limit
                ));
            }

            text = intercept_and_restore_symbols(surreal_backend, &text).await;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "nodes" => {
            let node_ids_val = args.get("node_ids").context("Missing node_ids")?;
            let node_ids_arr = node_ids_val.as_array().context("node_ids must be an array")?;
            let mut node_ids = Vec::new();
            for v in node_ids_arr {
                if let Some(s) = v.as_str() {
                    node_ids.push(s.to_string());
                }
            }

            let response = state.backend.get_memory_nodes(&node_ids).await?;
            let mut stripped_response = response.clone();
            for ep in &mut stripped_response.episodes {
                ep.embedding = None;
            }
            for r in &mut stripped_response.wisdom_rules {
                r.embedding = None;
            }
            for node in &mut stripped_response.wiki_nodes {
                node.embedding = None;
            }

            let mut text = serde_json::to_string_pretty(&stripped_response)?;
            text = intercept_and_restore_symbols(surreal_backend, &text).await;
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "query_symbolic" => {
            let node_id = args.get("node_id").and_then(|v| v.as_str()).context("Missing node_id")?;
            let relation = args.get("relation").and_then(|v| v.as_str());
            let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).map(|v| v as usize);

            let traversed_ids = state.backend.query_symbolic(node_id, relation, max_depth).await?;
            let text = serde_json::to_string_pretty(&traversed_ids)?;
            
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }))
        }
        "root" => {
            let vault_path = state.store.vault_root.to_string_lossy().to_string();
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": vault_path
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for query_memory: {}", action),
    }
}

fn make_subtitle(content: &str) -> String {
    let char_count = content.chars().count();
    if char_count <= 120 {
        content.to_string()
    } else {
        let truncated: String = content.chars().take(120).collect();
        format!("{}...", truncated)
    }
}

pub async fn search_by_concept_db(backend: &SurrealBackend, concept: &str) -> Result<Value> {
    let sql = "SELECT * FROM episode WHERE archived = false AND ($concept IN concepts OR $concept IN facts OR string::contains(title, $concept) OR string::contains(content, $concept));";
    let mut response = backend.db.query(sql).bind(("concept", concept)).await?.check()?;
    let mut episodes: Vec<crate::contracts::Episode> = response.take(0)?;
    for ep in &mut episodes {
        ep.embedding = None;
    }
    
    let sql_wiki = "SELECT * FROM wiki_node WHERE string::contains(name, $concept) OR string::contains(content, $concept);";
    let mut resp_wiki = backend.db.query(sql_wiki).bind(("concept", concept)).await?.check()?;
    let mut wiki_nodes: Vec<crate::contracts::WikiNode> = resp_wiki.take(0)?;
    for wk in &mut wiki_nodes {
        wk.embedding = None;
    }

    let sql_wisdom = "SELECT * FROM wisdom WHERE string::contains(target_pattern, $concept) OR string::contains(action_to_avoid, $concept) OR string::contains(causal_explanation, $concept) OR string::contains(prescribed_remedy, $concept);";
    let mut resp_wisdom = backend.db.query(sql_wisdom).bind(("concept", concept)).await?.check()?;
    let mut wisdom_rules: Vec<crate::contracts::WisdomRule> = resp_wisdom.take(0)?;
    for ws in &mut wisdom_rules {
        ws.embedding = None;
    }

    Ok(json!({
        "episodes": episodes,
        "wiki_nodes": wiki_nodes,
        "wisdom_rules": wisdom_rules,
    }))
}

pub async fn diff_sessions_db(backend: &SurrealBackend, session_a: &str, session_b: &str) -> Result<Value> {
    let sql_a = "SELECT role, content FROM chat_history WHERE session_id = $session_a ORDER BY created_at ASC;";
    let mut resp_a = backend.db.query(sql_a).bind(("session_a", session_a)).await?.check()?;
    let chat_a: Vec<Value> = resp_a.take(0)?;

    let sql_b = "SELECT role, content FROM chat_history WHERE session_id = $session_b ORDER BY created_at ASC;";
    let mut resp_b = backend.db.query(sql_b).bind(("session_b", session_b)).await?.check()?;
    let chat_b: Vec<Value> = resp_b.take(0)?;

    let text_a = chat_a.iter().map(|msg| {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("unknown");
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        format!("{}: {}", role, content)
    }).collect::<Vec<_>>().join("\n");

    let text_b = chat_b.iter().map(|msg| {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("unknown");
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        format!("{}: {}", role, content)
    }).collect::<Vec<_>>().join("\n");

    let uuid = uuid::Uuid::new_v4().to_string();
    let path_a = std::env::temp_dir().join(format!("diff_a_{}.txt", uuid));
    let path_b = std::env::temp_dir().join(format!("diff_b_{}.txt", uuid));

    std::fs::write(&path_a, &text_a)?;
    std::fs::write(&path_b, &text_b)?;

    let output = std::process::Command::new("diff")
        .args(["-u", path_a.to_str().unwrap(), path_b.to_str().unwrap()])
        .output()?;

    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);

    let diff_text = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(json!({
        "session_a": session_a,
        "session_b": session_b,
        "diff": diff_text
    }))
}
