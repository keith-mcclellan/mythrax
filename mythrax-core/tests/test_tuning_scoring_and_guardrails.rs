// Test-First Unit Tests for Parameter Tuning, Scoring, and Guardrails.
// Implements math, scoring formulas, and veto checks in Rust.

struct DummyRecord {
    pub retrieved_corpus_ids: Vec<String>,
    pub gold_corpus_ids: Vec<String>,
    pub recall_any_turn_at5: f32,
    pub recall_all_turn_at5: f32,
    pub ndcg_turn_at10: f32,
    pub category: String,
}

fn compute_coarse_score(records: &[DummyRecord]) -> f32 {
    if records.is_empty() {
        return 0.0;
    }
    let sum_ndcg: f32 = records.iter().map(|r| r.ndcg_turn_at10).sum();
    let sum_r_all: f32 = records.iter().map(|r| r.recall_all_turn_at5).sum();
    let sum_r_any: f32 = records.iter().map(|r| r.recall_any_turn_at5).sum();

    let count = records.len() as f32;
    let avg_ndcg = sum_ndcg / count;
    let avg_r_all = sum_r_all / count;
    let avg_r_any = sum_r_any / count;

    0.50 * avg_ndcg + 0.40 * avg_r_all + 0.10 * avg_r_any
}

fn compute_ndcg_at_k(retrieved: &[String], gold: &[String], k: usize) -> f32 {
    let limit = std::cmp::min(retrieved.len(), k);
    if limit == 0 || gold.is_empty() {
        return 0.0;
    }

    let mut dcg = 0.0;
    for i in 0..limit {
        if gold.contains(&retrieved[i]) {
            dcg += 1.0 / ((i + 2) as f64).log2();
        }
    }

    let mut idcg = 0.0;
    let ideal_limit = std::cmp::min(gold.len(), k);
    for i in 0..ideal_limit {
        idcg += 1.0 / ((i + 2) as f64).log2();
    }

    if idcg > 0.0 { (dcg / idcg) as f32 } else { 0.0 }
}

fn compute_mrr_penalty_recall3(retrieved: &[String], gold: &[String]) -> (f32, f32, f32) {
    let mut first_rank = None;
    for (i, item) in retrieved.iter().enumerate() {
        if gold.contains(item) {
            first_rank = Some(i + 1);
            break;
        }
    }

    let mrr = if let Some(r) = first_rank {
        1.0 / r as f32
    } else {
        0.0
    };

    let recall3 = if let Some(r) = first_rank {
        if r <= 3 { 1.0 } else { 0.0 }
    } else {
        0.0
    };

    let penalty = if let Some(r) = first_rank {
        if r >= 5 { 1.0 } else { 0.0 }
    } else {
        1.0
    };

    (mrr, penalty, recall3)
}

struct FineScoreMetrics {
    pub fine_score: f32,
    pub avg_ndcg_at3: f32,
    pub avg_mrr: f32,
    pub avg_recall_at3: f32,
    pub avg_penalty: f32,
}

fn compute_fine_score_for_category(
    records: &[DummyRecord],
    category: &str,
) -> Option<FineScoreMetrics> {
    let cat_records: Vec<&DummyRecord> =
        records.iter().filter(|r| r.category == category).collect();
    if cat_records.is_empty() {
        return None;
    }

    let mut sum_ndcg3 = 0.0;
    let mut sum_mrr = 0.0;
    let mut sum_recall3 = 0.0;
    let mut sum_penalty = 0.0;

    for r in &cat_records {
        sum_ndcg3 += compute_ndcg_at_k(&r.retrieved_corpus_ids, &r.gold_corpus_ids, 3);
        let (mrr, penalty, recall3) =
            compute_mrr_penalty_recall3(&r.retrieved_corpus_ids, &r.gold_corpus_ids);
        sum_mrr += mrr;
        sum_penalty += penalty;
        sum_recall3 += recall3;
    }

    let count = cat_records.len() as f32;
    let avg_ndcg_at3 = sum_ndcg3 / count;
    let avg_mrr = sum_mrr / count;
    let avg_recall_at3 = sum_recall3 / count;
    let avg_penalty = sum_penalty / count;

    let fine_score =
        0.50 * avg_ndcg_at3 + 0.30 * avg_mrr + 0.20 * avg_recall_at3 - 0.10 * avg_penalty;

    Some(FineScoreMetrics {
        fine_score,
        avg_ndcg_at3,
        avg_mrr,
        avg_recall_at3,
        avg_penalty,
    })
}

fn is_global_recall_vetoed(baseline_r_any: f32, actual_r_any: f32) -> bool {
    baseline_r_any - actual_r_any > 0.02
}

fn is_category_recall_vetoed(baseline_cat_recall3: f32, actual_cat_recall3: f32) -> bool {
    baseline_cat_recall3 - actual_cat_recall3 > 0.05
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coarse_score_calculation() {
        // Coarse_Score = 0.50 * nDCG@10 + 0.40 * Recall_All@5 + 0.10 * Recall_Any@5
        let records = vec![
            DummyRecord {
                retrieved_corpus_ids: vec![],
                gold_corpus_ids: vec![],
                recall_any_turn_at5: 1.0,
                recall_all_turn_at5: 0.0,
                ndcg_turn_at10: 0.60,
                category: "Temporal".to_string(),
            },
            DummyRecord {
                retrieved_corpus_ids: vec![],
                gold_corpus_ids: vec![],
                recall_any_turn_at5: 1.0,
                recall_all_turn_at5: 1.0,
                ndcg_turn_at10: 0.80,
                category: "Temporal".to_string(),
            },
        ];

        // Averages:
        // avg_ndcg = (0.60 + 0.80) / 2 = 0.70
        // avg_r_all = (0.0 + 1.0) / 2 = 0.50
        // avg_r_any = (1.0 + 1.0) / 2 = 1.00
        // Expected score = 0.50 * 0.70 + 0.40 * 0.50 + 0.10 * 1.00 = 0.35 + 0.20 + 0.10 = 0.65
        let score = compute_coarse_score(&records);
        assert!(
            (score - 0.65).abs() < 1e-5,
            "Expected coarse score 0.65, got {}",
            score
        );
    }

    #[test]
    fn test_fine_score_and_metrics() {
        // Test query-level calculations for metrics
        // Query 1: correct at rank 1.
        let ret1 = vec!["doc1".to_string(), "doc2".to_string(), "doc3".to_string()];
        let gold1 = vec!["doc1".to_string()];
        let (mrr1, penalty1, recall3_1) = compute_mrr_penalty_recall3(&ret1, &gold1);
        assert_eq!(mrr1, 1.0);
        assert_eq!(penalty1, 0.0);
        assert_eq!(recall3_1, 1.0);

        let ndcg3_1 = compute_ndcg_at_k(&ret1, &gold1, 3);
        // DCG@3 = 1.0 / log2(2) = 1.0. IDCG@3 = 1.0 / log2(2) = 1.0. nDCG@3 = 1.0
        assert_eq!(ndcg3_1, 1.0);

        // Query 2: correct at rank 5 (penalized).
        let ret2 = vec![
            "doc_noise1".to_string(),
            "doc_noise2".to_string(),
            "doc_noise3".to_string(),
            "doc_noise4".to_string(),
            "doc2".to_string(),
        ];
        let gold2 = vec!["doc2".to_string()];
        let (mrr2, penalty2, recall3_2) = compute_mrr_penalty_recall3(&ret2, &gold2);
        assert_eq!(mrr2, 0.2); // rank 5 -> 1/5
        assert_eq!(penalty2, 1.0); // rank 5 is >= 5
        assert_eq!(recall3_2, 0.0); // not in top 3

        let ndcg3_2 = compute_ndcg_at_k(&ret2, &gold2, 3);
        assert_eq!(ndcg3_2, 0.0); // not in top 3

        // Group together and evaluate category fine score
        let records = vec![
            DummyRecord {
                retrieved_corpus_ids: ret1,
                gold_corpus_ids: gold1,
                recall_any_turn_at5: 1.0,
                recall_all_turn_at5: 1.0,
                ndcg_turn_at10: 1.0,
                category: "Preference".to_string(),
            },
            DummyRecord {
                retrieved_corpus_ids: ret2,
                gold_corpus_ids: gold2,
                recall_any_turn_at5: 1.0,
                recall_all_turn_at5: 1.0,
                ndcg_turn_at10: 0.5,
                category: "Preference".to_string(),
            },
        ];

        let metrics = compute_fine_score_for_category(&records, "Preference").unwrap();
        // Averages for Preference:
        // avg_ndcg_at3 = (1.0 + 0.0) / 2 = 0.50
        // avg_mrr = (1.0 + 0.2) / 2 = 0.60
        // avg_recall_at3 = (1.0 + 0.0) / 2 = 0.50
        // avg_penalty = (0.0 + 1.0) / 2 = 0.50
        // Expected Fine Score = 0.50 * 0.50 + 0.30 * 0.60 + 0.20 * 0.50 - 0.10 * 0.50
        //                     = 0.25 + 0.18 + 0.10 - 0.05 = 0.48
        assert_eq!(metrics.avg_ndcg_at3, 0.50);
        assert_eq!(metrics.avg_mrr, 0.60);
        assert_eq!(metrics.avg_recall_at3, 0.50);
        assert_eq!(metrics.avg_penalty, 0.50);
        assert!(
            (metrics.fine_score - 0.48).abs() < 1e-5,
            "Expected 0.48, got {}",
            metrics.fine_score
        );
    }

    #[test]
    fn test_global_recall_veto() {
        let baseline = 0.85;
        let actual_ok = 0.84; // drop = 0.01 <= 0.02
        let actual_veto = 0.82; // drop = 0.03 > 0.02

        assert!(!is_global_recall_vetoed(baseline, actual_ok));
        assert!(is_global_recall_vetoed(baseline, actual_veto));
    }

    #[test]
    fn test_category_recall_veto() {
        let baseline = 0.90;
        let actual_ok = 0.86; // drop = 0.04 <= 0.05
        let actual_veto = 0.84; // drop = 0.06 > 0.05

        assert!(!is_category_recall_vetoed(baseline, actual_ok));
        assert!(is_category_recall_vetoed(baseline, actual_veto));
    }
}
