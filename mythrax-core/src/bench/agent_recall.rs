use serde::Deserialize;
use crate::db::backend::{SurrealBackend, StorageBackend};

#[derive(Deserialize, Debug, Clone)]
pub struct RecallQuery {
    pub query: String,
    pub expected_fragments: Vec<String>,
    #[serde(rename = "type")]
    pub query_type: String,
}

pub struct BenchmarkReport {
    pub total_passed: usize,
    pub total_queries: usize,
    pub overall_score: f32,
    pub scores_by_type: std::collections::HashMap<String, (usize, usize, f32)>,
}

pub async fn run_agent_recall(
    backend: &SurrealBackend,
    queries: &[RecallQuery],
    deep_insight: bool,
    threshold: f32,
    limit: usize,
) -> anyhow::Result<BenchmarkReport> {
    let mut scores_by_type: std::collections::HashMap<String, Vec<bool>> = std::collections::HashMap::new();

    for q in queries {
        let response = backend.search(crate::contracts::SearchParams::from_positional(
        &q.query,
        None,
        deep_insight,
        limit,
        0,
        threshold,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    )).await?;

        // Combine result content to search for fragments
        let combined_results: String = response.results.iter()
            .map(|r| format!("{} {}", r.title, r.content).to_lowercase())
            .collect::<Vec<String>>()
            .join("\n");

        let mut matched_all = true;
        for fragment in &q.expected_fragments {
            let frag_lower = fragment.to_lowercase();
            let is_match = if frag_lower.contains(".*") {
                let parts: Vec<&str> = frag_lower.split(".*").collect();
                parts.iter().all(|part| combined_results.contains(part))
            } else {
                combined_results.contains(&frag_lower)
            };
            if !is_match {
                matched_all = false;
            }
        }

        scores_by_type.entry(q.query_type.clone())
            .or_default()
            .push(matched_all);
    }

    let mut total_passed = 0;
    let mut total_queries = 0;
    let mut report_scores = std::collections::HashMap::new();

    for (q_type, passes) in &scores_by_type {
        let type_passed = passes.iter().filter(|&&p| p).count();
        let type_total = passes.len();
        total_passed += type_passed;
        total_queries += type_total;
        let pct = (type_passed as f32 / type_total as f32) * 100.0;
        report_scores.insert(q_type.clone(), (type_passed, type_total, pct));
    }

    let overall_score = if total_queries > 0 {
        (total_passed as f32 / total_queries as f32) * 100.0
    } else {
        0.0
    };

    Ok(BenchmarkReport {
        total_passed,
        total_queries,
        overall_score,
        scores_by_type: report_scores,
    })
}
