use anyhow::Result;
use std::sync::Arc;
use tempfile::tempdir;
use mythrax_core::store::{set_workspace_root, get_workspace_root, find_vault_root, clear_workspace_root};
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::LlmConfigRequest;
use mythrax_core::llm::LLMClient;
use axum::{routing::post, Router};

static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_thread_safe_workspace_root_context() {
    let _guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let temp = tempdir().unwrap();
    let custom_path = temp.path().join("custom_workspace");
    
    set_workspace_root(custom_path.clone());
    assert_eq!(get_workspace_root(), Some(custom_path.clone()));
    
    // Verify find_vault_root uses workspace root
    let orig_vault = std::env::var("MYTHRAX_VAULT_ROOT");
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
    }
    let orig_workspace = std::env::var("MYTHRAX_WORKSPACE_ROOT");
    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
    }
    
    let root = find_vault_root();
    assert_eq!(root, custom_path);
    
    // Restore environment variables
    if let Ok(val) = orig_vault {
        unsafe {
            std::env::set_var("MYTHRAX_VAULT_ROOT", val);
        }
    }
    if let Ok(val) = orig_workspace {
        unsafe {
            std::env::set_var("MYTHRAX_WORKSPACE_ROOT", val);
        }
    }

    // Clean up
    clear_workspace_root();
}

#[tokio::test]
async fn test_inference_delay_configurable() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    
    // Start mock LLM completions HTTP server on 127.0.0.1:8088
    let mock_app = Router::new().route(
        "/v1/chat/completions",
        post(|| async {
            axum::Json(serde_json::json!({
                "choices": [{
                    "message": {
                        "content": "Mock completion response"
                    }
                }]
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8088").await?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let mock_server_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, mock_app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    // Save custom post-inference delay of 250ms via update_llm_config
    let req = LlmConfigRequest {
        provider: "local".to_string(),
        duration: None,
        model: Some("mlx-community/Qwen3.6-35B-A3B-4bit".to_string()),
        cloud_provider: Some("gemini".to_string()),
        api_key: None,
        llm_post_inference_delay_ms: Some(250),
    };
    backend.update_llm_config(&req).await?;

    // Assert that it's retrieved by get_llm_config
    let config = backend.get_llm_config().await?;
    assert_eq!(config.llm_post_inference_delay_ms, Some(250));

    // Clear MYTHRAX_MOCK_LLM if set
    let orig_mock_llm = std::env::var("MYTHRAX_MOCK_LLM");
    unsafe {
        std::env::remove_var("MYTHRAX_MOCK_LLM");
    }

    // Set MYTHRAX_COMPLETIONS_URL
    let orig_url = std::env::var("MYTHRAX_COMPLETIONS_URL");
    unsafe {
        std::env::set_var("MYTHRAX_COMPLETIONS_URL", "http://127.0.0.1:8088/v1/chat/completions");
    }

    let client = LLMClient::new();
    let start = std::time::Instant::now();
    
    // Call completions to verify it respects this value
    let _resp = client.completion(&*backend, Some("system"), "prompt").await?;
    let elapsed = start.elapsed();

    // Restore env
    if let Ok(val) = orig_mock_llm {
        unsafe {
            std::env::set_var("MYTHRAX_MOCK_LLM", val);
        }
    }
    if let Ok(val) = orig_url {
        unsafe {
            std::env::set_var("MYTHRAX_COMPLETIONS_URL", val);
        }
    } else {
        unsafe {
            std::env::remove_var("MYTHRAX_COMPLETIONS_URL");
        }
    }
    let _ = shutdown_tx.send(());
    let _ = mock_server_handle.await;

    // Since we set 250ms delay, the call should take at least 250ms
    assert!(elapsed >= std::time::Duration::from_millis(250), "Expected inference to be delayed by at least 250ms, took {:?}", elapsed);
    // And it should not take 5 seconds (5000ms), let's say it's under 4 seconds
    assert!(elapsed < std::time::Duration::from_secs(4), "Expected inference to be faster than default 5s, took {:?}", elapsed);

    Ok(())
}

#[tokio::test]
async fn test_graceful_shutdown_channel() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let temp = tempdir()?;
    let vault_path = temp.path().join("vault");
    std::fs::create_dir_all(&vault_path)?;

    let home_dir = temp.path().join("home");
    std::fs::create_dir_all(&home_dir)?;

    let orig_home = std::env::var("HOME").ok();
    unsafe {
        std::env::set_var("HOME", home_dir.to_str().unwrap());
    }

    let orig_db_url = std::env::var("MYTHRAX_DB_URL").ok();
    unsafe {
        std::env::set_var("MYTHRAX_DB_URL", "mem://");
    }

    // Set port for daemon
    let port = 8092;

    let daemon_handle = tokio::spawn(async move {
        let action = mythrax_core::cli::DaemonAction::Run {
            port,
            vault: Some(vault_path.to_str().unwrap().to_string()),
        };
        mythrax_core::daemon::handle_daemon(action).await
    });

    // Wait for the daemon to start (poll up to 5 seconds)
    let mut healthy = false;
    let client = reqwest::Client::new();
    let ping_url = format!("http://127.0.0.1:{}/v1/config/llm", port);
    
    // Need token for auth
    let token_path = home_dir.join(".mythrax/token");
    let mut token = String::new();
    for _ in 0..50 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if token_path.exists() {
            if let Ok(t) = std::fs::read_to_string(&token_path) {
                token = t.trim().to_string();
                break;
            }
        }
    }

    for _ in 0..50 {
        let resp = client.get(&ping_url)
            .header("X-Mythrax-Token", &token)
            .send()
            .await;
        if let Ok(r) = resp {
            if r.status() == reqwest::StatusCode::OK {
                healthy = true;
                break;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    assert!(healthy, "Daemon did not start up within 5 seconds");

    // Check that the PID file is created
    let pid_path = home_dir.join(".mythrax/daemon.pid");
    assert!(pid_path.exists(), "PID file was not created");

    // Trigger stop via the HTTP endpoint
    let stop_url = format!("http://127.0.0.1:{}/v1/daemon/stop", port);
    let resp = client.post(&stop_url)
        .header("X-Mythrax-Token", &token)
        .send()
        .await?;
    
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    // Now wait for the daemon process/task to finish
    let run_result = tokio::time::timeout(std::time::Duration::from_secs(5), daemon_handle).await;
    assert!(run_result.is_ok(), "Daemon did not shut down within 5 seconds");
    
    // Verify PID file is deleted
    assert!(!pid_path.exists(), "PID file was not deleted upon shutdown");

    // Restore env
    if let Some(h) = orig_home {
        unsafe {
            std::env::set_var("HOME", h);
        }
    } else {
        unsafe {
            std::env::remove_var("HOME");
        }
    }
    if let Some(db) = orig_db_url {
        unsafe {
            std::env::set_var("MYTHRAX_DB_URL", db);
        }
    } else {
        unsafe {
            std::env::remove_var("MYTHRAX_DB_URL");
        }
    }

    Ok(())
}
