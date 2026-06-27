pub struct RetrievalScore {
    pub recall_any: f32,
    pub recall_all: f32,
    pub ndcg: f32,
}

pub fn evaluate_retrieval(
    rankings: &[usize],
    correct_ids: &[String],
    corpus_ids: &[String],
    k: usize,
) -> RetrievalScore {
    let k = std::cmp::min(k, rankings.len());
    if k == 0 || correct_ids.is_empty() || corpus_ids.is_empty() {
        return RetrievalScore {
            recall_any: 0.0,
            recall_all: 0.0,
            ndcg: 0.0,
        };
    }

    // 1. Get the top-k retrieved corpus IDs
    let top_k_ids: std::collections::HashSet<String> = rankings[..k]
        .iter()
        .filter_map(|&idx| corpus_ids.get(idx))
        .cloned()
        .collect();

    // 2. Compute recall_any (1.0 if ANY correct_id is in top_k_ids)
    let recall_any = if correct_ids.iter().any(|id| top_k_ids.contains(id)) {
        1.0
    } else {
        0.0
    };

    // 3. Compute recall_all (1.0 if ALL correct_ids are in top_k_ids)
    let recall_all = if correct_ids.iter().all(|id| top_k_ids.contains(id)) {
        1.0
    } else {
        0.0
    };

    // 4. Compute nDCG@k
    let ndcg_val = ndcg(rankings, correct_ids, corpus_ids, k);

    RetrievalScore {
        recall_any,
        recall_all,
        ndcg: ndcg_val,
    }
}

pub fn ndcg(
    rankings: &[usize],
    correct_ids: &[String],
    corpus_ids: &[String],
    k: usize,
) -> f32 {
    let k = std::cmp::min(k, rankings.len());
    if k == 0 || correct_ids.is_empty() {
        return 0.0;
    }

    // Compute DCG@k
    let mut dcg = 0.0f32;
    let correct_set: std::collections::HashSet<&String> = correct_ids.iter().collect();

    for i in 0..k {
        let idx = rankings[i];
        if let Some(corpus_id) = corpus_ids.get(idx) {
            if correct_set.contains(corpus_id) {
                let rel = 1.0f32;
                let discount = ((i + 2) as f32).log2();
                dcg += rel / discount;
            }
        }
    }

    // Compute IDCG@k (Ideal DCG)
    // The ideal case is where all relevant documents are ranked at the very top.
    // Total number of relevant documents is correct_ids.len()
    let num_relevant = correct_ids.len();
    let mut idcg = 0.0f32;
    let ideal_limit = std::cmp::min(num_relevant, k);

    for i in 0..ideal_limit {
        let discount = ((i + 2) as f32).log2();
        idcg += 1.0f32 / discount;
    }

    if idcg == 0.0 {
        0.0
    } else {
        dcg / idcg
    }
}

pub fn session_id_from_corpus_id(id: &str) -> &str {
    if let Some(idx) = id.rfind("_turn_") {
        &id[..idx]
    } else {
        id
    }
}
