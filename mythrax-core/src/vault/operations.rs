use anyhow::Result;
use crate::contracts;
use crate::db::backend::StorageBackend;
use crate::vault;
use crate::llm;

/// Merges conflicting wisdom rules in the shared vault directory.
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
                            tier: frontmatter.get("tier").and_then(|v| v.as_str()).unwrap_or("dynamic").parse::<contracts::Tier>().unwrap_or(contracts::Tier::Project),
                            scope: frontmatter.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string(),
                            vault_path: Some(file_path.to_string_lossy().to_string()),
                            embedding: None,
                            source_episodes: Vec::new(),
                            generator_name: frontmatter.get("generator_name").and_then(|v| v.as_str()).unwrap_or("manual").to_string(),
                            similarity: None,
                            utility: frontmatter.get("utility").and_then(|v| v.as_f64()).map(|u| u as f32),
                            status: None,
                            superseded_at: None,
                            superseded_by: None,
                            rule_type: None,
                        
                            ..Default::default()
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
            let mut max_tier = contracts::Tier::Project;
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
                if r.tier == contracts::Tier::Wisdom {
                    max_tier = r.tier;
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
                status: None,
                superseded_at: None,
                superseded_by: None,
                rule_type: None,
            
                ..Default::default()
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

/// Converts a record ID value to a string format.
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

/// Runs the auditor safety/compliance checks and self-healing memory threshold calibration.
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
        let synthetic_query = match client.routed_completion(backend, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Extraction), Some(system_prompt), &prompt).await {
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

        let search_res = backend.search(crate::contracts::SearchParams::from_positional(
        &synthetic_query,
        Some("all"),
        false,
        10,
        0,
        threshold,
        None,
        false,
        true,
        false,
        None,
        true,
        None,
    )).await?;

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

/// Recursively scans the filesystem vault and indexes all markdown files into SurrealDB.
pub async fn sync_vault_to_db(
    backend: &std::sync::Arc<dyn StorageBackend>,
    store: &std::sync::Arc<crate::store::MarkdownStore>,
) -> Result<usize> {
    let mut count = 0;
    let mut dirs_to_scan = vec![store.vault_root.clone()];
    
    let cache = if let Some(surreal_backend) = backend.as_any().downcast_ref::<crate::db::SurrealBackend>() {
        Some(crate::vault::watcher::TargetResolveCache::new(surreal_backend).await)
    } else {
        None
    };
    
    while let Some(dir) = dirs_to_scan.pop() {
        if !dir.exists() {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!("Failed to read directory {:?}: {:?}", dir, err);
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!("Failed to read directory entry: {:?}", err);
                    continue;
                }
            };
            let path = entry.path();
            if path.is_dir() {
                dirs_to_scan.push(path);
            } else if path.is_file() {
                if path.extension().map_or(false, |ext| ext == "md") {
                    match crate::vault::watcher::sync_file_to_db_with_cache(&path, backend, store, cache.as_ref()).await {
                        Ok(_) => {
                            count += 1;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to sync file {:?} to DB: {:?}", path, e);
                        }
                    }
                }
            }
        }
    }
    
    Ok(count)
}
