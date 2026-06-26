use mythrax_core::retrieval::bm25::{OkapiBM25, tokenize};

#[test]
fn test_tokenize_lowercase_and_punctuation() {
    let text = "Hello, world! This is a Rust-based BM25 tokenizer.";
    let tokens = tokenize(text);
    assert!(tokens.contains(&"hello".to_string()));
    assert!(tokens.contains(&"world".to_string()));
    assert!(tokens.contains(&"rust-based".to_string()) || tokens.contains(&"rust".to_string()));
    assert!(!tokens.contains(&"hello,".to_string()));
}

#[test]
fn test_bm25_scoring_ranking() {
    let docs = vec![
        "the quick brown fox jumps over the lazy dog".to_string(),
        "rusty metal pipes in the old basement".to_string(),
        "rust programming language and agentic systems".to_string(),
    ];
    let corpus = docs.into_iter().enumerate().map(|(i, content)| (i.to_string(), content)).collect::<Vec<_>>();
    
    let bm25 = OkapiBM25::new(&corpus);
    
    // Query: "rust language"
    let scores = bm25.score("rust language");
    
    // The third document should have the highest score since it has both "rust" and "language"
    let doc_idx_2 = "2".to_string();
    let score_2 = scores.iter().find(|(id, _)| id == &doc_idx_2).map(|(_, s)| *s).unwrap_or(0.0);
    
    let doc_idx_0 = "0".to_string();
    let score_0 = scores.iter().find(|(id, _)| id == &doc_idx_0).map(|(_, s)| *s).unwrap_or(0.0);
    
    assert!(score_2 > score_0, "Doc 2 score ({}) should be higher than Doc 0 score ({})", score_2, score_0);
}

#[test]
fn test_bm25_min_max_normalization() {
    let corpus = vec![
        ("1".to_string(), "query match term here".to_string()),
        ("2".to_string(), "no match word".to_string()),
    ];
    let bm25 = OkapiBM25::new(&corpus);
    let scores = bm25.score_normalized("query match");
    
    let norm_1 = scores.iter().find(|(id, _)| id == "1").map(|(_, s)| *s).unwrap_or(0.0);
    let norm_2 = scores.iter().find(|(id, _)| id == "2").map(|(_, s)| *s).unwrap_or(0.0);
    
    assert!((norm_1 - 1.0).abs() < 1e-5, "Highest score must normalize to 1.0, got {}", norm_1);
    assert!((norm_2 - 0.0).abs() < 1e-5, "Lowest score must normalize to 0.0, got {}", norm_2);
}
