use crate::db::StorageBackend;
use crate::llm::LLMClient;
use crate::store::MarkdownStore;
use crate::contracts::{Episode, WisdomRule, WikiNode};
use anyhow::Result;
use std::path::Path;
use std::collections::HashMap;

fn dot_product(u: &[f32], v: &[f32]) -> f32 {
    u.iter().zip(v.iter()).map(|(a, b)| a * b).sum()
}

pub fn cosine_distance(u: &[f32], v: &[f32]) -> f32 {
    1.0 - dot_product(u, v)
}

pub fn dbscan(
    embeddings: &[&[f32]],
    eps: f32,
    min_samples: usize,
) -> Vec<Option<usize>> {
    let n = embeddings.len();
    let mut labels = vec![None; n];
    let mut cluster_id = 0;

    for i in 0..n {
        if labels[i].is_some() {
            continue;
        }

        let mut neighbors = find_neighbors(i, embeddings, eps);
        if neighbors.len() < min_samples {
            continue;
        }

        labels[i] = Some(cluster_id);
        let mut j = 0;
        while j < neighbors.len() {
            let neighbor_idx = neighbors[j];
            if labels[neighbor_idx].is_none() {
                labels[neighbor_idx] = Some(cluster_id);
                let neighbor_neighbors = find_neighbors(neighbor_idx, embeddings, eps);
                if neighbor_neighbors.len() >= min_samples {
                    for &nn in &neighbor_neighbors {
                        if !neighbors.contains(&nn) {
                            neighbors.push(nn);
                        }
                    }
                }
            }
            j += 1;
        }
        cluster_id += 1;
    }

    labels
}

fn find_neighbors(i: usize, embeddings: &[&[f32]], eps: f32) -> Vec<usize> {
    let mut neighbors = Vec::new();
    let target = embeddings[i];
    for (idx, &emb) in embeddings.iter().enumerate() {
        if cosine_distance(target, emb) <= eps {
            neighbors.push(idx);
        }
    }
    neighbors
}

#[derive(serde::Deserialize, Default)]
struct DreamSettings {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    eps: Option<f32>,
    #[serde(default)]
    min_samples: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct InsightNote {
    pub title: String,
    pub content: String,
    pub scope: String,
    pub source_episodes: Vec<String>,
    pub vault_path: String,
}

pub fn load_insights(vault_root: &Path) -> Vec<InsightNote> {
    let mut insights = Vec::new();
    let wiki_dir = vault_root.join("wiki");
    if !wiki_dir.exists() {
        return insights;
    }
    if let Ok(entries) = std::fs::read_dir(&wiki_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let scope = entry.file_name().to_string_lossy().to_string();
                let insights_dir = entry.path().join("insights");
                if insights_dir.exists()
                    && let Ok(files) = std::fs::read_dir(&insights_dir) {
                        for file in files.flatten() {
                            if file.path().extension().map(|s| s == "md").unwrap_or(false)
                                && let Ok(content) = std::fs::read_to_string(file.path())
                                    && let Ok(note) = parse_insight_note(&content, &file.path(), &scope) {
                                        insights.push(note);
                                    }
                        }
                    }
            }
        }
    }
    insights
}

fn parse_insight_note(content: &str, path: &Path, scope: &str) -> Result<InsightNote> {
    if !content.starts_with("---") {
        anyhow::bail!("No frontmatter");
    }
    let parts: Vec<&str> = content.split("---").collect();
    if parts.len() < 3 {
        anyhow::bail!("Invalid frontmatter");
    }
    let yaml_str = parts[1];
    let body = parts[2..].join("---");

    #[derive(serde::Deserialize)]
    struct Frontmatter {
        title: String,
        source_episodes: Vec<String>,
    }
    let fm: Frontmatter = serde_yaml::from_str(yaml_str)?;
    Ok(InsightNote {
        title: fm.title,
        content: body.trim().to_string(),
        scope: scope.to_string(),
        source_episodes: fm.source_episodes,
        vault_path: path.to_string_lossy().to_string(),
    })
}

fn calculate_centroid(
    source_episodes: &[String],
    all_episodes: &[Episode],
) -> Option<Vec<f32>> {
    let mut sum = Vec::new();
    let mut count = 0;
    for ep_id in source_episodes {
        if let Some(ep) = all_episodes.iter().find(|e| e.id.as_ref() == Some(ep_id))
            && let Some(ref emb) = ep.embedding {
                if sum.is_empty() {
                    sum = vec![0.0; emb.len()];
                }
                for (i, val) in emb.iter().enumerate() {
                    sum[i] += val;
                }
                count += 1;
            }
    }
    if count > 0 {
        for val in &mut sum {
            *val /= count as f32;
        }
        let norm: f32 = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut sum {
                *val /= norm;
            }
        }
        Some(sum)
    } else {
        None
    }
}

pub struct DreamCoordinator {
    llm: LLMClient,
}

impl Default for DreamCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl DreamCoordinator {
    pub fn new() -> Self {
        Self {
            llm: LLMClient::new(),
        }
    }

    pub async fn run_dream(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
        mode_override: Option<&str>,
    ) -> Result<()> {
        let settings_path = store.vault_root.join("wiki/dream_settings.md");
        let mut active_mode = "incremental".to_string();
        let mut file_eps = None;
        let mut file_min_samples = None;

        if settings_path.exists()
            && let Ok(content) = std::fs::read_to_string(&settings_path) {
                let yaml_str = if content.starts_with("---") {
                    let parts: Vec<&str> = content.split("---").collect();
                    if parts.len() >= 3 { parts[1] } else { &content }
                } else {
                    &content
                };
                if let Ok(settings) = serde_yaml::from_str::<DreamSettings>(yaml_str) {
                    if let Some(m) = settings.mode {
                        active_mode = m;
                    }
                    file_eps = settings.eps;
                    file_min_samples = settings.min_samples;
                }
            }

        if let Some(mo) = mode_override {
            active_mode = mo.to_string();
        }

        let (default_eps, default_min_samples) = match active_mode.as_str() {
            "deep" => (0.15, 2),
            "bulk" => (0.12, 4),
            _ => (0.08, 2), // incremental
        };

        let eps = file_eps.unwrap_or(default_eps);
        let min_samples = file_min_samples.unwrap_or(default_min_samples);

        let all_episodes = db.get_all_episodes().await?;
        let unprocessed = db.get_unprocessed_episodes().await?;

        if unprocessed.is_empty() {
            return Ok(());
        }

        let mut scope_groups: HashMap<String, Vec<Episode>> = HashMap::new();
        for ep in unprocessed {
            let scope = ep.scope.clone().unwrap_or_else(|| "general".to_string());
            scope_groups.entry(scope).or_default().push(ep);
        }

        let total_scopes = scope_groups.len();
        for (scope_idx, (scope, new_episodes)) in scope_groups.into_iter().enumerate() {
            let mut candidates = Vec::new();

            if active_mode == "incremental" {
                let existing_insights = load_insights(&store.vault_root);
                let scope_insights: Vec<InsightNote> = existing_insights
                    .into_iter()
                    .filter(|ins| ins.scope == scope)
                    .collect();

                let mut centroids = Vec::new();
                for ins in &scope_insights {
                    if let Some(cent) = calculate_centroid(&ins.source_episodes, &all_episodes) {
                        centroids.push((ins.clone(), cent));
                    }
                }

                let total_new_episodes = new_episodes.len();
                for (ep_idx, ep) in new_episodes.into_iter().enumerate() {
                    let mut matched_insight: Option<(&InsightNote, f32)> = None;
                    if let Some(ref ep_emb) = ep.embedding {
                        for (ins, cent) in &centroids {
                            let dist = cosine_distance(ep_emb, cent);
                            if dist < 0.25 {
                                if let Some((_, best_dist)) = matched_insight {
                                    if dist < best_dist {
                                        matched_insight = Some((ins, dist));
                                    }
                                } else {
                                    matched_insight = Some((ins, dist));
                                }
                            }
                        }
                    }

                    if let Some((ins, _)) = matched_insight {
                        let mut new_source_episodes = ins.source_episodes.clone();
                        let ep_id_str = ep.id.clone().unwrap_or_default();
                        if !new_source_episodes.contains(&ep_id_str) {
                            new_source_episodes.push(ep_id_str);
                        }

                        let sys_prompt = "You are a systems synthesizer. Refine the existing architectural insight note by incorporating the details of the new event.";
                        let content_len = ep.content.len();
                        let display_content = if content_len > 8000 {
                            format!("{}... [Truncated {} characters of content due to size]", &ep.content[..8000], content_len - 8000)
                        } else {
                            ep.content.clone()
                        };
                        let prompt_text = format!(
                            "Existing Insight Body:\n{}\n\nNew Event content:\nTitle: {}\n{}",
                            ins.content, ep.title, display_content
                        );
                        let updated_summary = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

                        let relative_path = format!("wiki/{}/insights/{}.md", scope, ins.title.replace(' ', "_"));
                        let new_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource_episodes:\n{}\n---\n\n{}",
                            ins.title,
                            scope,
                            new_source_episodes.iter().map(|id| format!("  - \"{}\"", id)).collect::<Vec<_>>().join("\n"),
                            updated_summary
                        );
                        store.write_file(&relative_path, &new_content)?;

                        if let Some(ref ep_id) = ep.id {
                            db.mark_episode_processed(ep_id).await?;
                        }

                        let node_contract = WikiNode {
                            id: None,
                            name: ins.title.clone(),
                            content: updated_summary.clone(),
                            scope: scope.clone(),
                            vault_path: Some(relative_path.clone()),
                            embedding: None,
                        };
                        if let Ok(wiki_node_id) = db.save_wiki_node(&node_contract).await
                            && let Some(ref ep_id) = ep.id {
                                let _ = db.relate_nodes(ep_id, &wiki_node_id).await;
                            }

                        tracing::info!(
                            "Dreaming scope {}/{} ('{}'): incremental episode {} of {} complete (merged into '{}')",
                            scope_idx + 1,
                            total_scopes,
                            scope,
                            ep_idx + 1,
                            total_new_episodes,
                            ins.title
                        );
                    } else {
                        candidates.push(ep);
                    }
                }
            } else {
                candidates = new_episodes;
            }

            let candidate_embs: Vec<&[f32]> = candidates
                .iter()
                .filter_map(|ep| ep.embedding.as_deref())
                .collect();

            let valid_candidates: Vec<&Episode> = candidates
                .iter()
                .filter(|ep| ep.embedding.is_some())
                .collect();

            if valid_candidates.is_empty() {
                continue;
            }

            let labels = dbscan(&candidate_embs, eps, min_samples);

            let mut clusters: HashMap<usize, Vec<&Episode>> = HashMap::new();
            for (idx, &label) in labels.iter().enumerate() {
                if let Some(lbl) = label {
                    clusters.entry(lbl).or_default().push(valid_candidates[idx]);
                }
            }

            let total_clusters = clusters.len();
            for (cluster_idx, (_, cluster_eps)) in clusters.into_iter().enumerate() {
                let mut events_text = String::new();
                for ep in &cluster_eps {
                    let content_len = ep.content.len();
                    let display_content = if content_len > 8000 {
                        format!("{}... [Truncated {} characters of content due to size]", &ep.content[..8000], content_len - 8000)
                    } else {
                        ep.content.clone()
                    };
                    events_text.push_str(&format!("Event: {}\nContent:\n{}\n\n", ep.title, display_content));
                }

                let sys_prompt = "You are a systems synthesizer. Analyze the cluster of events and output a JSON object containing a 'title' field and a 'summary' field summarizing the architectural decisions, patterns, or habits observed.";
                let prompt_text = format!(
                    "Please analyze these events:\n\n{}Respond ONLY with JSON matching: {{ \"title\": \"...\", \"summary\": \"...\" }}",
                    events_text
                );

                let llm_res = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;
                
                #[derive(serde::Deserialize)]
                struct ClusterAnalysis {
                    title: String,
                    summary: String,
                }
                
                let analysis: ClusterAnalysis = match serde_json::from_str(&llm_res) {
                    Ok(a) => a,
                    Err(_) => {
                        ClusterAnalysis {
                            title: format!("Cluster Analysis {}", &uuid::Uuid::new_v4().to_string()[..8]),
                            summary: llm_res,
                        }
                    }
                };

                let cluster_ep_ids: Vec<String> = cluster_eps.iter().map(|ep| ep.id.clone().unwrap_or_default()).collect();

                let clean_title = analysis.title.replace([' ', '/'], "_");
                let insight_uuid = uuid::Uuid::new_v4().to_string();
                let relative_path = format!("wiki/{}/insights/{}_{}.md", scope, clean_title, &insight_uuid[..8]);
                let insight_content = format!(
                    "---\ntitle: \"{}\"\nscope: \"{}\"\nsource_episodes:\n{}\n---\n\n{}",
                    analysis.title,
                    scope,
                    cluster_ep_ids.iter().map(|id| format!("  - \"{}\"", id)).collect::<Vec<_>>().join("\n"),
                    analysis.summary
                );
                store.write_file(&relative_path, &insight_content)?;

                let node_contract = WikiNode {
                    id: None,
                    name: analysis.title.clone(),
                    content: analysis.summary.clone(),
                    scope: scope.clone(),
                    vault_path: Some(relative_path.clone()),
                    embedding: None,
                };
                if let Ok(wiki_node_id) = db.save_wiki_node(&node_contract).await {
                    for ep_id in &cluster_ep_ids {
                        let _ = db.relate_nodes(ep_id, &wiki_node_id).await;
                    }
                }

                let sys_wisdom = "You are a systems synthesizer. Analyze the events and extract system-level Wisdom Rules to avoid mistakes. Respond with a JSON array of rules.";
                let prompt_wisdom = format!(
                    "Events:\n\n{}Respond ONLY with a JSON array of objects, each containing:\n- target_pattern (context/trigger)\n- action_to_avoid (what to avoid)\n- causal_explanation (why to avoid)\n- prescribed_remedy (what to do instead)",
                    events_text
                );

                if let Ok(wisdom_res) = self.llm.completion(db, Some(sys_wisdom), &prompt_wisdom).await {
                    #[derive(serde::Deserialize)]
                    struct RawWisdom {
                        target_pattern: String,
                        action_to_avoid: String,
                        causal_explanation: String,
                        prescribed_remedy: String,
                    }
                    if let Ok(rules) = serde_json::from_str::<Vec<RawWisdom>>(&wisdom_res) {
                        for r in rules {
                            let rule_uuid = uuid::Uuid::new_v4().to_string();
                            let rule_path = format!("wisdom/dynamic/{}_{}.md", r.target_pattern.replace([' ', '/'], "_"), &rule_uuid[..8]);
                            let rule_md = format!(
                                "---\ntarget_pattern: \"{}\"\naction_to_avoid: \"{}\"\ncausal_explanation: \"{}\"\nprescribed_remedy: \"{}\"\ntier: \"dynamic\"\nscope: \"{}\"\nsource_episodes:\n{}\ngenerator_name: \"DreamCoordinator\"\n---\n\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}",
                                r.target_pattern, r.action_to_avoid, r.causal_explanation, r.prescribed_remedy, scope,
                                cluster_ep_ids.iter().map(|id| format!("  - \"{}\"", id)).collect::<Vec<_>>().join("\n"),
                                r.target_pattern, r.action_to_avoid, r.causal_explanation, r.prescribed_remedy
                            );
                            store.write_file(&rule_path, &rule_md)?;

                            let rule_contract = WisdomRule {
                                id: None,
                                target_pattern: r.target_pattern,
                                action_to_avoid: r.action_to_avoid,
                                causal_explanation: r.causal_explanation,
                                prescribed_remedy: r.prescribed_remedy,
                                tier: "dynamic".to_string(),
                                scope: scope.clone(),
                                vault_path: Some(rule_path),
                                embedding: None,
                                source_episodes: cluster_ep_ids.clone(),
                                generator_name: "DreamCoordinator".to_string(),
                                similarity: None,
                                utility: None,
                            };
                            if let Ok(wisdom_id) = save_wisdom_rule_with_deduplication(db, store, &rule_contract).await {
                                for ep_id in &cluster_ep_ids {
                                    let _ = db.relate_nodes(ep_id, &wisdom_id).await;
                                }
                            }
                        }
                    }
                }

                for ep in cluster_eps {
                    if let Some(ref ep_id) = ep.id {
                        db.mark_episode_processed(ep_id).await?;
                    }
                }

                tracing::info!(
                    "Dreaming scope {}/{} ('{}'): cluster {} of {} complete",
                    scope_idx + 1,
                    total_scopes,
                    scope,
                    cluster_idx + 1,
                    total_clusters
                );
            }

            // --- DRIFT & SPLIT MANAGEMENT LOGIC START ---
            
            // 1. Load all insights for the current scope
            let existing_insights = load_insights(&store.vault_root);
            tracing::debug!("Scope: {}, existing_insights count: {}", scope, existing_insights.len());
            let scope_insights: Vec<InsightNote> = existing_insights
                .into_iter()
                .filter(|ins| ins.scope == scope)
                .collect();
            tracing::debug!("Scope: {}, scope_insights count: {}", scope, scope_insights.len());

            // 2. Process each insight for drift detection
            for ins in scope_insights {
                let source_ids = ins.source_episodes.clone();
                tracing::debug!("Checking insight: {}, source episodes count: {}", ins.title, source_ids.len());
                if source_ids.len() < 2 {
                    continue;
                }

                // Fetch full episode records
                if let Ok(nodes_resp) = db.get_memory_nodes(&source_ids).await {
                    let episodes = nodes_resp.episodes;
                    tracing::debug!("Fetched episodes count: {}", episodes.len());

                    // 3. Prepare embeddings with local fallback
                    let mut episode_embeddings = Vec::new();
                    let mut valid_episodes = Vec::new();
                    let local_embedder = crate::embeddings::LocalEmbedder::new().ok();

                    for mut ep in episodes {
                        if ep.embedding.is_none() {
                            if let Some(ref embedder) = local_embedder {
                                if let Ok(emb) = embedder.embed(&ep.content) {
                                    ep.embedding = Some(emb);
                                }
                            }
                        }
                        if let Some(emb) = &ep.embedding {
                            episode_embeddings.push(emb.clone());
                            valid_episodes.push(ep);
                        }
                    }

                    tracing::debug!("Valid episodes count: {}", valid_episodes.len());
                    // Need at least 2 valid episodes with embeddings
                    if valid_episodes.len() < 2 {
                        continue;
                    }

                    // 4. Compute max pairwise cosine distance
                    let mut max_dist = 0.0;
                    let mut max_pair = (0, 1);
                    for i in 0..episode_embeddings.len() {
                        for j in (i + 1)..episode_embeddings.len() {
                            let dist = cosine_distance(&episode_embeddings[i], &episode_embeddings[j]);
                            if dist > max_dist {
                                max_dist = dist;
                                max_pair = (i, j);
                            }
                        }
                    }

                    tracing::debug!("Max pairwise distance: {}", max_dist);
                    // 5. If drift is high (> 0.30), trigger split
                    if max_dist > 0.30 {
                        tracing::debug!("Drift > 0.30 detected! Triggering split for: {}", ins.title);
                        // Prepare references for DBSCAN
                        let emb_refs: Vec<&[f32]> = episode_embeddings.iter().map(|e| e.as_slice()).collect();
                        let labels = dbscan(&emb_refs, 0.08, 2);

                        // Group episodes by DBSCAN labels
                        let mut clusters: std::collections::HashMap<usize, Vec<Episode>> = std::collections::HashMap::new();
                        let mut outliers = Vec::new();

                        for (idx, label) in labels.into_iter().enumerate() {
                            let ep = valid_episodes[idx].clone();
                            if let Some(cid) = label {
                                clusters.entry(cid).or_default().push(ep);
                            } else {
                                outliers.push(ep);
                            }
                        }

                        let mut groups: Vec<Vec<Episode>> = Vec::new();

                        // 6. Handle DBSCAN results
                        if clusters.len() <= 1 {
                            // Manual Bisection Split
                            let seed1_emb = &episode_embeddings[max_pair.0];
                            let seed2_emb = &episode_embeddings[max_pair.1];
                            
                            let mut group1 = Vec::new();
                            let mut group2 = Vec::new();

                            for (k, ep) in valid_episodes.iter().enumerate() {
                                let dist1 = cosine_distance(&episode_embeddings[k], seed1_emb);
                                let dist2 = cosine_distance(&episode_embeddings[k], seed2_emb);
                                
                                // Assign to closer seed. Ensure seeds themselves are in their respective groups.
                                if k == max_pair.0 {
                                    group1.push(ep.clone());
                                } else if k == max_pair.1 {
                                    group2.push(ep.clone());
                                } else if dist1 < dist2 {
                                    group1.push(ep.clone());
                                } else {
                                    group2.push(ep.clone());
                                }
                            }

                            if !group1.is_empty() {
                                groups.push(group1);
                            }
                            if !group2.is_empty() {
                                groups.push(group2);
                            }
                        } else {
                            // Multiple clusters found by DBSCAN
                            groups = clusters.into_values().collect();
                        }

                        // 7. Process each resulting group
                        for group in groups {
                            if group.is_empty() {
                                continue;
                            }

                            // Format events for LLM
                            let mut events_text = String::new();
                            for ep in &group {
                                let content_len = ep.content.len();
                                let display_content = if content_len > 8000 {
                                    format!("{}... [Truncated {} characters of content due to size]", &ep.content[..8000], content_len - 8000)
                                } else {
                                    ep.content.clone()
                                };
                                events_text.push_str(&format!("Event: {}\nContent:\n{}\n\n", ep.title, display_content));
                            }

                            // Call LLM Synthesizer
                            let sys_prompt = "You are a systems synthesizer. Analyze the cluster of events and output a JSON object containing a 'title' field and a 'summary' field summarizing the architectural decisions, patterns, or habits observed.";
                            let prompt_text = format!(
                                "Please analyze these events:\n\n{}Respond ONLY with JSON matching: {{ \"title\": \"...\", \"summary\": \"...\" }}",
                                events_text
                            );
                            
                            if let Ok(llm_res) = self.llm.completion(db, Some(sys_prompt), &prompt_text).await {
                                #[derive(serde::Deserialize)]
                                struct ClusterAnalysis {
                                    title: String,
                                    summary: String,
                                }

                                let analysis: ClusterAnalysis = match serde_json::from_str(&llm_res) {
                                    Ok(a) => a,
                                    Err(_) => {
                                        ClusterAnalysis {
                                            title: format!("Split Analysis {}", &uuid::Uuid::new_v4().to_string()[..8]),
                                            summary: llm_res,
                                        }
                                    }
                                };

                                // Write new insight to disk
                                let clean_title = analysis.title.replace([' ', '/'], "_");
                                let insight_uuid = uuid::Uuid::new_v4().to_string();
                                let relative_path = format!("wiki/{}/insights/{}_{}.md", scope, clean_title, &insight_uuid[..8]);
                                let insight_content = format!(
                                    "---\ntitle: \"{}\"\nscope: \"{}\"\nsource_episodes:\n{}\n---\n\n{}",
                                    analysis.title,
                                    scope,
                                    group.iter().map(|ep| format!("  - \"{}\"", ep.id.as_ref().unwrap_or(&String::new()))).collect::<Vec<_>>().join("\n"),
                                    analysis.summary
                                );
                                if store.write_file(&relative_path, &insight_content).is_ok() {
                                    // Save WikiNode to SurrealDB
                                    let node_contract = WikiNode {
                                        id: None,
                                        name: analysis.title.clone(),
                                        content: analysis.summary.clone(),
                                        scope: scope.to_string(),
                                        vault_path: Some(relative_path.clone()),
                                        embedding: None,
                                    };
                                    
                                    if let Ok(wiki_node_id) = db.save_wiki_node(&node_contract).await {
                                        for ep in &group {
                                            if let Some(ref ep_id) = ep.id {
                                                let _ = db.relate_nodes(ep_id, &wiki_node_id).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // 8. Delete old drifting insight
                        let _ = std::fs::remove_file(Path::new(&ins.vault_path));
                        
                        let rel_path = Path::new(&ins.vault_path)
                            .strip_prefix(&store.vault_root)
                            .unwrap_or(Path::new(&ins.vault_path))
                            .to_string_lossy()
                            .to_string();
                        println!("DEBUG: rel_path: '{}', vault_path: '{}', vault_root: '{}'", rel_path, ins.vault_path, store.vault_root.display());
                        let _ = db.delete_by_vault_path(&rel_path).await;
                    }
                }
            }
            // --- DRIFT & SPLIT MANAGEMENT LOGIC END ---
        }
        Ok(())
    }
}

pub fn safe_delete_file(vault_root: &std::path::Path, relative_path: &str) -> std::io::Result<()> {
    let src = vault_root.join(relative_path);
    if src.exists() {
        let trash_dir = vault_root.join(".trash");
        if !trash_dir.exists() {
            std::fs::create_dir_all(&trash_dir)?;
        }
        if let Some(filename) = src.file_name() {
            let dest = trash_dir.join(filename);
            std::fs::rename(src, dest)?;
        }
    }
    Ok(())
}

pub async fn save_wisdom_rule_with_deduplication(
    db: &dyn StorageBackend,
    store: &MarkdownStore,
    rule: &WisdomRule,
) -> Result<String> {
    let text_to_embed = format!(
        "Pattern: {}\nAvoid: {}\nWhy: {}\nRemedy: {}",
        rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
    );
    
    let new_emb = match db.embed(&text_to_embed).await {
        Ok(emb) => emb,
        Err(e) => {
            tracing::warn!("Failed to generate embedding for deduplication: {}", e);
            return db.save_wisdom_rule(rule).await;
        }
    };

    let all_rules = match db.get_all_wisdom_rules().await {
        Ok(rules) => rules,
        Err(e) => {
            tracing::warn!("Failed to get existing rules for deduplication: {}", e);
            return db.save_wisdom_rule(rule).await;
        }
    };

    let mut best_match: Option<(WisdomRule, f32)> = None;

    for mut existing in all_rules {
        let existing_emb = match existing.embedding.as_ref() {
            Some(emb) => emb.clone(),
            None => {
                let ext_text = format!(
                    "Pattern: {}\nAvoid: {}\nWhy: {}\nRemedy: {}",
                    existing.target_pattern, existing.action_to_avoid, existing.causal_explanation, existing.prescribed_remedy
                );
                match db.embed(&ext_text).await {
                    Ok(emb) => {
                        existing.embedding = Some(emb.clone());
                        emb
                    }
                    Err(_) => continue,
                }
            }
        };

        let sim = dot_product(&new_emb, &existing_emb);
        if sim > 0.80 {
            if let Some((_, best_sim)) = best_match.as_ref() {
                if sim > *best_sim {
                    best_match = Some((existing, sim));
                }
            } else {
                best_match = Some((existing, sim));
            }
        }
    }

    if let Some((matched, _sim)) = best_match {
        if matched.tier == "skills" {
            if let Some(ref vp) = rule.vault_path {
                let _ = safe_delete_file(&store.vault_root, vp);
            }
            if let Some(ref skills_id) = matched.id {
                for ep in &rule.source_episodes {
                    let _ = db.relate_nodes(ep, skills_id).await;
                }
                return Ok(skills_id.clone());
            }
        } else if matched.tier == "dynamic" || matched.tier == "forge" {
            let system_prompt = "You are an expert software engineer and systems architect. Merge and generalize two similar wisdom rules into a single, high-quality, comprehensive wisdom rule.";
            let prompt = format!(
                "Rule 1:\nPattern: {}\nAvoid: {}\nWhy: {}\nRemedy: {}\n\n\
                 Rule 2:\nPattern: {}\nAvoid: {}\nWhy: {}\nRemedy: {}\n\n\
                 Please merge and generalize these two similar rules into a single comprehensive rule. \
                 Respond ONLY with a JSON object matching the structure of WisdomRule, with fields:\n\
                 - target_pattern\n- action_to_avoid\n- causal_explanation\n- prescribed_remedy",
                matched.target_pattern, matched.action_to_avoid, matched.causal_explanation, matched.prescribed_remedy,
                rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
            );

            let llm = crate::llm::LLMClient::new();
            match llm.completion(db, Some(system_prompt), &prompt).await {
                Ok(res) => {
                    let trimmed = res.trim();
                    let stripped = if trimmed.starts_with("```json") {
                        trimmed.strip_prefix("```json").unwrap_or(trimmed).strip_suffix("```").unwrap_or(trimmed).trim()
                    } else if trimmed.starts_with("```") {
                        trimmed.strip_prefix("```").unwrap_or(trimmed).strip_suffix("```").unwrap_or(trimmed).trim()
                    } else {
                        trimmed
                    };

                    #[derive(serde::Deserialize)]
                    struct MergedFields {
                        target_pattern: String,
                        action_to_avoid: String,
                        causal_explanation: String,
                        prescribed_remedy: String,
                    }

                    let parsed_fields = if stripped.starts_with('[') {
                        if let Ok(list) = serde_json::from_str::<Vec<MergedFields>>(stripped) {
                            list.into_iter().next()
                        } else {
                            None
                        }
                    } else {
                        serde_json::from_str::<MergedFields>(stripped).ok()
                    };

                    if let Some(fields) = parsed_fields {
                        let mut merged_eps = matched.source_episodes.clone();
                        for ep in &rule.source_episodes {
                            if !merged_eps.contains(ep) {
                                merged_eps.push(ep.clone());
                            }
                        }

                        let final_path = if let Some(ref path) = rule.vault_path {
                            path.clone()
                        } else if let Some(ref path) = matched.vault_path {
                            path.clone()
                        } else {
                            format!("wisdom/dynamic/merged_{}.md", &uuid::Uuid::new_v4().to_string()[..8])
                        };

                        let rule_md = format!(
                            "---\ntarget_pattern: \"{}\"\naction_to_avoid: \"{}\"\ncausal_explanation: \"{}\"\nprescribed_remedy: \"{}\"\ntier: \"dynamic\"\nscope: \"{}\"\nsource_episodes:\n{}\ngenerator_name: \"DreamCoordinator\"\n---\n\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}",
                            fields.target_pattern, fields.action_to_avoid, fields.causal_explanation, fields.prescribed_remedy, rule.scope,
                            merged_eps.iter().map(|id| format!("  - \"{}\"", id)).collect::<Vec<_>>().join("\n"),
                            fields.target_pattern, fields.action_to_avoid, fields.causal_explanation, fields.prescribed_remedy
                        );

                        if let Err(e) = store.write_file(&final_path, &rule_md) {
                            tracing::error!("Failed to write merged rule file: {}", e);
                        }

                        if let Some(ref old_vp) = matched.vault_path {
                            let _ = safe_delete_file(&store.vault_root, old_vp);
                        }

                        if let Some(ref old_vp) = matched.vault_path {
                            let _ = db.delete_by_vault_path(old_vp).await;
                        }

                        let merged_contract = WisdomRule {
                            id: None,
                            target_pattern: fields.target_pattern,
                            action_to_avoid: fields.action_to_avoid,
                            causal_explanation: fields.causal_explanation,
                            prescribed_remedy: fields.prescribed_remedy,
                            tier: "dynamic".to_string(),
                            scope: rule.scope.clone(),
                            vault_path: Some(final_path),
                            embedding: None,
                            source_episodes: merged_eps,
                            generator_name: "DreamCoordinator".to_string(),
                            similarity: None,
                            utility: None,
                        };

                        match db.save_wisdom_rule(&merged_contract).await {
                            Ok(saved_id) => return Ok(saved_id),
                            Err(e) => {
                                tracing::warn!("Failed to save merged rule to SurrealDB: {}", e);
                            }
                        }
                    } else {
                        tracing::warn!("Failed to parse LLM response as merged fields: {}", stripped);
                    }
                }
                Err(e) => {
                    tracing::warn!("LLM completion failed for rule merge: {}", e);
                }
            }
        }
    }

    db.save_wisdom_rule(rule).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dbscan_cosine_metrics() {
        let u = vec![1.0, 0.0];
        let v = vec![0.995, 0.1];
        let w = vec![0.0, 1.0];

        let embeddings = vec![u.as_slice(), v.as_slice(), w.as_slice()];
        
        let labels = dbscan(&embeddings, 0.05, 2);

        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0], labels[1]);
        assert!(labels[0].is_some());
        assert_eq!(labels[2], None);
    }
}
