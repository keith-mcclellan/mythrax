use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::mcp_routes::truncate_summary;
use mythrax_core::secret_filter::SecretFilter;
use std::sync::Arc;
use tempfile::tempdir;
use axum::{routing::post, Router, response::IntoResponse};
use std::fs;
use std::env;
use mythrax_core::api::{create_router, ApiState};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use mythrax_core::cognitive::meta_skill::MetaSkillSynthesizer;

static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_utf8_boundary_truncation() {
    // 200 characters of Chinese, each character is 3 bytes (total 600 bytes).
    // Let's create a string with 205 Chinese characters, so slicing at 200 bytes would fall in the middle of a character.
    let chinese_char = "中";
    let input = chinese_char.repeat(205);
    
    // Call truncate_summary
    let truncated = truncate_summary(&input);
    
    // It should not panic, and since it is > 200 chars, it should be truncated to exactly 200 chars plus "..."
    // Let's count characters in the truncated string
    let char_count = truncated.chars().count();
    // 200 characters plus 3 characters for "..." = 203 characters
    assert_eq!(char_count, 203);
    assert!(truncated.ends_with("..."));
}

#[test]
fn test_secret_filter_no_panic_on_mismatch() {
    // 1. Unmatched quotes
    let unmatched = "password = \"secret";
    let cleaned_unmatched = SecretFilter::clean(unmatched);
    assert_eq!(cleaned_unmatched, "password = \"secret");

    // 2. Secret with multi-byte characters
    let multibyte = "password = \"🔑secret\"";
    let cleaned_multibyte = SecretFilter::clean(multibyte);
    assert!(cleaned_multibyte.contains("[REDACTED]"));
    assert!(!cleaned_multibyte.contains("🔑secret"));
}

#[tokio::test]
async fn test_embed_batch_error_propagation() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    // Temporarily clear MYTHRAX_TEST_MOCK if it's set, to force an error (no embedder loaded)
    let original_mock = std::env::var("MYTHRAX_TEST_MOCK");
    unsafe {
        std::env::remove_var("MYTHRAX_TEST_MOCK");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let result = backend.embed_batch(&["test".to_string()]).await;

    // Restore MYTHRAX_TEST_MOCK
    if let Ok(ref val) = original_mock {
        unsafe {
            std::env::set_var("MYTHRAX_TEST_MOCK", val);
        }
    }

    assert!(result.is_err());
    let err_msg = format!("{:?}", result.err().unwrap());
    assert!(err_msg.contains("No embedding model loaded"));

    Ok(())
}

#[test]
fn test_retry_jitter_distribution() {
    let mut jitters = Vec::new();
    for attempt in 1..=5 {
        for i in 0..200 {
            let ns = 1718000000000000000 + i * 987654321;
            let jitter = mythrax_core::llm::calculate_lcg_jitter(attempt, ns);
            assert!(jitter >= 0.0 && jitter < 100.0, "Jitter out of range: {}", jitter);
            jitters.push(jitter);
        }
    }
    
    let mut unique_values: std::collections::HashSet<i32> = std::collections::HashSet::new();
    for j in &jitters {
        unique_values.insert(*j as i32);
    }
    assert!(unique_values.len() >= 70, "Entropy too low: got only {} unique values out of 1000", unique_values.len());
}

#[tokio::test]
async fn test_completions_proxy_passthrough() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let mock_app = Router::new().route(
        "/v1/chat/completions",
        post(|axum::Json(payload): axum::Json<serde_json::Value>| async move {
            let is_stream = payload.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
            if is_stream {
                let stream = futures_util::stream::iter(vec![
                    Ok::<_, std::io::Error>(bytes::Bytes::from("data: {\"choices\": [{\"delta\": {\"content\": \"Hello stream\"}}]}\n\n")),
                    Ok::<_, std::io::Error>(bytes::Bytes::from("data: [DONE]\n\n")),
                ]);
                let mut header_map = axum::http::HeaderMap::new();
                header_map.insert(
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("text/event-stream"),
                );
                (axum::http::StatusCode::OK, header_map, axum::body::Body::from_stream(stream)).into_response()
            } else {
                (
                    axum::http::StatusCode::OK,
                    axum::Json(serde_json::json!({
                        "choices": [{
                            "message": {
                                "content": "Hello world from mock LLM!"
                            }
                        }]
                    })),
                ).into_response()
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    
    let mock_server_handle = if let Ok(l) = listener {
        let handle = tokio::spawn(async move {
            let _ = axum::serve(l, mock_app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await;
        });
        Some((handle, shutdown_tx))
    } else {
        println!("WARNING: Could not bind mock server to 127.0.0.1:8080. Skipping local mock check.");
        None
    };

    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let temp = tempdir()?;
    let store = Arc::new(MarkdownStore::new(temp.path())?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    let state = Arc::new(ApiState {
        backend,
        auth_token: "test-token".to_string(),
        store,
        ignore_list,
        dream_tx: None,
    });

    let app = create_router(state);

    if mock_server_handle.is_some() {
        use tower::ServiceExt;
        
        let request_body = serde_json::json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": false
        });

        let response = app.clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("X-Mythrax-Token", "test-token")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&request_body)?))?
            )
            .await?;

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        
        let body_bytes = axum::body::to_bytes(response.into_body(), 10000).await?;
        let res_val: serde_json::Value = serde_json::from_slice(&body_bytes)?;
        let content = res_val["choices"][0]["message"]["content"].as_str().unwrap();
        
        assert_eq!(content, "Hello world from mock LLM!");
        assert!(!content.contains("Execution Check:"));

        let request_body_stream = serde_json::json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true
        });

        let response_stream = app.clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("X-Mythrax-Token", "test-token")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&request_body_stream)?))?
            )
            .await?;

        assert_eq!(response_stream.status(), axum::http::StatusCode::OK);
        
        let body_bytes_stream = axum::body::to_bytes(response_stream.into_body(), 10000).await?;
        let stream_str = String::from_utf8(body_bytes_stream.to_vec())?;
        
        assert!(stream_str.contains("Hello stream"));
        assert!(!stream_str.contains("Execution Check:"));
    }

    if let Some((handle, shutdown_tx)) = mock_server_handle {
        let _ = shutdown_tx.send(());
        let _ = handle.await;
    }

    Ok(())
}

#[tokio::test]
async fn test_meta_skill_malformed_llm_json() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        env::set_var("MYTHRAX_MOCK_LLM", "true");
        env::set_var("MYTHRAX_MOCK_MALFORMED_MERGE", "true");
    }

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    if backend.embed("test").await.is_err() {
        println!("Skipping test_meta_skill_malformed_llm_json: model files not present in ~/.mythrax/models/");
        unsafe {
            env::remove_var("MYTHRAX_MOCK_MALFORMED_MERGE");
        }
        return Ok(());
    }

    let original_home = env::var("HOME").ok();
    unsafe { env::set_var("HOME", tmp.path()); }

    let store = MarkdownStore::new(&vault_root)?;

    let skills_dir = vault_root.join("../.agents/skills");
    let sk1_dir = skills_dir.join("meta-git-commit");
    let sk2_dir = skills_dir.join("meta-git-pull");
    fs::create_dir_all(&sk1_dir)?;
    fs::create_dir_all(&sk2_dir)?;

    let sk1_content = "---\nname: meta-git-commit\ndescription: git workflow management instructions\ngenerator_name: MetaSkillSynthesizer\n---\nbody";
    let sk2_content = "---\nname: meta-git-pull\ndescription: git workflow management instructions\ngenerator_name: MetaSkillSynthesizer\n---\nbody";

    fs::write(sk1_dir.join("SKILL.md"), sk1_content)?;
    fs::write(sk2_dir.join("SKILL.md"), sk2_content)?;

    let synthesizer = MetaSkillSynthesizer::new();
    let suggestions = synthesizer.detect_skill_merges(&backend, &store).await?;

    assert!(!suggestions.is_empty());
    assert_eq!(suggestions[0]["suggested_target_name"], serde_json::Value::Null);

    let suggestions_file = vault_root.join("wiki/skill_merge_suggestions.md");
    assert!(suggestions_file.exists());
    let suggestions_content = fs::read_to_string(suggestions_file)?;
    
    assert!(suggestions_content.contains("Unknown Target"));
    assert!(suggestions_content.contains("No reason provided."));

    unsafe {
        env::remove_var("MYTHRAX_MOCK_MALFORMED_MERGE");
        if let Some(h) = original_home {
            env::set_var("HOME", h);
        } else {
            env::remove_var("HOME");
        }
    }

    Ok(())
}
