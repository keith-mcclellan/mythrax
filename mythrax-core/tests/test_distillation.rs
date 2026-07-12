use tempfile::tempdir;
use mythrax_core::embeddings::*;
use mythrax_core::vault::distillation::*;
use mythrax_core::db::StorageBackend;
use mythrax_core::db::backend::SurrealBackend;
use mythrax_core::contracts::{ForgedSectionBatch, ForgedConcept, WisdomRule, Tier};

#[tokio::test]
async fn test_embedding_cache_incremental_persistence() {
    let tmp = tempdir().unwrap();
    let cache_path = tmp.path().join("emb_cache.bin");
    
    // Clear and set capacity
    clear_embedding_cache();
    unsafe {
        std::env::set_var("MYTHRAX_EMBEDDING_CACHE_CAPACITY", "5");
    }
    
    // Check initial state
    assert_eq!(get_embedding_cache_len(), 0);
    
    // Cache some entries
    cache_embedding("text1".to_string(), vec![1.0, 2.0, 3.0]);
    cache_embedding("text2".to_string(), vec![4.0, 5.0, 6.0]);
    
    assert_eq!(get_embedding_cache_len(), 2);
    
    // Flush to disk
    set_embedding_cache_path(&cache_path);
    flush_dirty(&cache_path).unwrap();
    assert!(cache_path.exists());
    
    // Clear and verify empty
    clear_embedding_cache();
    assert_eq!(get_embedding_cache_len(), 0);
    
    // Load and verify
    load_embedding_cache_from_disk(&cache_path).unwrap();
    assert_eq!(get_embedding_cache_len(), 2);
    assert_eq!(get_cached_embedding("text1").unwrap(), vec![1.0, 2.0, 3.0]);
    assert_eq!(get_cached_embedding("text2").unwrap(), vec![4.0, 5.0, 6.0]);
}

#[test]
fn test_transcript_chunking_boundaries() {
    let mut steps = Vec::new();
    
    // Add 5 normal steps
    for i in 0..5 {
        steps.push(TranscriptStep {
            step_index: i,
            source: "user".to_string(),
            r#type: "USER_INPUT".to_string(),
            status: "DONE".to_string(),
            created_at: "2026-07-12T12:00:00Z".to_string(),
            content: Some("User query".to_string()),
            tool_calls: None,
        });
    }
    
    // Add step with tool calls (non-edit)
    steps.push(TranscriptStep {
        step_index: 5,
        source: "agent".to_string(),
        r#type: "TOOL_CALL".to_string(),
        status: "DONE".to_string(),
        created_at: "2026-07-12T12:00:05Z".to_string(),
        content: None,
        tool_calls: Some(vec![ToolCall {
            name: "grep_search".to_string(),
            args: serde_json::json!({"SearchPath": "/path", "Query": "test"}),
        }]),
    });
    
    // Add step with edit tool calls
    steps.push(TranscriptStep {
        step_index: 6,
        source: "agent".to_string(),
        r#type: "TOOL_CALL".to_string(),
        status: "DONE".to_string(),
        created_at: "2026-07-12T12:00:10Z".to_string(),
        content: None,
        tool_calls: Some(vec![ToolCall {
            name: "replace_file_content".to_string(),
            args: serde_json::json!({"TargetFile": "src/main.rs"}),
        }]),
    });
    
    let chunks = chunk_transcript(&steps);
    // Should split because step 5 is tool call (non-edit) vs previous user inputs
    // And step 6 is edit tool call vs step 5 non-edit tool call
    assert!(chunks.len() >= 2);
}

#[tokio::test]
async fn test_wisdom_seeding_cosine_similarity() {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
    }
    let backend = SurrealBackend::new("mem://").await.unwrap();
    backend.init().await.unwrap();
    
    // Use 768 dimensions for index compliance
    let mut rule1_emb = vec![0.0; 768];
    rule1_emb[0] = 1.0;
    
    let rule1 = WisdomRule {
        id: None,
        target_pattern: "Avoid Hardcoded API Keys".to_string(),
        action_to_avoid: "Hardcoding secrets in source files".to_string(),
        causal_explanation: "Security risk".to_string(),
        prescribed_remedy: "Use config files or env variables".to_string(),
        tier: Tier::Wisdom,
        scope: "general".to_string(),
        embedding: Some(rule1_emb),
        generator_name: "TestGenerator".to_string(),
        ..Default::default()
    };
    
    backend.save_wisdom_rule(&rule1).await.unwrap();
    
    // Create rule2 which is similar (sim >= 0.85)
    let mut rule2_emb = vec![0.0; 768];
    rule2_emb[0] = 0.95;
    rule2_emb[1] = 0.05;
    
    let mut is_duplicate = false;
    let existing_rules = backend.get_all_wisdom_rules().await.unwrap();
    
    // Calculate cosine similarity manually in test
    let dot: f32 = rule2_emb.iter().zip(existing_rules[0].embedding.as_ref().unwrap().iter()).map(|(a, b)| a * b).sum();
    let norm_a = (rule2_emb.iter().map(|x| x * x).sum::<f32>()).sqrt();
    let norm_b = (existing_rules[0].embedding.as_ref().unwrap().iter().map(|x| x * x).sum::<f32>()).sqrt();
    let sim = dot / (norm_a * norm_b);
    
    if sim >= 0.85 {
        is_duplicate = true;
    }
    
    assert!(is_duplicate);
}

#[tokio::test]
async fn test_forge_pipeline_deduplication() {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
    }
    let backend = SurrealBackend::new("mem://").await.unwrap();
    backend.init().await.unwrap();
    
    let batch = ForgedSectionBatch {
        scope: "general".to_string(),
        chunk_text: "Implementation of the new user dashboard page layout".to_string(),
        chunk_index: 0,
        doc_title: "Test Doc".to_string(),
        concepts: vec![ForgedConcept {
            name: "UserDashboard".to_string(),
            content: "Dashboard implementation".to_string(),
        }],
        rules: vec![],
    };
    
    // First save
    backend.save_forged_section_db(&batch).await.unwrap();
    
    // Let's verify we have a record in forged_section_hash
    let check_query = "SELECT VALUE hash FROM forged_section_hash LIMIT 1;";
    let mut resp = backend.db.query(check_query).await.unwrap();
    let hash: Option<String> = resp.take(0).unwrap();
    assert!(hash.is_some());
    
    // Second save with same batch should skip/not fail
    backend.save_forged_section_db(&batch).await.unwrap();
}
