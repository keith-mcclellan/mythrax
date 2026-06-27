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

pub fn find_elbow_point(k_distances: &[f32]) -> f32 {
    if k_distances.is_empty() {
        return 0.55;
    }
    if k_distances.len() < 3 {
        return k_distances[0];
    }
    let n = k_distances.len();
    let y0 = k_distances[0];
    let yn = k_distances[n - 1];
    let x0 = 0.0f32;
    let xn = (n - 1) as f32;

    let mut max_dist = -1.0;
    let mut elbow_idx = 0;

    for i in 0..n {
        let x = i as f32;
        let y = k_distances[i];
        let num = ((yn - y0) * x - (xn - x0) * y + xn * y0).abs();
        if num > max_dist {
            max_dist = num;
            elbow_idx = i;
        }
    }
    k_distances[elbow_idx]
}

pub fn calibrate_epsilon_fallback(model_name: &str, user_override: Option<f32>) -> f32 {
    if let Some(val) = user_override {
        return val;
    }
    if model_name.contains("nomic") {
        0.55
    } else {
        0.55
    }
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
        embedder: Option<std::sync::Arc<crate::embeddings::LocalEmbedder>>,
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

        // Background Transcript Sweep & Idle Session Recovery
        if let Ok(transcripts) = db.get_all_registered_transcripts().await {
            let idle_threshold = chrono::Duration::minutes(10);
            let now = chrono::Utc::now();
            for (session_id, path) in transcripts {
                // Check last activity timestamp for this session
                if let Ok(Some(last_activity)) = db.get_session_last_activity(&session_id).await {
                    if now - last_activity > idle_threshold {
                        // Retrieve the _last_swept_at key for this session
                        let stm_map = db.get_stm(&session_id, Some("_last_swept_at")).await.unwrap_or_default();
                        let last_swept_at_str = stm_map.get("_last_swept_at");
                        
                        let needs_mine = match last_swept_at_str {
                            Some(swept_str) => {
                                if let Ok(swept_time) = chrono::DateTime::parse_from_rfc3339(swept_str) {
                                    let swept_utc = swept_time.with_timezone(&chrono::Utc);
                                    // Check modification time of file
                                    if let Ok(metadata) = std::fs::metadata(&path) {
                                        if let Ok(modified_time) = metadata.modified() {
                                            let modified_utc: chrono::DateTime<chrono::Utc> = modified_time.into();
                                            modified_utc > swept_utc
                                        } else {
                                            true
                                        }
                                    } else {
                                        // Path invalid or file missing. Clear from STM to prevent loop
                                        let _ = db.clear_stm(&session_id).await;
                                        false
                                    }
                                } else {
                                    true
                                }
                            }
                            None => true,
                        };

                        if needs_mine {
                            // Check file exists before mining
                            if std::path::Path::new(&path).exists() {
                                let ignore_list = crate::vault::watcher::WatchIgnoreList::default();
                                if let Ok(_) = crate::hooks::precompact::mine_transcript(&session_id, &path, db, store, &ignore_list).await {
                                    let _ = db.save_stm(&session_id, "_last_swept_at", &now.to_rfc3339()).await;
                                }
                            } else {
                                // Clear STM path registry if file is missing/deleted
                                let _ = db.clear_stm(&session_id).await;
                            }
                        }
                    }
                }
            }
        }

        let all_episodes = db.get_all_episodes().await?;
        let unprocessed = db.get_unprocessed_episodes().await?;

        if unprocessed.is_empty() {
            return Ok(());
        }

        let (_default_eps, default_min_samples) = match active_mode.as_str() {
            "deep" => (0.15, 2),
            "bulk" => (0.12, 4),
            _ => (0.08, 2), // incremental
        };

        let min_samples = file_min_samples.unwrap_or(default_min_samples);
        let final_eps = if let Some(f_eps) = file_eps {
            f_eps
        } else {
            // Dynamic epsilon calibration using k-distance elbow method
            let all_nodes = db.get_all_wiki_nodes().await.unwrap_or_default();
            let mut embeddings = Vec::new();
            for ep in &all_episodes {
                if let Some(ref emb) = ep.embedding {
                    embeddings.push(emb.clone());
                }
            }
            for node in &all_nodes {
                if let Some(ref emb) = node.embedding {
                    embeddings.push(emb.clone());
                }
            }

            if embeddings.len() >= 100 {
                let sample = &embeddings[0..100];
                let mut k_distances = Vec::new();
                for i in 0..sample.len() {
                    let mut dists = Vec::new();
                    for j in 0..sample.len() {
                        let d = cosine_distance(&sample[i], &sample[j]);
                        dists.push(d);
                    }
                    dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    if dists.len() > 4 {
                        k_distances.push(dists[4]);
                    }
                }
                k_distances.sort_by(|a, b| a.partial_cmp(b).unwrap());
                find_elbow_point(&k_distances)
            } else {
                let user_override_val = match db.get_profile_key("embeddings.default_epsilon").await {
                    Ok(Some(val_str)) => val_str.parse::<f32>().ok(),
                    _ => None,
                };
                let model_name = match db.get_llm_config().await {
                    Ok(cfg) => cfg.model,
                    _ => "nomic-embed-text-v1.5-mlx".to_string(),
                };
                calibrate_epsilon_fallback(&model_name, user_override_val)
            }
        };

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
                        let mut display_content = ep.content.clone();
                        if let Some(ref ep_id) = ep.id {
                            if let Ok(related_ids) = db.get_related_node_ids(ep_id).await {
                                if !related_ids.is_empty() {
                                    if let Ok(mem_nodes_resp) = db.get_memory_nodes(&related_ids).await {
                                        let mut artifacts_text = String::new();
                                        for node in mem_nodes_resp.wiki_nodes {
                                            artifacts_text.push_str(&format!(
                                                "\nArtifact Name: {}\nContent:\n{}\n",
                                                node.name, node.content
                                            ));
                                        }
                                        if !artifacts_text.is_empty() {
                                            display_content.push_str("\n\nAssociated Artifacts:\n");
                                            display_content.push_str(&artifacts_text);
                                        }
                                    }
                                }
                            }
                        }
                        
                        let content_len = display_content.len();
                        let display_content = if content_len > 100_000 {
                            let truncated = truncate_to_boundary(&display_content, 100_000);
                            format!("{}... [Truncated {} characters of content due to size]", truncated, content_len - 100_000)
                        } else {
                            display_content
                        };
                        let prompt_text = format!(
                            "Existing Insight Body:\n{}\n\nNew Event content:\nTitle: {}\n{}",
                            ins.content, ep.title, display_content
                        );
                        let updated_summary = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

                        let mut source_ep_links = Vec::new();
                        let mut eps_to_link = Vec::new();
                        if let Ok(mem_nodes) = db.get_memory_nodes(&new_source_episodes).await {
                            for ep in mem_nodes.episodes {
                                if let Some(ref path) = ep.vault_path {
                                    let target = path.strip_suffix(".md").unwrap_or(path);
                                    source_ep_links.push(format!("- [[{}|{}]]", target, ep.title));
                                    eps_to_link.push((path.clone(), ep.title.clone()));
                                }
                            }
                        }
                        let source_ep_section = if !source_ep_links.is_empty() {
                            format!("\n\n## Source Episodes\n{}", source_ep_links.join("\n"))
                        } else {
                            String::new()
                        };

                        let relative_path = format!("wiki/{}/insights/{}.md", scope, ins.title.replace(' ', "_"));
                        let new_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource_episodes:\n{}\n---\n\n{}{}",
                            ins.title,
                            scope,
                            new_source_episodes.iter().map(|id| format!("  - \"{}\"", id)).collect::<Vec<_>>().join("\n"),
                            updated_summary,
                            source_ep_section
                        );
                        store.write_file(&relative_path, &new_content)?;

                        for (ep_path, _ep_title) in eps_to_link {
                            let _ = store.append_link_to_file(&ep_path, "Insights & Summaries", &relative_path, &ins.title);
                        }

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
                                 let _ = db.relate_nodes(ep_id, &wiki_node_id, None, None, None).await;
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

            let labels = dbscan(&candidate_embs, final_eps, min_samples);

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
                    let mut ep_display_content = ep.content.clone();
                    if let Some(ref ep_id) = ep.id {
                        if let Ok(related_ids) = db.get_related_node_ids(ep_id).await {
                            if !related_ids.is_empty() {
                                if let Ok(mem_nodes_resp) = db.get_memory_nodes(&related_ids).await {
                                    let mut artifacts_text = String::new();
                                    for node in mem_nodes_resp.wiki_nodes {
                                        artifacts_text.push_str(&format!(
                                            "\nArtifact Name: {}\nContent:\n{}\n",
                                            node.name, node.content
                                        ));
                                    }
                                    if !artifacts_text.is_empty() {
                                        ep_display_content.push_str("\n\nAssociated Artifacts:\n");
                                        ep_display_content.push_str(&artifacts_text);
                                    }
                                }
                            }
                        }
                    }
                    
                    let content_len = ep_display_content.len();
                    let ep_display_content = if content_len > 100_000 {
                        let truncated = truncate_to_boundary(&ep_display_content, 100_000);
                        format!("{}... [Truncated {} characters of content due to size]", truncated, content_len - 100_000)
                    } else {
                        ep_display_content
                    };
                    events_text.push_str(&format!("Event: {}\nContent:\n{}\n\n", ep.title, ep_display_content));
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
                        let _ = db.relate_nodes(ep_id, &wiki_node_id, None, None, None).await;
                    }
                }

                let sys_wisdom = "You are a systems synthesizer. Analyze the events and extract system-level Wisdom Rules to avoid mistakes. Respond with a JSON array of rules.";
                let prompt_wisdom = format!(
                    "Events:\n\n{}Respond ONLY with a JSON array of objects, each containing exactly:\n\
                    - target_pattern (string: context/trigger)\n\
                    - action_to_avoid (string: what to avoid)\n\
                    - causal_explanation (string: why to avoid)\n\
                    - prescribed_remedy (string: what to do instead)\n\
                    - rule_type (string: must be either \"aesthetic\" or \"procedural\". \"aesthetic\" rules govern styling, CSS, visual layouts, colors, or UI tokens. \"procedural\" rules govern workflows, TDD, testing, git, database logic, or compilers.)",
                    events_text
                );

                if let Ok(wisdom_res) = self.llm.completion(db, Some(sys_wisdom), &prompt_wisdom).await {
                    #[derive(serde::Deserialize)]
                    struct RawWisdom {
                        target_pattern: String,
                        action_to_avoid: String,
                        causal_explanation: String,
                        prescribed_remedy: String,
                        #[serde(default)]
                        rule_type: Option<String>,
                    }
                    if let Ok(rules) = serde_json::from_str::<Vec<RawWisdom>>(&wisdom_res) {
                        for r in rules {
                            let rule_type = r.rule_type.as_deref().unwrap_or("aesthetic").to_lowercase();
                            let is_procedural = rule_type == "procedural";

                            let mut source_ep_links = Vec::new();
                            let mut eps_to_link = Vec::new();
                            if let Ok(mem_nodes) = db.get_memory_nodes(&cluster_ep_ids).await {
                                for ep in mem_nodes.episodes {
                                    if let Some(ref path) = ep.vault_path {
                                        let target = path.strip_suffix(".md").unwrap_or(path);
                                        source_ep_links.push(format!("- [[{}|{}]]", target, ep.title));
                                        eps_to_link.push((path.clone(), ep.title.clone()));
                                    }
                                }
                            }
                            let source_ep_section = if !source_ep_links.is_empty() {
                                format!("\n\n## Source Episodes\n{}", source_ep_links.join("\n"))
                            } else {
                                String::new()
                            };

                            let rule_uuid = uuid::Uuid::new_v4().to_string();
                            let (rule_path, final_tier, final_scope) = if is_procedural {
                                // Save to global/wisdom/permanent/
                                let relative_global_path = format!("global/wisdom/permanent/{}_{}.md", r.target_pattern.replace([' ', '/'], "_"), &rule_uuid[..8]);
                                (relative_global_path, "permanent".to_string(), "general".to_string())
                            } else {
                                // Save to wisdom/dynamic/ (local)
                                let relative_local_path = format!("wisdom/dynamic/{}_{}.md", r.target_pattern.replace([' ', '/'], "_"), &rule_uuid[..8]);
                                (relative_local_path, "dynamic".to_string(), scope.clone())
                            };

                            let rule_md = format!(
                                "---\ntarget_pattern: \"{}\"\naction_to_avoid: \"{}\"\ncausal_explanation: \"{}\"\nprescribed_remedy: \"{}\"\ntier: \"{}\"\nscope: \"{}\"\nsource_episodes:\n{}\ngenerator_name: \"DreamCoordinator\"\n---\n\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}{}",
                                r.target_pattern, r.action_to_avoid, r.causal_explanation, r.prescribed_remedy, final_tier, final_scope,
                                cluster_ep_ids.iter().map(|id| format!("  - \"{}\"", id)).collect::<Vec<_>>().join("\n"),
                                r.target_pattern, r.action_to_avoid, r.causal_explanation, r.prescribed_remedy,
                                source_ep_section
                            );
                            store.write_file(&rule_path, &rule_md)?;

                            for (ep_path, _ep_title) in eps_to_link {
                                let _ = store.append_link_to_file(&ep_path, "Derived Wisdom Rules", &rule_path, &format!("Wisdom: {}", r.target_pattern));
                            }

                            let rule_contract = WisdomRule {
                                id: None,
                                target_pattern: r.target_pattern,
                                action_to_avoid: r.action_to_avoid,
                                causal_explanation: r.causal_explanation,
                                prescribed_remedy: r.prescribed_remedy,
                                tier: final_tier,
                                scope: final_scope,
                                vault_path: Some(rule_path),
                                embedding: None,
                                source_episodes: cluster_ep_ids.clone(),
                                generator_name: "DreamCoordinator".to_string(),
                                similarity: None,
                                utility: None,
                                status: None,
                                superseded_at: None,
                                superseded_by: None,
                            };
                            if let Ok(wisdom_id) = save_wisdom_rule_with_deduplication(db, store, &rule_contract).await {
                                for ep_id in &cluster_ep_ids {
                                    let _ = db.relate_nodes(ep_id, &wisdom_id, None, None, None).await;
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
                    
                    // Separate episodes that need embedding
                    let mut episodes_needing_embedding = Vec::new();
                    let mut episodes_with_embedding = Vec::new();
                    
                    for mut ep in episodes {
                        if ep.embedding.is_none() {
                            episodes_needing_embedding.push(ep.content.clone());
                            ep.embedding = None;
                            episodes_with_embedding.push(ep);
                        } else {
                            if let Some(emb) = &ep.embedding {
                                episode_embeddings.push(emb.clone());
                                valid_episodes.push(ep);
                            }
                        }
                    }

                    // Batch embed missing episodes if embedder is provided
                    if let Some(ref embedder) = embedder {
                        if !episodes_needing_embedding.is_empty() {
                            if let Ok(embeds) = embedder.embed_batch(&episodes_needing_embedding) {
                                for (i, ep) in episodes_with_embedding.iter_mut().enumerate() {
                                    if i < embeds.len() {
                                        ep.embedding = Some(embeds[i].clone());
                                    }
                                }
                            }
                        }
                    }
                    
                    // Merge back into final lists
                    for ep in episodes_with_embedding {
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
                                let mut ep_display_content = ep.content.clone();
                                if let Some(ref ep_id) = ep.id {
                                    if let Ok(related_ids) = db.get_related_node_ids(ep_id).await {
                                        if !related_ids.is_empty() {
                                            if let Ok(mem_nodes_resp) = db.get_memory_nodes(&related_ids).await {
                                                let mut artifacts_text = String::new();
                                                for node in mem_nodes_resp.wiki_nodes {
                                                    artifacts_text.push_str(&format!(
                                                        "\nArtifact Name: {}\nContent:\n{}\n",
                                                        node.name, node.content
                                                    ));
                                                }
                                                if !artifacts_text.is_empty() {
                                                    ep_display_content.push_str("\n\nAssociated Artifacts:\n");
                                                    ep_display_content.push_str(&artifacts_text);
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                let content_len = ep_display_content.len();
                                let ep_display_content = if content_len > 100_000 {
                                    let truncated = truncate_to_boundary(&ep_display_content, 100_000);
                                    format!("{}... [Truncated {} characters of content due to size]", truncated, content_len - 100_000)
                                } else {
                                    ep_display_content
                                };
                                events_text.push_str(&format!("Event: {}\nContent:\n{}\n\n", ep.title, ep_display_content));
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

                                let mut source_ep_links = Vec::new();
                                for ep in &group {
                                    if let Some(ref path) = ep.vault_path {
                                        let target = path.strip_suffix(".md").unwrap_or(path);
                                        source_ep_links.push(format!("- [[{}|{}]]", target, ep.title));
                                    }
                                }
                                let source_ep_section = if !source_ep_links.is_empty() {
                                    format!("\n\n## Source Episodes\n{}", source_ep_links.join("\n"))
                                } else {
                                    String::new()
                                };

                                let insight_content = format!(
                                    "---\ntitle: \"{}\"\nscope: \"{}\"\nsource_episodes:\n{}\n---\n\n{}{}",
                                    analysis.title,
                                    scope,
                                    group.iter().map(|ep| format!("  - \"{}\"", ep.id.as_ref().unwrap_or(&String::new()))).collect::<Vec<_>>().join("\n"),
                                    analysis.summary,
                                    source_ep_section
                                );
                                if store.write_file(&relative_path, &insight_content).is_ok() {
                                    for ep in &group {
                                        if let Some(ref path) = ep.vault_path {
                                            let _ = store.append_link_to_file(path, "Insights & Summaries", &relative_path, &analysis.title);
                                        }
                                    }

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
                                                let _ = db.relate_nodes(ep_id, &wiki_node_id, None, None, None).await;
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
        if let Err(e) = db.prune_stale_memories(&store.vault_root).await {
            tracing::error!("Dreaming pruning failed: {:?}", e);
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

fn update_archived_rule_content(content: &str, new_id: &str) -> String {
    if content.starts_with("---") {
        if let Some(second_dash_idx) = content[3..].find("---") {
            let actual_second_idx = second_dash_idx + 3;
            let frontmatter = &content[3..actual_second_idx];
            let rest = &content[actual_second_idx..];
            
            let mut new_frontmatter = String::new();
            let mut status_written = false;
            let mut superseded_by_written = false;
            for line in frontmatter.lines() {
                if line.trim().starts_with("status:") {
                    new_frontmatter.push_str("status: \"superseded\"\n");
                    status_written = true;
                } else if line.trim().starts_with("superseded_by:") {
                    new_frontmatter.push_str(&format!("superseded_by: \"{}\"\n", new_id));
                    superseded_by_written = true;
                } else {
                    new_frontmatter.push_str(line);
                    new_frontmatter.push_str("\n");
                }
            }
            if !status_written {
                new_frontmatter.push_str("status: \"superseded\"\n");
            }
            if !superseded_by_written {
                new_frontmatter.push_str(&format!("superseded_by: \"{}\"\n", new_id));
            }
            
            return format!("---\n{}---{}", new_frontmatter, rest);
        }
    }
    format!("---\nstatus: \"superseded\"\nsuperseded_by: \"{}\"\n---\n\n{}", new_id, content)
}

pub async fn save_wisdom_rule_with_deduplication(
    db: &dyn StorageBackend,
    store: &MarkdownStore,
    rule: &WisdomRule,
) -> Result<String> {
    let new_emb = if let Some(ref emb) = rule.embedding {
        emb.clone()
    } else {
        let text_to_embed = format!(
            "Pattern: {}\nAvoid: {}\nWhy: {}\nRemedy: {}",
            rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
        );
        match db.embed(&text_to_embed).await {
            Ok(emb) => emb,
            Err(e) => {
                tracing::warn!("Failed to generate embedding for deduplication: {}", e);
                return db.save_wisdom_rule(rule).await;
            }
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
                    let _ = db.relate_nodes(ep, skills_id, None, None, None).await;
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
                            status: None,
                            superseded_at: None,
                            superseded_by: None,
                        };

                        match db.save_wisdom_rule(&merged_contract).await {
                            Ok(saved_id) => {
                                // 1. Update old rule status to "superseded" and set superseded_at in SurrealDB
                                if let Some(surreal_backend) = db.as_any().downcast_ref::<crate::db::SurrealBackend>() {
                                    let old_uuid = matched.id.as_ref().unwrap().strip_prefix("wisdom:").unwrap_or(matched.id.as_ref().unwrap());
                                    let new_uuid = saved_id.strip_prefix("wisdom:").unwrap_or(&saved_id);
                                    
                                    let sql = "
                                        LET $old_rec = type::record('wisdom', $old_uuid);
                                        LET $new_rec = type::record('wisdom', $new_uuid);
                                        UPDATE $old_rec SET status = 'superseded', superseded_at = time::now();
                                        RELATE $old_rec -> superseded_by -> $new_rec CONTENT { reason: 'Consolidated during dreaming compaction', created_at: time::now() };
                                    ";
                                    if let Err(e) = surreal_backend.db.query(sql)
                                        .bind(("old_uuid", old_uuid))
                                        .bind(("new_uuid", new_uuid))
                                        .await {
                                        tracing::error!("Failed to update superseded status or relate nodes: {}", e);
                                    }
                                }

                                // 2. Move physical old rule file to superseded_archive
                                if let Some(ref old_vp) = matched.vault_path {
                                    let src_path = store.vault_root.join(old_vp);
                                    if src_path.exists() {
                                        let archive_dir = store.vault_root.join("wisdom/superseded_archive");
                                        let _ = std::fs::create_dir_all(&archive_dir);
                                        if let Some(filename) = src_path.file_name() {
                                            let dest_path = archive_dir.join(filename);
                                            if std::fs::rename(&src_path, &dest_path).is_ok() {
                                                if let Ok(content) = std::fs::read_to_string(&dest_path) {
                                                    let updated_content = update_archived_rule_content(&content, &saved_id);
                                                    let _ = std::fs::write(&dest_path, updated_content);
                                                }
                                            }
                                        }
                                    }
                                }

                                return Ok(saved_id);
                            }
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

/// Truncates a string to a maximum character count, respecting boundary
/// awareness (paragraphs, lines, words) to avoid cutting mid-sentence.
fn truncate_to_boundary(s: &str, max_chars: usize) -> &str {
    // Check if the string is within limits
    if s.chars().count() <= max_chars {
        return s;
    }
    
    // Find the byte index at the character limit
    let limit_byte_idx = match s.char_indices().nth(max_chars) {
        Some((idx, _)) => idx,
        None => return s,
    };
    
    let candidate = &s[..limit_byte_idx];
    
    // Scan backward to find a clean boundary
    // 1. Try paragraph boundary (\n\n) within the last 5000 characters
    if let Some(para_idx) = candidate.rfind("\n\n") {
        if limit_byte_idx - para_idx < 5000 {
            return &candidate[..para_idx];
        }
    }
    
    // 2. Try line boundary (\n) within the last 2000 characters
    if let Some(line_idx) = candidate.rfind('\n') {
        if limit_byte_idx - line_idx < 2000 {
            return &candidate[..line_idx];
        }
    }
    
    // 3. Try word boundary (space) within the last 500 characters
    if let Some(space_idx) = candidate.rfind(' ') {
        if limit_byte_idx - space_idx < 500 {
            return &candidate[..space_idx];
        }
    }
    
    // Fallback to exact character boundary truncation
    candidate
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
