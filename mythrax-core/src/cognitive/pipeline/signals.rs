use crate::contracts::{ArborNode, Fact, PipelineConfig};
use crate::math::cosine_similarity;
use std::path::Path;

/// Formats arbitrary text into a lowercase, alphanumeric slug joined by single underscores.
pub fn slugify_title(title: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('_');
            last_dash = true;
        }
    }
    let trimmed = slug.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "rule".to_string()
    } else {
        trimmed
    }
}

/// Derives a 65-character word-boundary capped slug from a raw slug option or fallback text.
pub fn derive_slug(raw_slug: Option<&str>, fallback_text: &str) -> String {
    let raw = raw_slug.unwrap_or("").trim();
    let text_to_slug = if raw.is_empty() { fallback_text } else { raw };
    crate::vault::organization::slugify_title(text_to_slug, 65)
}

/// Computes the relative vault path for a wisdom rule based on its target pattern.
pub fn resolve_rule_path(scope: &str, target_pattern: &str) -> String {
    let slug = slugify_title(target_pattern);
    format!("wisdom/{}/rule_{}.md", scope, slug)
}

/// Greedy Cosine Clustering (CTO Mandate: Zero Centroid Vector Math)
/// Groups unassociated facts into topically coherent clusters when pairwise cosine similarity >= threshold.
pub fn cluster_facts(
    facts: &[Fact],
    embeddings: &[Vec<f32>],
    config: &PipelineConfig,
) -> Vec<Vec<usize>> {
    let n = facts.len();
    if n < config.cluster_min_size || embeddings.len() != n {
        return Vec::new();
    }

    let mut assigned = vec![false; n];
    let mut clusters = Vec::new();

    for i in 0..n {
        if assigned[i] {
            continue;
        }

        let mut current_cluster = vec![i];
        let emb_i_valid = embeddings[i].iter().any(|v| *v != 0.0);
        let s_i = format!(
            "{} {} {}",
            facts[i].h_n().unwrap_or(""),
            facts[i].iota_n().unwrap_or(""),
            facts[i].artifact_refs.join(" ")
        )
        .to_lowercase();
        let tokens_i: std::collections::HashSet<&str> =
            s_i.split_whitespace().filter(|w| w.len() > 3).collect();

        for j in (i + 1)..n {
            if assigned[j] {
                continue;
            }
            let emb_j_valid = embeddings[j].iter().any(|v| *v != 0.0);
            let sim = if emb_i_valid && emb_j_valid {
                cosine_similarity(&embeddings[i], &embeddings[j])
            } else {
                let s_j = format!(
                    "{} {} {}",
                    facts[j].h_n().unwrap_or(""),
                    facts[j].iota_n().unwrap_or(""),
                    facts[j].artifact_refs.join(" ")
                )
                .to_lowercase();
                let tokens_j: std::collections::HashSet<&str> =
                    s_j.split_whitespace().filter(|w| w.len() > 3).collect();
                if tokens_i.is_empty() || tokens_j.is_empty() {
                    0.0
                } else {
                    let intersection = tokens_i.intersection(&tokens_j).count();
                    let union = tokens_i.union(&tokens_j).count();
                    if union == 0 {
                        0.0
                    } else {
                        (intersection as f32 / union as f32) * 5.0
                    }
                }
            };

            if sim >= config.cluster_similarity {
                current_cluster.push(j);
            }
        }

        if current_cluster.len() >= config.cluster_min_size {
            for &idx in &current_cluster {
                assigned[idx] = true;
            }
            clusters.push(current_cluster);
        }
    }

    clusters
}

/// Reads active short-term memory handoff files to extract anchor skill identifiers.
pub fn get_active_stm_anchors(vault_root: &Path) -> Vec<String> {
    let handoffs_dir = vault_root.join(".handoffs");
    if !handoffs_dir.exists() {
        return Vec::new();
    }
    let mut anchors = Vec::new();
    if let Ok(entries) = std::fs::read_dir(handoffs_dir) {
        for entry in entries.flatten() {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    let anchor_arr = val
                        .get("anchor_skills")
                        .or_else(|| val.get("_active_anchors"))
                        .and_then(|s| s.as_array());
                    if let Some(a) = anchor_arr {
                        for item in a {
                            if let Some(str_val) = item.as_str() {
                                anchors.push(str_val.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    anchors
}
