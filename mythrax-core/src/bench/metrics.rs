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
    if k == 0 || correct_ids.is_empty() || corpus_ids.is_empty() {
        return 0.0;
    }

    let get_relevance = |c_id: &str| -> f32 {
        if correct_ids.iter().any(|id| id == c_id) {
            3.0
        } else {
            let c_session = session_id_from_corpus_id(c_id);
            let same_session = correct_ids.iter().any(|id| {
                session_id_from_corpus_id(id) == c_session
            });
            if same_session {
                2.0
            } else {
                0.0
            }
        }
    };

    // Compute DCG@k
    let mut dcg = 0.0f32;
    for i in 0..k {
        let idx = rankings[i];
        if let Some(corpus_id) = corpus_ids.get(idx) {
            let rel = get_relevance(corpus_id);
            let discount = ((i + 2) as f32).log2();
            dcg += rel / discount;
        }
    }

    // Compute IDCG@k (Ideal DCG)
    let mut all_relevances: Vec<f32> = corpus_ids.iter().map(|id| get_relevance(id)).collect();
    all_relevances.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let mut idcg = 0.0f32;
    let ideal_limit = std::cmp::min(all_relevances.len(), k);
    for i in 0..ideal_limit {
        let discount = ((i + 2) as f32).log2();
        idcg += all_relevances[i] / discount;
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

pub fn parse_haystack_date(date_str: &str) -> Option<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"^(\d{4})/(\d{2})/(\d{2})\s+\([A-Za-z]+\)\s+(\d{2}):(\d{2})$").unwrap()
    });
    if let Some(caps) = re.captures(date_str) {
        let year = caps.get(1)?.as_str();
        let month = caps.get(2)?.as_str();
        let day = caps.get(3)?.as_str();
        let hour = caps.get(4)?.as_str();
        let minute = caps.get(5)?.as_str();
        Some(format!("{}-{}-{}T{}:{}:00Z", year, month, day, hour, minute))
    } else {
        None
    }
}
