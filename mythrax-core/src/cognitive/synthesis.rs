//! # Mythrax Cognitive Synthesis Pipeline
//!
//! Provides the primary cognitive architecture for episodic memory distillation,
//! hierarchical clustering via DBSCAN, Direction backpropagation, and Wisdom graduation.
//! Incorporates Strunk & White concision rules and compression warning metrics.

use crate::db::StorageBackend;
use crate::llm::LLMClient;
use crate::store::MarkdownStore;
use crate::contracts::{Episode, WisdomRule, WikiNode};
use surrealdb_types::SurrealValue;
use anyhow::Result;
use std::path::Path;
use std::collections::HashMap;

pub const CONCISION_DIRECTIVE: &str = "\nWrite clearly and concisely (Rules from Strunk & White's Elements of Style):\n- Omit needless words: make every word tell. Do not use filler or throat-clearing phrasing.\n- Use active voice, positive form, and definite, specific, concrete language.";

pub fn build_synthesis_prompt(base_sys: &str) -> String {
    format!("{}\n\n{}", CONCISION_DIRECTIVE, base_sys)
}

pub fn check_compression_ratio(input_text: &str, output_text: &str, original_content_tokens: usize) {
    let input_tokens = input_text.len() / 4;
    let output_tokens = output_text.len() / 4;
    
    let original = std::cmp::max(original_content_tokens, 1);
    let ratio = (input_tokens + output_tokens) as f64 / original as f64;
    
    let alert_ratio: f64 = std::env::var("MYTHRAX_VERBOSITY_ALERT_RATIO")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.5);
        
    if ratio > alert_ratio {
        tracing::warn!("Verbosity alert: compression ratio {:.2} exceeds limit {:.2}", ratio, alert_ratio);
    }
}

pub fn cosine_distance(u: &[f32], v: &[f32]) -> f32 {
    1.0 - crate::math::cosine_similarity(u, v)
}

pub fn slugify_title(text: &str) -> String {
    let mut slug = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() || c == '-' {
            slug.push(c);
        } else if c.is_whitespace() || c == '_' || c == '/' || c == '\\' {
            slug.push('_');
        }
    }
    while slug.contains("__") {
        slug = slug.replace("__", "_");
    }
    let trimmed = slug.trim_matches(|c| c == '_' || c == '-').to_string();
    if trimmed.len() > 100 {
        if let Some(pos) = trimmed[..100].rfind('_') {
            trimmed[..pos].to_string()
        } else {
            trimmed[..100].to_string()
        }
    } else {
        trimmed
    }
}

pub fn resolve_rule_path(scope: &str, action_to_avoid: &str) -> String {
    if scope == "general" {
        format!("global/wisdom/dynamic/{}.md", slugify_title(action_to_avoid))
    } else {
        format!("wisdom/dynamic/{}/{}.md", scope, slugify_title(action_to_avoid))
    }
}


pub fn dbscan(
    embeddings: &[&[f32]],
    eps: f32,
    min_samples: usize,
) -> Vec<Option<usize>> {
    let n = embeddings.len();
    let mut labels = vec![None; n];
    let mut cluster_id = 0;

    // Precompute norms for all embeddings to prevent O(N^2) recalculations
    // during the nested find_neighbors loops.
    let mut norms = Vec::with_capacity(n);
    for emb in embeddings {
        let mut norm = 0.0;
        for val in *emb {
            norm += val * val;
        }
        norms.push(norm.sqrt());
    }

    for i in 0..n {
        if labels[i].is_some() {
            continue;
        }

        let mut neighbors = find_neighbors(i, embeddings, &norms, eps);
        if neighbors.len() < min_samples {
            continue;
        }

        labels[i] = Some(cluster_id);
        let mut j = 0;
        while j < neighbors.len() {
            let neighbor_idx = neighbors[j];
            if labels[neighbor_idx].is_none() {
                labels[neighbor_idx] = Some(cluster_id);
                let neighbor_neighbors = find_neighbors(neighbor_idx, embeddings, &norms, eps);
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


fn find_neighbors(i: usize, embeddings: &[&[f32]], norms: &[f32], eps: f32) -> Vec<usize> {
    let mut neighbors = Vec::new();
    let target = embeddings[i];
    let norm_target = norms[i];
    for (idx, &emb) in embeddings.iter().enumerate() {
        let dist = 1.0 - crate::math::cosine_similarity_precomputed(target, norm_target, emb, norms[idx]);
        if dist <= eps {
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
                
                for sub in &["insights", "raw"] {
                    let dir = entry.path().join(sub);
                    if dir.exists()
                        && let Ok(files) = std::fs::read_dir(&dir) {
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
    }
    insights
}

fn parse_insight_note(content: &str, path: &Path, scope: &str) -> Result<InsightNote> {
    if content.starts_with("---") {
        let parts: Vec<&str> = content.split("---").collect();
        if parts.len() >= 3 {
            let yaml_str = parts[1];
            let body = parts[2..].join("---");

            #[derive(serde::Deserialize)]
            struct Frontmatter {
                title: Option<String>,
                name: Option<String>,
                source_episodes: Option<Vec<String>>,
            }
            if let Ok(fm) = serde_yaml::from_str::<Frontmatter>(yaml_str) {
                let title = fm.title.or(fm.name).unwrap_or_else(|| "Untitled Note".to_string());
                let source_episodes = fm.source_episodes.unwrap_or_default();

                return Ok(InsightNote {
                    title,
                    content: body.trim().to_string(),
                    scope: scope.to_string(),
                    source_episodes,
                    vault_path: path.to_string_lossy().to_string(),
                });
            }
        }
    }

    // Fallback for raw files or files with invalid frontmatter
    let mut title = path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled Note".to_string());
        
    // Look for the first # header for a better title
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            title = trimmed[2..].trim().to_string();
            break;
        }
    }

    let mut source_episodes = Vec::new();
    // Parse wikilinks to episodes or archive
    let mut pos = 0;
    while let Some(start_idx) = content[pos..].find("[[") {
        let actual_start = pos + start_idx + 2;
        if let Some(end_idx) = content[actual_start..].find("]]") {
            let link_content = &content[actual_start..actual_start + end_idx];
            let link_path = if let Some(pipe_idx) = link_content.find('|') {
                &link_content[..pipe_idx]
            } else {
                link_content
            };
            
            let link_path = link_path.trim();
            let clean_id = if let Some(stripped) = link_path.strip_prefix("episodes/") {
                Some(stripped)
            } else if let Some(stripped) = link_path.strip_prefix("archive/") {
                Some(stripped)
            } else {
                None
            };
            
            if let Some(id) = clean_id {
                let id_no_ext = Path::new(id).file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| id.to_string());
                if !source_episodes.contains(&id_no_ext) {
                    source_episodes.push(id_no_ext);
                }
            }
            pos = actual_start + end_idx + 2;
        } else {
            break;
        }
    }

    Ok(InsightNote {
        title,
        content: content.trim().to_string(),
        scope: scope.to_string(),
        source_episodes,
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
    scope_locks: dashmap::DashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>,
}

impl Default for DreamCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl DreamCoordinator {
    pub fn new() -> Self {
        Self {
            llm: LLMClient::default(),
            scope_locks: dashmap::DashMap::new(),
        }
    }

    pub async fn save_wiki_node_with_contradiction_resolution(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
        node: &WikiNode,
        embedder: Option<std::sync::Arc<dyn crate::embeddings::TextEmbedder>>,
    ) -> Result<String> {
        if !db.is_feature_enabled("compactor.enable_contradiction_detection", true).await {
            return db.save_wiki_node(node).await;
        }

        let mut node = node.clone();
        if node.embedding.is_none() {
            if let Some(ref emb) = embedder {
                let max_tokens = 2048;
                let truncated_content = truncate_by_tokens(&node.content, max_tokens, Some(emb.as_ref()));
                if let Ok(e) = emb.embed(&truncated_content) {
                    node.embedding = Some(e);
                }
            }
        }

        // Get all existing wiki nodes in the SAME scope
        let all_nodes = db.get_all_wiki_nodes().await?;
        let same_scope_nodes: Vec<WikiNode> = all_nodes.into_iter()
            .filter(|n| n.scope == node.scope && n.embedding.is_some())
            .collect();

        let mut candidates = Vec::new();
        if let Some(ref new_emb) = node.embedding {
            for existing in same_scope_nodes {
                if let Some(ref ext_emb) = existing.embedding {
                    let sim = {
                        let dot: f32 = new_emb.iter().zip(ext_emb.iter()).map(|(a, b)| a * b).sum();
                        let norm_u: f32 = new_emb.iter().map(|x| x * x).sum::<f32>().sqrt();
                        let norm_v: f32 = ext_emb.iter().map(|x| x * x).sum::<f32>().sqrt();
                        if norm_u == 0.0 || norm_v == 0.0 {
                            0.0
                        } else {
                            dot / (norm_u * norm_v)
                        }
                    };
                    if sim >= 0.70 {
                        candidates.push((sim, existing));
                    }
                }
            }
        }

        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let top_candidates: Vec<_> = candidates.into_iter().take(5).collect();

        for (_sim, existing_node) in top_candidates {
            let sys_prompt = "You are an expert knowledge consistency checker. Compare the NEW insight against the EXISTING insight. Determine if they contradict each other. Output ONLY valid JSON.";
            let user_prompt = format!(
                "NEW INSIGHT:\n{}\n\nEXISTING INSIGHT:\n{}\n\nRespond with a JSON object containing contradicts: bool, conflicting_field: string, resolution: string, and confidence: float.",
                node.content,
                existing_node.content
            );

            if let Ok(resp_str) = self.llm.routed_completion(db, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Reasoning), Some(sys_prompt), &user_prompt).await {
                #[derive(serde::Deserialize)]
                struct ContradictionResponse {
                    contradicts: bool,
                    resolution: Option<String>,
                    confidence: f32,
                }
                // Strip markdown code block wrappers if any
                let clean_resp = crate::llm::strip_code_fences(&resp_str);
                if let Ok(res) = serde_json::from_str::<ContradictionResponse>(&clean_resp) {
                    if res.contradicts && res.confidence >= 0.80 {
                        if let Some(resolution) = res.resolution {
                            let mut updated_node = existing_node.clone();
                            updated_node.content = resolution.clone();
                            
                            // Re-embed resolved content
                            if let Some(ref emb) = embedder {
                                let max_tokens = 2048;
                                let truncated_content = truncate_by_tokens(&updated_node.content, max_tokens, Some(emb.as_ref()));
                                if let Ok(e) = emb.embed(&truncated_content) {
                                    updated_node.embedding = Some(e);
                                }
                            }

                            // Save updated existing node to DB
                            db.save_wiki_node(&updated_node).await?;

                            // Update its physical file, preserving frontmatter
                            if let Some(ref vp) = updated_node.vault_path {
                                if let Ok(existing_file_content) = std::fs::read_to_string(store.vault_root.join(vp)) {
                                    let parts: Vec<&str> = existing_file_content.splitn(3, "---").collect();
                                    if parts.len() == 3 {
                                        let updated_file_content = format!("---{}---\n\n{}", parts[1], resolution);
                                        let _ = store.write_file(vp, &updated_file_content);
                                    } else {
                                        let _ = store.write_file(vp, &resolution);
                                    }
                                } else {
                                    let _ = store.write_file(vp, &resolution);
                                }
                            }

                            return Ok(updated_node.id.unwrap_or_default());
                        }
                    }
                }
            }
        }

        db.save_wiki_node(&node).await
    }

    pub async fn run_dream(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
        mode_override: Option<&str>,
        embedder: Option<std::sync::Arc<dyn crate::embeddings::TextEmbedder>>,
    ) -> Result<()> {
        if crate::vault::ingestion::IS_INGESTING.load(std::sync::atomic::Ordering::SeqCst) {
            tracing::info!("Ingestion in progress, skipping background dream synthesis.");
            return Ok(());
        }

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
        let mut mined_any = false;
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
                                if let Ok(count) = crate::hooks::precompact::mine_transcript(&session_id, &path, db, store, &ignore_list).await {
                                    if count > 0 {
                                        mined_any = true;
                                    }
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

        if mined_any {
            let cooldown_secs = std::env::var("MYTHRAX_PHASE_COOLDOWN_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(300);
            if cooldown_secs > 0 {
                tracing::info!("Phase A (Ingestion) finished. Cooldown sleep for {} seconds before Phase B (Synthesis).", cooldown_secs);
                tokio::time::sleep(tokio::time::Duration::from_secs(cooldown_secs)).await;
            }
        }

        let mut attempted_ids = std::collections::HashSet::new();
        loop {
            let unprocessed = db.get_unprocessed_episodes().await?;
            let filtered_unprocessed: Vec<Episode> = unprocessed.into_iter()
                .filter(|ep| ep.id.as_ref().map_or(true, |id| !attempted_ids.contains(id)))
                .collect();

            if filtered_unprocessed.is_empty() {
                break;
            }

            let chunk_unprocessed: Vec<Episode> = filtered_unprocessed.into_iter().take(500).collect();
            for ep in &chunk_unprocessed {
                if let Some(ref id) = ep.id {
                    attempted_ids.insert(id.clone());
                }
            }

            let all_episodes = db.get_all_episodes().await?;

            let (_default_eps, default_min_samples) = match active_mode.as_str() {
                "deep" => (0.15, 2),
                "bulk" => (0.12, 4),
                _ => (0.25, 2), // default/incremental
            };

            // Query database profile for DBSCAN settings
            let db_min_samples = match db.get_profile_key("embeddings.dbscan_min_samples").await {
                Ok(Some(val_str)) => val_str.parse::<usize>().ok(),
                _ => None,
            };
            let db_eps = match db.get_profile_key("embeddings.dbscan_epsilon").await {
                Ok(Some(val_str)) => val_str.parse::<f32>().ok(),
                _ => None,
            };

            let min_samples = db_min_samples
                .or(file_min_samples)
                .unwrap_or(default_min_samples);

            let final_eps = if let Some(eps) = db_eps.or(file_eps) {
                eps
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
                    user_override_val.unwrap_or(0.12)
                }
            };

            let mut scope_groups: HashMap<String, Vec<Episode>> = HashMap::new();
            for ep in chunk_unprocessed {
                let scope = ep.scope.clone().unwrap_or_else(|| "general".to_string());
                scope_groups.entry(scope).or_default().push(ep);
            }

            let total_scopes = scope_groups.len();
            for (scope_idx, (scope, new_episodes)) in scope_groups.into_iter().enumerate() {
            let mut insights_changed = 0;
            let scope_lock = self.scope_locks.entry(scope.clone()).or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(()))).clone();
            let _guard = scope_lock.lock().await;
            
            for chunk in new_episodes.chunks(500) {
                let new_episodes = chunk.to_vec();
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
                            if dist < 0.10 {
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

                        let base_sys = "You are a systems synthesizer. Refine the existing architectural insight note by incorporating the details of the new event.";
                        let sys_prompt = crate::cognitive::synthesis::build_synthesis_prompt(base_sys);
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
                        let original_tokens = (ins.content.len() + display_content.len()) / 4;
                        let updated_summary = self.llm.routed_completion(db, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Summarization), Some(&sys_prompt), &prompt_text).await?;
                        crate::cognitive::synthesis::check_compression_ratio(&prompt_text, &updated_summary, original_tokens);

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

                        let relative_path = format!("wiki/{}/insights/{}.md", scope, slugify_title(&ins.title));
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
                            ..Default::default()
                        };
                        if let Ok(wiki_node_id) = self.save_wiki_node_with_contradiction_resolution(db, store, &node_contract, embedder.clone()).await {
                            if let Some(ref ep_id) = ep.id {
                                let _ = db.relate_nodes(ep_id, &wiki_node_id, None, None, None).await;
                            }
                        }
                        
                        insights_changed += 1;
                        if insights_changed > 5 {
                            tracing::info!("Scope '{}' exceeded 5 insight changes. Triggering interleaved compaction.", scope);
                            let compactor = crate::cognitive::compactor::Compactor::new();
                            let _ = compactor.compact_scope(db, store, &scope, embedder.clone()).await;
                            insights_changed = 0;
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

                let base_sys = "You are a systems synthesizer. Analyze the cluster of events and output a JSON object containing the fields: 'title', 'summary', 'metacognitive_confidence', and 'node_type'.\n\n\
                For 'metacognitive_confidence', use the following strict integer rubric (1-5):\n\
                - 1: Anecdotal / Single Episode\n\
                - 3: Corroborated / Tested\n\
                - 5: Proven / Universal\n\n\
                For 'node_type', actively check the events for contradictory evidence. If any conflicting or contradictory evidence is detected, set 'node_type' to 'conflict'. Otherwise, set it to 'insight'.";
                let sys_prompt = crate::cognitive::synthesis::build_synthesis_prompt(base_sys);
                
                let prompt_text = format!(
                    "Please analyze these events:\n\n{}Respond ONLY with JSON matching: {{ \"title\": \"...\", \"summary\": \"...\", \"metacognitive_confidence\": 3, \"node_type\": \"insight\" }}",
                    events_text
                );

                let original_tokens = events_text.len() / 4;
                let llm_res = self.llm.routed_completion(db, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Summarization), Some(&sys_prompt), &prompt_text).await?;
                crate::cognitive::synthesis::check_compression_ratio(&prompt_text, &llm_res, original_tokens);
                
                #[derive(serde::Deserialize)]
                struct ClusterAnalysis {
                    title: String,
                    summary: String,
                    #[serde(default)]
                    metacognitive_confidence: Option<i32>,
                    #[serde(default)]
                    node_type: Option<String>,
                }
                
                let analysis: ClusterAnalysis = match serde_json::from_str(&llm_res) {
                    Ok(a) => a,
                    Err(_) => {
                        ClusterAnalysis {
                            title: format!("Cluster Analysis {}", &uuid::Uuid::new_v4().to_string()[..8]),
                            summary: llm_res,
                            metacognitive_confidence: None,
                            node_type: None,
                        }
                    }
                };

                let cluster_ep_ids: Vec<String> = cluster_eps.iter().map(|ep| ep.id.clone().unwrap_or_default()).collect();

                let clean_title = slugify_title(&analysis.title);
                let relative_path = format!("wiki/{}/insights/{}.md", scope, clean_title);
                let insight_content = format!(
                    "---\ntitle: \"{}\"\nscope: \"{}\"\nsource_episodes:\n{}\nmetacognitive_confidence: {}\nnode_type: \"{}\"\n---\n\n{}",
                    analysis.title,
                    scope,
                    cluster_ep_ids.iter().map(|id| format!("  - \"{}\"", id)).collect::<Vec<_>>().join("\n"),
                    analysis.metacognitive_confidence.unwrap_or(3),
                    analysis.node_type.as_deref().unwrap_or("insight"),
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
                    metacognitive_confidence: analysis.metacognitive_confidence,
                    node_type: analysis.node_type.clone().or(Some("insight".to_string())),
                    ..Default::default()
                };
                if let Ok(wiki_node_id) = self.save_wiki_node_with_contradiction_resolution(db, store, &node_contract, embedder.clone()).await {
                    for ep_id in &cluster_ep_ids {
                        let _ = db.relate_nodes(ep_id, &wiki_node_id, None, None, None).await;
                    }
                }
                
                insights_changed += 1;
                if insights_changed > 5 {
                    tracing::info!("Scope '{}' exceeded 5 insight changes. Triggering interleaved compaction.", scope);
                    let compactor = crate::cognitive::compactor::Compactor::new();
                    let _ = compactor.compact_scope(db, store, &scope, embedder.clone()).await;
                    insights_changed = 0;
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
                            let max_tokens = 2048;
                            let truncated_content = truncate_by_tokens(&ep.content, max_tokens, embedder.as_deref());
                            episodes_needing_embedding.push(truncated_content);
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

                        // Prepare references for DBSCAN
                        let emb_refs: Vec<&[f32]> = episode_embeddings.iter().map(|e| e.as_slice()).collect();
                        let labels = dbscan(&emb_refs, 0.08, 2);

                        // Group episodes by DBSCAN labels
                        let mut clusters: std::collections::HashMap<usize, Vec<Episode>> = std::collections::HashMap::new();
                        let mut outliers = Vec::new();

                        for (idx, label) in labels.clone().into_iter().enumerate() {
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
                            let base_sys = "You are a systems synthesizer. Analyze the cluster of events and output a JSON object containing the fields: 'title', 'summary', 'metacognitive_confidence', and 'node_type'.\n\n\
                            For 'metacognitive_confidence', use the following strict integer rubric (1-5):\n\
                            - 1: Anecdotal / Single Episode\n\
                            - 3: Corroborated / Tested\n\
                            - 5: Proven / Universal\n\n\
                            For 'node_type', actively check the events for contradictory evidence. If any conflicting or contradictory evidence is detected, set 'node_type' to 'conflict'. Otherwise, set it to 'insight'.";
                            let sys_prompt = crate::cognitive::synthesis::build_synthesis_prompt(base_sys);
                            
                            let prompt_text = format!(
                                "Please analyze these events:\n\n{}Respond ONLY with JSON matching: {{ \"title\": \"...\", \"summary\": \"...\", \"metacognitive_confidence\": 3, \"node_type\": \"insight\" }}",
                                events_text
                            );
                            
                            let original_tokens = events_text.len() / 4;
                            if let Ok(llm_res) = self.llm.routed_completion(db, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Summarization), Some(&sys_prompt), &prompt_text).await {
                                crate::cognitive::synthesis::check_compression_ratio(&prompt_text, &llm_res, original_tokens);
                                #[derive(serde::Deserialize)]
                                struct ClusterAnalysis {
                                    title: String,
                                    summary: String,
                                    #[serde(default)]
                                    metacognitive_confidence: Option<i32>,
                                    #[serde(default)]
                                    node_type: Option<String>,
                                }

                                let analysis: ClusterAnalysis = match serde_json::from_str(&llm_res) {
                                    Ok(a) => a,
                                    Err(_) => {
                                        ClusterAnalysis {
                                            title: format!("Split Analysis {}", &uuid::Uuid::new_v4().to_string()[..8]),
                                            summary: llm_res,
                                            metacognitive_confidence: None,
                                            node_type: None,
                                        }
                                    }
                                };

                                // Write new insight to disk
                                let clean_title = slugify_title(&analysis.title);
                                let relative_path = format!("wiki/{}/insights/{}.md", scope, clean_title);

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
                                    "---\ntitle: \"{}\"\nscope: \"{}\"\nsource_episodes:\n{}\nmetacognitive_confidence: {}\nnode_type: \"{}\"\n---\n\n{}{}",
                                    analysis.title,
                                    scope,
                                    group.iter().map(|ep| format!("  - \"{}\"", ep.id.as_ref().unwrap_or(&String::new()))).collect::<Vec<_>>().join("\n"),
                                    analysis.metacognitive_confidence.unwrap_or(3),
                                    analysis.node_type.as_deref().unwrap_or("insight"),
                                    analysis.summary,
                                    source_ep_section
                                );
                                let write_res = store.write_file(&relative_path, &insight_content);

                                if write_res.is_ok() {
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
                                        metacognitive_confidence: analysis.metacognitive_confidence,
                                        node_type: analysis.node_type.clone().or(Some("insight".to_string())),
                                        ..Default::default()
                                    };
                                    
                                    let save_res = self.save_wiki_node_with_contradiction_resolution(db, store, &node_contract, embedder.clone()).await;

                                    if let Ok(wiki_node_id) = save_res {
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
                        tracing::debug!("DEBUG: rel_path: '{}', vault_path: '{}', vault_root: '{}'", rel_path, ins.vault_path, store.vault_root.display());
                        let _ = db.delete_by_vault_path(&rel_path).await;

                    }
                }
            }
            // --- DRIFT & SPLIT MANAGEMENT LOGIC END ---
        }
        }

        // --- Tasks C.6 & C.6a: Cross-Scope Graduation Pass ---
        if db.is_feature_enabled("compactor.enable_cross_scope_graduation", true).await {
            #[derive(Clone, Debug)]
            struct GradCandidate {
                id: String,
                scope: String,
                name: String,
                content: String,
                embedding: Vec<f32>,
                is_procedural: bool,
            }

            let mut candidates = Vec::new();

            // 1. Fetch wiki nodes
            if let Ok(wiki_nodes) = db.get_all_wiki_nodes().await {
                for node in wiki_nodes {
                    if let Some(ref emb) = node.embedding {
                        candidates.push(GradCandidate {
                            id: node.id.unwrap_or_default().replace("`", ""),
                            scope: node.scope,
                            name: node.name,
                            content: node.content,
                            embedding: emb.clone(),
                            is_procedural: false,
                        });
                    }
                }
            }

            // 2. Fetch procedural episodes
            if let Ok(episodes) = db.get_episodes_by_node_type("procedural").await {
                for ep in episodes {
                    if !ep.archived.unwrap_or(false) {
                        if let Some(ref emb) = ep.embedding {
                            candidates.push(GradCandidate {
                                id: ep.id.unwrap_or_default().replace("`", ""),
                                scope: ep.scope.unwrap_or_else(|| "general".to_string()),
                                name: ep.title,
                                content: ep.content,
                                embedding: emb.clone(),
                                is_procedural: true,
                            });
                        }
                    }
                }
            }

            let hnsw_ef = match db.get_profile_key("search.hnsw_ef").await {
                Ok(Some(val_str)) => val_str.parse::<usize>().unwrap_or(100),
                _ => 100,
            };

            let mut clusters: Vec<Vec<GradCandidate>> = Vec::new();

            for cand in &candidates {
                let mut cluster = vec![cand.clone()];

                let mut matches_wiki = Vec::new();
                let mut matches_ep = Vec::new();

                if let Some(surreal_backend) = db.as_any().downcast_ref::<crate::db::backend::SurrealBackend>() {
                    // Search wiki_node HNSW index
                    let sql_wiki = format!("SELECT *, vector::similarity::cosine(embedding, $emb) AS similarity \
                               FROM wiki_node \
                               WHERE embedding <|200, {}|> $emb;", hnsw_ef);
                    if let Ok(mut resp) = surreal_backend.db.query(&sql_wiki).bind(("emb", cand.embedding.clone())).await {
                        if let Ok(rows) = resp.take::<Vec<serde_json::Value>>(0) {
                            for row in rows {
                                let sim = row.get("similarity").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                if sim >= 0.85 {
                                    if let Ok(node) = serde_json::from_value::<WikiNode>(row) {
                                        if node.scope != cand.scope {
                                            matches_wiki.push(node);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Search procedural episodes HNSW index
                    let sql_ep = format!("SELECT *, vector::similarity::cosine(embedding, $emb) AS similarity \
                             FROM episode \
                             WHERE node_type = 'procedural' AND embedding <|200, {}|> $emb;", hnsw_ef);
                    if let Ok(mut resp) = surreal_backend.db.query(&sql_ep).bind(("emb", cand.embedding.clone())).await {
                        if let Ok(rows) = resp.take::<Vec<serde_json::Value>>(0) {
                            for row in rows {
                                let sim = row.get("similarity").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                if sim >= 0.85 {
                                    if let Ok(ep) = serde_json::from_value::<Episode>(row) {
                                        let ep_scope = ep.scope.clone().unwrap_or_else(|| "general".to_string());
                                        if ep_scope != cand.scope {
                                            matches_ep.push(ep);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // In-memory similarity fallbacks for testing / mocked environments where index is not dynamically built
                if matches_wiki.is_empty() {
                    for other in &candidates {
                        if !other.is_procedural && other.scope != cand.scope {
                            let sim = {
                                let dot: f32 = cand.embedding.iter().zip(other.embedding.iter()).map(|(a, b)| a * b).sum();
                                let norm_u: f32 = cand.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                                let norm_v: f32 = other.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                                if norm_u == 0.0 || norm_v == 0.0 {
                                    0.0
                                } else {
                                    dot / (norm_u * norm_v)
                                }
                            };
                            if sim >= 0.85 {
                                cluster.push(other.clone());
                            }
                        }
                    }
                } else {
                    for node in matches_wiki {
                        cluster.push(GradCandidate {
                            id: node.id.unwrap_or_default().replace("`", ""),
                            scope: node.scope,
                            name: node.name,
                            content: node.content,
                            embedding: node.embedding.unwrap_or_default(),
                            is_procedural: false,
                        });
                    }
                }

                if matches_ep.is_empty() {
                    for other in &candidates {
                        if other.is_procedural && other.scope != cand.scope {
                            let sim = {
                                let dot: f32 = cand.embedding.iter().zip(other.embedding.iter()).map(|(a, b)| a * b).sum();
                                let norm_u: f32 = cand.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                                let norm_v: f32 = other.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                                if norm_u == 0.0 || norm_v == 0.0 {
                                    0.0
                                } else {
                                    dot / (norm_u * norm_v)
                                }
                            };
                            if sim >= 0.85 {
                                if !cluster.iter().any(|c| c.id == other.id) {
                                    cluster.push(other.clone());
                                }
                            }
                        }
                    }
                } else {
                    for ep in matches_ep {
                        let ep_scope = ep.scope.clone().unwrap_or_else(|| "general".to_string());
                        cluster.push(GradCandidate {
                            id: ep.id.unwrap_or_default().replace("`", ""),
                            scope: ep_scope,
                            name: ep.title,
                            content: ep.content,
                            embedding: ep.embedding.unwrap_or_default(),
                            is_procedural: true,
                        });
                    }
                }

                let distinct_scopes: std::collections::HashSet<String> = cluster.iter().map(|c| c.scope.clone()).collect();
                if distinct_scopes.len() >= 2 {
                    cluster.sort_by(|a, b| a.id.cmp(&b.id));
                    clusters.push(cluster);
                }
            }

            // Deduplicate overlapping clusters: sort clusters by size descending, and skip clusters whose anchor is already in a larger accepted cluster
            clusters.sort_by(|a, b| b.len().cmp(&a.len()));

            let mut accepted_clusters = Vec::new();
            let mut seen_anchors = std::collections::HashSet::new();

            for cluster in clusters {
                let anchor_id = &cluster[0].id;
                if seen_anchors.contains(anchor_id) {
                    continue;
                }
                
                // Mark all members of this cluster as seen anchors
                for member in &cluster {
                    seen_anchors.insert(member.id.clone());
                }
                accepted_clusters.push(cluster);
            }
            println!("DEBUG - accepted_clusters.len() = {}", accepted_clusters.len());
            for (idx, c) in accepted_clusters.iter().enumerate() {
                println!("DEBUG - cluster {} members: {:?}", idx, c.iter().map(|m| &m.id).collect::<Vec<_>>());
            }

            for cluster in accepted_clusters {
                let n = cluster.len();
                let mut insights_with_scope_labels = String::new();
                for member in &cluster {
                    insights_with_scope_labels.push_str(&format!(
                        "Scope: {}\nTitle/Name: {}\nContent:\n{}\n\n",
                        member.scope, member.name, member.content
                    ));
                }

                let base_sys = "You are a knowledge generalizer. Given project-specific insights that independently emerged in multiple projects, synthesize a single general-purpose rule that captures the cross-cutting pattern. Strip project-specific details. Output valid JSON.";
                let sys_prompt = crate::cognitive::synthesis::build_synthesis_prompt(base_sys);
                let user_prompt = format!(
                    "The following insights emerged independently in {} different projects:\n\n{}Respond with a JSON object containing target_pattern: string, action_to_avoid: string, causal_explanation: string, prescribed_remedy: string, and confidence: float.",
                    n,
                    insights_with_scope_labels
                );

                let original_tokens = insights_with_scope_labels.len() / 4;
                match self.llm.routed_completion(db, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Reasoning), Some(&sys_prompt), &user_prompt).await {
                    Ok(resp_str) => {
                        crate::cognitive::synthesis::check_compression_ratio(&user_prompt, &resp_str, original_tokens);
                        println!("DEBUG - routed_completion succeeded: {:?}", resp_str);
                        #[derive(serde::Deserialize)]
                        struct GeneralizationResponse {
                            target_pattern: String,
                            action_to_avoid: String,
                            causal_explanation: String,
                            prescribed_remedy: String,
                            confidence: f32,
                        }
                        let clean_resp = crate::llm::strip_code_fences(&resp_str);
                        match serde_json::from_str::<GeneralizationResponse>(&clean_resp) {
                            Ok(res) => {
                                println!("DEBUG - JSON parsing succeeded: confidence={}", res.confidence);
                                if res.confidence >= 0.80 {
                                    let tier = "dynamic";

                            let rule_path = resolve_rule_path("general", &res.action_to_avoid);

                            let mut rule_md = format!(
                                "---\ntarget_pattern: \"{}\"\naction_to_avoid: \"{}\"\ncausal_explanation: \"{}\"\nprescribed_remedy: \"{}\"\ntier: \"{}\"\nscope: \"general\"\nsource_nodes:\n{}\ngenerator_name: \"ScopeGraduator\"\n---\n\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}",
                                res.target_pattern, res.action_to_avoid, res.causal_explanation, res.prescribed_remedy, tier,
                                cluster.iter().map(|c| format!("  - \"{}\"", c.id)).collect::<Vec<_>>().join("\n"),
                                res.target_pattern, res.action_to_avoid, res.causal_explanation, res.prescribed_remedy
                            );
                            rule_md.push_str("\n\n## Source Insights\n");
                            for member in &cluster {
                                rule_md.push_str(&format!("- [[{}]]\n", member.name));
                            }
                            let _ = store.write_file(&rule_path, &rule_md);

                            let rule_contract = WisdomRule {
                                id: None,
                                target_pattern: res.target_pattern,
                                action_to_avoid: res.action_to_avoid,
                                causal_explanation: res.causal_explanation,
                                prescribed_remedy: res.prescribed_remedy,
                                tier: crate::contracts::Tier::Project,
                                scope: "general".to_string(),
                                vault_path: Some(rule_path),
                                embedding: None,
                                source_episodes: cluster.iter().map(|c| c.id.clone()).collect(),
                                generator_name: "ScopeGraduator".to_string(),
                                similarity: None,
                                utility: None,
                                status: None,
                                superseded_at: None,
                                superseded_by: None,
                                rule_type: Some("aesthetic".to_string()),
                            
                                ..Default::default()
                            };

                            match save_wisdom_rule_with_deduplication(db, store, &rule_contract).await {
                                Ok(wisdom_id) => {
                                    println!("DEBUG - save_wisdom_rule_with_deduplication succeeded: {}", wisdom_id);
                                    for member in &cluster {
                                        let _ = db.relate_nodes(&member.id, &wisdom_id, None, None, None).await;
                                    }
                                }
                                Err(e) => {
                                    println!("DEBUG - save_wisdom_rule_with_deduplication failed: {:?}", e);
                                }
                            }
                            }
                        }
                        Err(e) => {
                            println!("DEBUG - JSON parsing failed: error={:?}, clean_resp={:?}", e, clean_resp);
                        }
                    }
                    }
                    Err(e) => {
                        println!("DEBUG - routed_completion failed: {:?}", e);
                    }
                }
            }
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

        let sim = crate::math::cosine_similarity(&new_emb, &existing_emb);
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
        if matched.tier == crate::contracts::Tier::Wisdom {
            if let Some(ref vp) = rule.vault_path {
                let _ = safe_delete_file(&store.vault_root, vp);
            }
            if let Some(ref skills_id) = matched.id {
                for ep in &rule.source_episodes {
                    let _ = db.relate_nodes(ep, skills_id, None, None, None).await;
                }
                return Ok(skills_id.clone());
            }
        } else if matched.tier == crate::contracts::Tier::Project {
            let base_sys = "You are an expert software engineer and systems architect. Merge and generalize two similar wisdom rules into a single, high-quality, comprehensive wisdom rule.";
            let system_prompt = crate::cognitive::synthesis::build_synthesis_prompt(base_sys);
            let prompt = format!(
                "Rule 1:\nPattern: {}\nAvoid: {}\nWhy: {}\nRemedy: {}\n\n\
                 Rule 2:\nPattern: {}\nAvoid: {}\nWhy: {}\nRemedy: {}\n\n\
                 Please merge and generalize these two similar rules into a single comprehensive rule. \
                 Respond ONLY with a JSON object matching the structure of WisdomRule, with fields:\n\
                 - target_pattern\n- action_to_avoid\n- causal_explanation\n- prescribed_remedy",
                matched.target_pattern, matched.action_to_avoid, matched.causal_explanation, matched.prescribed_remedy,
                rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
            );

            let original_tokens = (matched.causal_explanation.len() + rule.causal_explanation.len()) / 4;
            let llm = crate::llm::LLMClient::default();
            match llm.routed_completion(db, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Reasoning), Some(&system_prompt), &prompt).await {
                Ok(res) => {
                    crate::cognitive::synthesis::check_compression_ratio(&prompt, &res, original_tokens);
                    let stripped = crate::llm::strip_code_fences(&res);

                    #[derive(serde::Deserialize)]
                    struct MergedFields {
                        target_pattern: String,
                        action_to_avoid: String,
                        causal_explanation: String,
                        prescribed_remedy: String,
                    }

                    let parsed_fields = if stripped.starts_with('[') {
                        if let Ok(list) = serde_json::from_str::<Vec<MergedFields>>(&stripped) {
                            list.into_iter().next()
                        } else {
                            None
                        }
                    } else {
                        serde_json::from_str::<MergedFields>(&stripped).ok()
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
                            resolve_rule_path(&rule.scope, &fields.action_to_avoid)
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
                            tier: crate::contracts::Tier::Project,
                            scope: rule.scope.clone(),
                            vault_path: Some(final_path),
                            embedding: None,
                            source_episodes: merged_eps,
                            generator_name: rule.generator_name.clone(),
                            similarity: None,
                            utility: None,
                            status: None,
                            superseded_at: None,
                            superseded_by: None,
                            rule_type: None,
                        
                            ..Default::default()
                        };

                        match db.save_wisdom_rule(&merged_contract).await {
                            Ok(saved_id) => {
                                let old_uuid = matched.id.as_ref().unwrap().strip_prefix("wisdom:").unwrap_or(matched.id.as_ref().unwrap());
                                let new_uuid = saved_id.strip_prefix("wisdom:").unwrap_or(&saved_id);

                                if old_uuid != new_uuid {
                                    // 1. Update old rule status to "superseded" and set superseded_at in SurrealDB
                                    if let Some(surreal_backend) = db.as_any().downcast_ref::<crate::db::SurrealBackend>() {
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

pub async fn traverse_adjacent_logs(
    db: &dyn crate::db::StorageBackend,
    start_nodes: &[String],
    max_depth: usize,
    max_nodes: usize,
    max_tokens: usize,
    session_id: Option<&str>,
    include_types: Option<&[&str]>,
) -> Result<Vec<crate::contracts::Episode>> {
    let Some(surreal) = db.as_any().downcast_ref::<crate::db::SurrealBackend>() else {
        anyhow::bail!("traverse_adjacent_logs is only supported on SurrealBackend");
    };

    use std::collections::HashSet;
    let mut visited = HashSet::new();
    let mut results = Vec::new();
    let mut current_level = Vec::new();

    for node_id in start_nodes {
        if !node_id.is_empty() {
            visited.insert(node_id.clone());
            current_level.push(node_id.clone());
        }
    }

    let char_cap = max_tokens * 4;
    let mut accumulated_chars = 0;

    for _depth in 0..=max_depth {
        if current_level.is_empty() || results.len() >= max_nodes || accumulated_chars >= char_cap {
            break;
        }

        // Convert string IDs to RecordIds
        let mut record_ids = Vec::new();
        for id_str in &current_level {
            if let Ok(rid) = crate::db::backend::parse_record_id(id_str) {
                record_ids.push(rid);
            }
        }

        if record_ids.is_empty() {
            break;
        }

        // Query adjacent nodes
        let sql = "SELECT id, title, content, scope, vault_path, embedding, processed_in_dream, source_episode, last_retrieved_at, utility, archived, archived_at, discovery_tokens, facts, concepts, files_read, files_modified, session_id, word_count, node_type,
                  <-followed_by<-episode AS followed_by_in,
                  ->followed_by->episode AS followed_by_out,
                  <-relates_to<-episode AS relates_to_in,
                  ->relates_to->episode AS relates_to_out
           FROM episode WHERE id IN $ids;";

        let mut query_res = surreal.db.query(sql).bind(("ids", record_ids)).await?;
        
        #[derive(serde::Serialize, serde::Deserialize, Debug, SurrealValue)]
        struct TraversalNode {
            id: surrealdb::types::RecordId,
            title: String,
            content: String,
            scope: Option<String>,
            vault_path: Option<String>,
            embedding: Option<Vec<f32>>,
            processed_in_dream: Option<bool>,
            source_episode: Option<String>,
            last_retrieved_at: Option<String>,
            utility: Option<f32>,
            archived: Option<bool>,
            archived_at: Option<String>,
            discovery_tokens: Option<u32>,
            facts: Option<Vec<String>>,
            concepts: Option<Vec<String>>,
            files_read: Option<Vec<String>>,
            files_modified: Option<Vec<String>>,
            session_id: Option<String>,
            word_count: Option<u32>,
            node_type: Option<String>,
            
            #[serde(default)]
            followed_by_in: Vec<surrealdb::types::RecordId>,
            #[serde(default)]
            followed_by_out: Vec<surrealdb::types::RecordId>,
            #[serde(default)]
            relates_to_in: Vec<surrealdb::types::RecordId>,
            #[serde(default)]
            relates_to_out: Vec<surrealdb::types::RecordId>,
        }

        let nodes: Vec<TraversalNode> = query_res.take(0)?;
        let mut next_level = Vec::new();

        for node in nodes {
            if results.len() >= max_nodes || accumulated_chars >= char_cap {
                break;
            }

            // Session isolation check
            if let Some(target_sess) = session_id {
                if let Some(ref node_sess) = node.session_id {
                    if node_sess != target_sess {
                        continue;
                    }
                }
            }

            // Include types check
            if let Some(types) = include_types {
                if let Some(ref ntype) = node.node_type {
                    if !types.contains(&ntype.as_str()) {
                        continue;
                    }
                } else {
                    continue; // Skip if node_type is None and we filter
                }
            }

            let node_id_str = crate::db::backend::format_record_id(&node.id);
            let node_char_count = node.content.chars().count();

            if accumulated_chars + node_char_count > char_cap {
                // Truncate this node using truncate_to_boundary
                let remaining = char_cap - accumulated_chars;
                let truncated_content = truncate_to_boundary(&node.content, remaining).to_string();
                results.push(crate::contracts::Episode {
                    id: Some(node_id_str),
                    title: node.title,
                    content: truncated_content,
                    source: None,
                    scope: node.scope,
                    vault_path: node.vault_path,
                    embedding: node.embedding,
                    processed_in_dream: node.processed_in_dream,
                    source_episode: node.source_episode,
                    last_retrieved_at: node.last_retrieved_at,
                    utility: node.utility,
                    archived: node.archived,
                    discovery_tokens: node.discovery_tokens,
                    facts: node.facts,
                    concepts: node.concepts,
                    files_read: node.files_read,
                    files_modified: node.files_modified,
                    session_id: node.session_id,
                    word_count: node.word_count,
                    archived_at: node.archived_at,
                    node_type: node.node_type,
                    confidence: None,
                
                    ..Default::default()
                });
                accumulated_chars = char_cap;
                break;
            } else {
                accumulated_chars += node_char_count;
                results.push(crate::contracts::Episode {
                    id: Some(node_id_str),
                    title: node.title,
                    content: node.content,
                    source: None,
                    scope: node.scope,
                    vault_path: node.vault_path,
                    embedding: node.embedding,
                    processed_in_dream: node.processed_in_dream,
                    source_episode: node.source_episode,
                    last_retrieved_at: node.last_retrieved_at,
                    utility: node.utility,
                    archived: node.archived,
                    discovery_tokens: node.discovery_tokens,
                    facts: node.facts,
                    concepts: node.concepts,
                    files_read: node.files_read,
                    files_modified: node.files_modified,
                    session_id: node.session_id,
                    word_count: node.word_count,
                    archived_at: node.archived_at,
                    node_type: node.node_type,
                    confidence: None,
                
                    ..Default::default()
                });
            }

            // Collect unique adjacent node IDs for the next level traversal
            let mut add_adjacent = |adj_list: Vec<surrealdb::types::RecordId>| {
                for adj_id in adj_list {
                    let adj_str = crate::db::backend::format_record_id(&adj_id);
                    if !visited.contains(&adj_str) {
                        visited.insert(adj_str.clone());
                        next_level.push(adj_str);
                    }
                }
            };

            add_adjacent(node.followed_by_in);
            add_adjacent(node.followed_by_out);
            add_adjacent(node.relates_to_in);
            add_adjacent(node.relates_to_out);
        }

        current_level = next_level;
    }

    Ok(results)
}

pub fn truncate_by_tokens(
    text: &str,
    max_tokens: usize,
    embedder: Option<&dyn crate::embeddings::TextEmbedder>,
) -> String {
    if let Some(emb) = embedder {
        let count = emb.count_tokens(text).unwrap_or(text.len() / 4);
        if count <= max_tokens {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let mut low = 0;
        let mut high = chars.len();
        let mut best_fit = 0;
        while low <= high {
            let mid = (low + high) / 2;
            let candidate: String = chars[..mid].iter().collect();
            let tokens = emb.count_tokens(&candidate).unwrap_or(mid / 4);
            if tokens <= max_tokens {
                best_fit = mid;
                low = mid + 1;
            } else {
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            }
        }
        chars[..best_fit].iter().collect()
    } else {
        let max_chars = max_tokens * 4;
        let char_count = text.chars().count();
        if char_count <= max_chars {
            return text.to_string();
        }
        text.chars().take(max_chars).collect()
    }
}

pub async fn backpropagate_directions(db: &dyn StorageBackend, store: &MarkdownStore) -> Result<()> {
    let all_nodes = db.get_all_wiki_nodes().await?;
    let directions: Vec<_> = all_nodes.into_iter().filter(|n| n.node_type.as_deref() == Some("direction")).collect();

    for dir_node in directions {
        println!("Checking direction {:?}", dir_node.id);
        if let Some(ref dir_id) = dir_node.id {
            let related_ids = db.get_related_node_ids(dir_id).await.unwrap_or_default();
            println!("Related IDs for {}: {:?}", dir_id, related_ids);
            if related_ids.is_empty() { continue; }
            
            let mem_nodes = db.get_memory_nodes(&related_ids).await?;
            let insights = mem_nodes.wiki_nodes;
            println!("Fetched {} wiki nodes", insights.len());

            
            if insights.is_empty() { continue; }
            
            let base_sys = "You are a direction synthesizer. Synthesize the child insights into an updated Current Understanding section.";
            let sys_prompt = crate::cognitive::synthesis::build_synthesis_prompt(base_sys);
            let mut prompt_text = format!("Existing Direction Content:\n{}\n\nChild Insights:\n", dir_node.content);
            for ins in &insights {
                prompt_text.push_str(&format!("- {}: {}\n", ins.name, ins.content));
            }
            
            let _original_tokens = dir_node.content.len() / 4;
            let llm = crate::llm::LLMClient::default();
            match llm.routed_completion(db, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Summarization), Some(&sys_prompt), &prompt_text).await {
                Ok(updated_understanding) => {
                    let mut updated_node = dir_node.clone();
                    updated_node.content = updated_understanding.clone();
                    
                    db.save_wiki_node(&updated_node).await.unwrap();
                    
                    let slug = crate::cognitive::synthesis::slugify_title(&updated_node.name);
                    let rel_path = format!("wiki/{}/directions/{}.md", updated_node.scope, slug);
                    let new_content = format!("---\ntitle: \"{}\"\nscope: \"{}\"\nnode_type: \"direction\"\n---\n\n## Current Understanding\n{}", updated_node.name, updated_node.scope, updated_understanding);
                    let _ = store.write_file(&rel_path, &new_content);
                },
                Err(e) => println!("routed_completion failed: {:?}", e),
            }
        }
    }
    Ok(())
}

pub async fn promote_insight_to_direction(
    db: &dyn StorageBackend,
    _store: &MarkdownStore,
    node: &WikiNode,
    episodes: &[crate::contracts::Episode],
) -> Result<()> {
    let mut high_conf_count = 0;
    for ep in episodes {
        if let Some(conf) = ep.confidence {
            if conf >= 4.0 { high_conf_count += 1; }
        }
    }
    
    let mut drift = 0.0;
    if !episodes.is_empty() {
        if let Some(initial_emb) = &episodes[0].embedding {
            if let Some(current_emb) = &node.embedding {
                drift = crate::cognitive::synthesis::cosine_distance(initial_emb, current_emb);
            } else {
                let mut sum = vec![0.0; initial_emb.len()];
                let mut count = 0;
                for ep in episodes {
                    if let Some(ref emb) = ep.embedding {
                        for (i, val) in emb.iter().enumerate() { sum[i] += val; }
                        count += 1;
                    }
                }
                if count > 0 {
                    for val in &mut sum { *val /= count as f32; }
                    let norm: f32 = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
                    if norm > 0.0 { for val in &mut sum { *val /= norm; } }
                    drift = crate::cognitive::synthesis::cosine_distance(initial_emb, &sum);
                }
            }
        }
    }

    if drift > 0.20 || high_conf_count > 15 {
        let mut dir_node = node.clone();
        dir_node.node_type = Some("direction".to_string());
        println!("Saving promoted direction node: {:#?}", dir_node);
        match db.save_wiki_node(&dir_node).await {
            Ok(_) => println!("Save succeeded"),
            Err(e) => println!("Save failed: {:?}", e),
        }
        
        let embs: Vec<&[f32]> = episodes.iter().filter_map(|e| e.embedding.as_deref()).collect();
        if !embs.is_empty() {
            let _labels = crate::cognitive::synthesis::dbscan(&embs, 0.15, 2);
        }
    }
    
    Ok(())
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

    #[test]
    fn test_truncate_by_tokens_heuristic() {
        let long_text = "a".repeat(10000);
        let truncated = truncate_by_tokens(&long_text, 2048, None);
        assert_eq!(truncated.len(), 2048 * 4);
    }
}

pub async fn graduate_wisdom(db: &dyn crate::db::StorageBackend, store: &crate::store::MarkdownStore) -> Result<()> {
    let all_nodes = db.get_all_wiki_nodes().await?;
    let mut directions = Vec::new();
    for node in all_nodes {
        if node.node_type.as_deref() == Some("direction") && node.embedding.is_some() {
            directions.push(node);
        }
    }

    if directions.is_empty() {
        return Ok(());
    }

    let mut to_graduate = Vec::new();
    let mut handled = std::collections::HashSet::new();

    for i in 0..directions.len() {
        if handled.contains(&i) { continue; }
        
        let mut cluster = vec![&directions[i]];
        let emb_i = directions[i].embedding.as_ref().unwrap();

        for j in (i + 1)..directions.len() {
            if handled.contains(&j) { continue; }
            if directions[i].scope == directions[j].scope { continue; }
            
            let emb_j = directions[j].embedding.as_ref().unwrap();
            let sim = crate::math::cosine_similarity(emb_i, emb_j);
            if sim >= 0.85 {
                cluster.push(&directions[j]);
                handled.insert(j);
            }
        }

        if cluster.len() > 1 {
            to_graduate.push(cluster);
            handled.insert(i);
        }
    }

    for cluster in to_graduate {
        let mut has_conflict = false;
        let mut source_ep_ids = std::collections::HashSet::new();
        
        for node in &cluster {
            let id = node.id.clone().unwrap_or_default();
            source_ep_ids.insert(id.clone());
            
            let mut visited = std::collections::HashSet::new();
            let mut queue = vec![id.clone()];
            visited.insert(id.clone());
            
            while let Some(current) = queue.pop() {
                if let Ok(related) = db.get_related_node_ids(&current).await {
                    if let Ok(mem_nodes) = db.get_memory_nodes(&related).await {
                        for related_node in mem_nodes.wiki_nodes {
                            if related_node.node_type.as_deref() == Some("conflict") {
                                has_conflict = true;
                                break;
                            }
                            let rel_id = related_node.id.unwrap_or_default();
                            if !visited.contains(&rel_id) {
                                visited.insert(rel_id.clone());
                                queue.push(rel_id);
                            }
                        }
                        if has_conflict { break; }
                    }
                }
            }
            if has_conflict { break; }
        }

        if has_conflict {
            continue;
        }

        let rule_class = "system_constraint";
        let target_pattern = cluster[0].name.clone();
        let slug = slugify_title(&target_pattern);
        let rule_path = format!("wisdom/{}/{}.md", rule_class, slug);
        
        let rule = crate::contracts::WisdomRule {
            target_pattern: target_pattern.clone(),
            action_to_avoid: target_pattern.clone(),
            causal_explanation: "Synthesized via cross-scope graduation.".into(),
            prescribed_remedy: cluster[0].content.clone(),
            tier: crate::contracts::Tier::Wisdom,
            scope: "general".into(),
            rule_type: Some(rule_class.to_string()),
            source_episodes: source_ep_ids.into_iter().collect(),
            generator_name: "WisdomGraduator".into(),
            vault_path: Some(rule_path.clone()),
            ..Default::default()
        };

        if let Ok(_wisdom_id) = save_wisdom_rule_with_deduplication(db, store, &rule).await {
            let rule_md = format!(
                "---\ntarget_pattern: \"{}\"\naction_to_avoid: \"{}\"\ncausal_explanation: \"{}\"\nprescribed_remedy: \"{}\"\ntier: \"{}\"\nscope: \"general\"\n---\n\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}",
                rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy, rule.tier.as_str(),
                rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
            );
            let _ = store.write_file(&rule_path, &rule_md);
        }
    }

    Ok(())
}
