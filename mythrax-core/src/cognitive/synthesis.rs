use crate::db::StorageBackend;
use crate::llm::LLMClient;
use crate::store::MarkdownStore;
use crate::contracts::{Episode, WisdomRule};
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
                if insights_dir.exists() {
                    if let Ok(files) = std::fs::read_dir(&insights_dir) {
                        for file in files.flatten() {
                            if file.path().extension().map(|s| s == "md").unwrap_or(false) {
                                if let Ok(content) = std::fs::read_to_string(file.path()) {
                                    if let Ok(note) = parse_insight_note(&content, &file.path(), &scope) {
                                        insights.push(note);
                                    }
                                }
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
        if let Some(ep) = all_episodes.iter().find(|e| e.id.as_ref() == Some(ep_id)) {
            if let Some(ref emb) = ep.embedding {
                if sum.is_empty() {
                    sum = vec![0.0; emb.len()];
                }
                for (i, val) in emb.iter().enumerate() {
                    sum[i] += val;
                }
                count += 1;
            }
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

        if settings_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&settings_path) {
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

        for (scope, new_episodes) in scope_groups {
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

                for ep in new_episodes {
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
                        let prompt_text = format!(
                            "Existing Insight Body:\n{}\n\nNew Event content:\nTitle: {}\n{}",
                            ins.content, ep.title, ep.content
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

            for (_, cluster_eps) in clusters {
                let mut events_text = String::new();
                for ep in &cluster_eps {
                    events_text.push_str(&format!("Event: {}\nContent:\n{}\n\n", ep.title, ep.content));
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
                            title: format!("Cluster Analysis {}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
                            summary: llm_res,
                        }
                    }
                };

                let cluster_ep_ids: Vec<String> = cluster_eps.iter().map(|ep| ep.id.clone().unwrap_or_default()).collect();

                let clean_title = analysis.title.replace(' ', "_").replace('/', "_");
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
                            let rule_path = format!("wisdom/dynamic/{}_{}.md", r.target_pattern.replace(' ', "_").replace('/', "_"), &rule_uuid[..8]);
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
                            let _ = db.save_wisdom_rule(&rule_contract).await;
                        }
                    }
                }

                for ep in cluster_eps {
                    if let Some(ref ep_id) = ep.id {
                        db.mark_episode_processed(ep_id).await?;
                    }
                }
            }
        }

        Ok(())
    }
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
        assert_eq!(labels[0].is_some(), true);
        assert_eq!(labels[2], None);
    }
}
