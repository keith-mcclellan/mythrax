use mythrax_core::bench::metrics::{evaluate_retrieval, ndcg};

#[test]
fn recall_any_true_when_one_gold_in_topk() {
    // corpus ids c0..c4, rankings put c3 (a gold) at rank 2 (within k=5)
    let corpus = vec!["c0","c1","c2","c3","c4"].iter().map(|s|s.to_string()).collect::<Vec<_>>();
    let rankings = vec![0usize,3,1,2,4];        // index order into corpus
    let gold = vec!["c3".to_string(), "c9_missing".to_string()];
    let s = evaluate_retrieval(&rankings, &gold, &corpus, 5);
    assert_eq!(s.recall_any, 1.0);              // at least one gold present
    assert_eq!(s.recall_all, 0.0);              // not ALL golds present (c9_missing absent)
}

#[test]
fn recall_all_requires_every_gold_in_topk() {
    let corpus = vec!["c0","c1","c2","c3","c4"].iter().map(|s|s.to_string()).collect::<Vec<_>>();
    let rankings = vec![3usize,1,0,2,4];
    let gold = vec!["c3".to_string(), "c1".to_string()];
    let s = evaluate_retrieval(&rankings, &gold, &corpus, 5);
    assert_eq!(s.recall_all, 1.0);
}

#[test]
fn k_cutoff_excludes_gold_beyond_k() {
    let corpus = vec!["c0","c1","c2","c3","c4"].iter().map(|s|s.to_string()).collect::<Vec<_>>();
    let rankings = vec![0usize,1,2,4,3];        // gold c3 is at rank 5 (index 4) -> outside k=4
    let gold = vec!["c3".to_string()];
    assert_eq!(evaluate_retrieval(&rankings,&gold,&corpus,4).recall_any, 0.0);
    assert_eq!(evaluate_retrieval(&rankings,&gold,&corpus,5).recall_any, 1.0);
}

#[test]
fn ndcg_rewards_higher_rank() {
    let corpus = vec!["c0","c1"].iter().map(|s|s.to_string()).collect::<Vec<_>>();
    let gold = vec!["c1".to_string()];
    let high = ndcg(&vec![1usize,0], &gold, &corpus, 2);   // gold first
    let low  = ndcg(&vec![0usize,1], &gold, &corpus, 2);   // gold second
    assert!(high > low);
}
