use anyhow::Result;
use mythrax_core::cognitive::synthesis::truncate_by_tokens;

#[test]
fn test_token_bounds_and_truncation_heuristics() {
    let long_text = "a".repeat(10000);
    let truncated_heuristic = truncate_by_tokens(&long_text, 2048, None);
    assert_eq!(truncated_heuristic.len(), 8192);
    assert_eq!(truncated_heuristic, "a".repeat(8192));

    let short_text = "hello";
    let truncated_short = truncate_by_tokens(short_text, 2048, None);
    assert_eq!(truncated_short, "hello");
}

#[tokio::test]
async fn test_synthesis_prompts_request_confidence_and_contradiction() -> Result<()> {
    let file_content = std::fs::read_to_string("src/cognitive/synthesis.rs")?;
    
    assert!(file_content.contains("metacognitive_confidence"));
    assert!(file_content.contains("node_type"));
    assert!(file_content.contains("rubric"));
    assert!(file_content.contains("contradictory"));
    assert!(file_content.contains("conflict"));
    
    Ok(())
}