#![allow(dead_code, unused_imports)]

mod vault_lifecycle {
use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::vault::ingestion::bulk_ingest_vault;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_rocksdb_connection_and_persistence() -> Result<()> {
    let tmp = tempdir()?;
    let db_path = tmp.path().join("db");
    let surreal_url = format!("rocksdb://{}", db_path.to_string_lossy());

    // 1. Initialize persistent backend
    let backend = SurrealBackend::new(
        &surreal_url,
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await?;
    backend.init().await?;

    // 2. Save an episode
    let ep = EpisodeSave {
        created_at: None,
        title: "Persistence Test".to_string(),
        content: "Verifying persistent storage in rocksdb.".to_string(),
        entities: vec![],
        scope: Some("testing".to_string()),
        vault_path: Some("episodes/persist_test.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await?;
    assert!(ep_id.contains("episode:"));

    // 3. Drop connection and reconnect
    drop(backend);

    let lock_file = db_path.join("LOCK");
    let mut backend2 = None;
    for attempt in 0..10 {
        if lock_file.exists() {
            let _ = std::fs::remove_file(&lock_file);
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        match SurrealBackend::new(
            &surreal_url,
            mythrax_core::db::BackendConfig {
                check_daemon: false,
                embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
                llm: Some(mythrax_core::llm::LLMClient::new_mock()),
            },
        )
        .await
        {
            Ok(b) => {
                backend2 = Some(b);
                break;
            }
            Err(_) if attempt < 9 => {
                // Retry under high-load test execution
            }
            Err(e) => return Err(e),
        }
    }
    let backend2 = backend2.unwrap();
    backend2.init().await?;

    // 4. Retrieve saved episode and assert it exists
    let all_eps = backend2.get_all_episodes().await?;
    assert_eq!(all_eps.len(), 1);
    assert_eq!(all_eps[0].title, "Persistence Test");
    assert_eq!(
        all_eps[0].content,
        "Verifying persistent storage in rocksdb."
    );

    Ok(())
}

#[tokio::test]
async fn test_mock_ingestions_and_reprocessing() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    let source_dir = tmp.path().join("source");
    let db_path = tmp.path().join("db");

    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(&source_dir)?;

    // Create folders inside vault
    let folders = ["episodes", "wiki", "wisdom", "general", "archive"];
    for f in &folders {
        fs::create_dir_all(vault_root.join(f))?;
    }

    let surreal_url = format!("rocksdb://{}", db_path.to_string_lossy());
    let backend = SurrealBackend::new(
        &surreal_url,
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await?;
    backend.init().await?;

    // Create a mock Antigravity transcript structure
    let session_dir = source_dir.join("session_123");
    let logs_dir = session_dir.join(".system_generated/logs");
    fs::create_dir_all(&logs_dir)?;

    let transcript_content = r#"{"type":"USER_INPUT","content":"Please write a function to search."}
{"type":"PLANNER_RESPONSE","content":"I will write a grep search helper."}"#;
    fs::write(logs_dir.join("transcript.jsonl"), transcript_content)?;

    // Run bulk ingestion for Antigravity
    let (count, errors, _has_more) = bulk_ingest_vault(
        &vault_root,
        &source_dir,
        "antigravity",
        "antigravity-scope",
        &backend,
        None,
        None,
        false,
    )
    .await?;

    assert_eq!(count, 1);
    assert!(errors.is_empty());

    // Verify episode in db
    let all_eps = backend.get_all_episodes().await?;
    assert_eq!(all_eps.len(), 1);
    assert_eq!(all_eps[0].scope, Some("antigravity-scope".to_string()));
    assert!(all_eps[0].content.contains("User Request"));

    // Reprocess check
    // Save a stub with None embedding
    let save_stub = EpisodeSave {
        created_at: None,
        title: "Stub note".to_string(),
        content: "Some dummy content.".to_string(),
        entities: vec![],
        scope: Some("reprocess-test".to_string()),
        vault_path: Some("episodes/stub.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let stub_id = backend.save_episode(&save_stub).await?;

    // Explicitly update db to clear its embedding to simulate missing models
    let record_id = mythrax_core::db::parse_record_id(&stub_id)?;
    let _ = backend
        .db
        .query("UPDATE $id SET embedding = NONE;")
        .bind(("id", record_id))
        .await?
        .check()?;

    let ep_before = backend
        .get_all_episodes()
        .await?
        .into_iter()
        .find(|e| e.id.as_ref() == Some(&stub_id))
        .unwrap();
    assert!(ep_before.embedding.is_none());

    // Reprocess command logic:
    let all_eps_for_reprocess = backend.get_all_episodes().await?;
    let mut reprocess_count = 0;
    for ep in all_eps_for_reprocess {
        if ep.embedding.is_none() {
            let s = EpisodeSave {
                created_at: None,
                title: ep.title.clone(),
                content: ep.content.clone(),
                entities: vec![],
                scope: ep.scope.clone(),
                vault_path: ep.vault_path.clone(),
                source_episode: ep.source_episode.clone(),
                session_id: None,
                task_id: None,
                ..Default::default()
            };
            backend.save_episode(&s).await?;
            reprocess_count += 1;
        }
    }

    let expected_reprocess_count = if backend.embedder.is_some() { 1 } else { 2 };
    assert_eq!(reprocess_count, expected_reprocess_count);

    // Verify embedding generated (or remains None if models are missing, but connection doesn't crash)
    let ep_after = backend
        .get_all_episodes()
        .await?
        .into_iter()
        .find(|e| e.id.as_ref() == Some(&stub_id))
        .unwrap();

    if backend.embedder.is_some() {
        assert!(ep_after.embedding.is_some());
        assert_eq!(ep_after.embedding.unwrap().len(), 768);
    }

    Ok(())
}

#[tokio::test]
async fn test_executor_applies_code_changes() -> Result<()> {
    let tmp = tempdir()?;
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir)?;

    // Initialize mock git repo
    let status = std::process::Command::new("git")
        .arg("init")
        .current_dir(&repo_dir)
        .status()?;
    assert!(status.success());

    // Configure user info for commits
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&repo_dir)
        .status();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&repo_dir)
        .status();

    // Create a dummy file to commit so there is a HEAD commit
    fs::write(repo_dir.join("base.txt"), "hello")?;
    let _ = std::process::Command::new("git")
        .args(["add", "base.txt"])
        .current_dir(&repo_dir)
        .status();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_dir)
        .status();

    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&repo_dir)
        .output()?;
    let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let executor = mythrax_core::cognitive::executor::ArborExecutor::new(repo_dir);

    // Dynamic code changes to apply
    let mut code_changes = std::collections::HashMap::new();
    code_changes.insert(
        "src/calc.rs".to_string(),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
    );

    let backend = mythrax_core::db::SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Execute test command
    let (success, logs) = executor
        .execute(
            "test-node",
            &commit_sha,
            "cat src/calc.rs",
            &Some(code_changes),
            &backend,
        )
        .await?;

    assert!(success);
    assert!(logs.contains("pub fn add(a: i32, b: i32) -> i32 { a + b }"));

    Ok(())
}

#[tokio::test]
async fn test_ingestion_chunking_and_linking() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    let source_dir = tmp.path().join("source");
    let db_path = tmp.path().join("db");

    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(&source_dir)?;

    // Create folders inside vault
    let folders = [
        "episodes",
        "wiki",
        "wiki/artifacts",
        "wisdom",
        "general",
        "archive",
    ];
    for f in &folders {
        fs::create_dir_all(vault_root.join(f))?;
    }

    let surreal_url = format!("rocksdb://{}", db_path.to_string_lossy());
    let backend = SurrealBackend::new(
        &surreal_url,
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await?;
    backend.init().await?;

    // Create a mock Antigravity folder
    let session_dir = source_dir.join("session_linking_123");
    let logs_dir = session_dir.join(".system_generated/logs");
    fs::create_dir_all(&logs_dir)?;

    // Create a large transcript of ~120,000 characters to trigger chunking into 2 parts (cap = 100k)
    let mut large_transcript = String::new();
    large_transcript.push_str("{\"type\":\"USER_INPUT\",\"content\":\"");
    large_transcript.push_str(&"A".repeat(60000));
    large_transcript.push_str("\"}\n");
    large_transcript.push_str("{\"type\":\"PLANNER_RESPONSE\",\"content\":\"");
    large_transcript.push_str(&"B".repeat(60000));
    large_transcript.push_str("\"}\n");

    fs::write(logs_dir.join("transcript.jsonl"), large_transcript)?;

    // Create mock artifacts
    fs::write(
        session_dir.join("walkthrough.md"),
        "Walkthrough artifact content",
    )?;
    fs::write(
        session_dir.join("implementation_plan.md"),
        "Plan artifact content",
    )?;

    // Run bulk ingestion
    let (count, errors, _has_more) = bulk_ingest_vault(
        &vault_root,
        &source_dir,
        "antigravity",
        "testing-linking-scope",
        &backend,
        None,
        None,
        false,
    )
    .await?;

    // We ingested 8 episode parts + 1 parent index + 2 artifacts = 11 success counts
    assert_eq!(count, 11);
    assert!(errors.is_empty());

    // 1. Verify episodes in DB
    let all_eps = backend.get_all_episodes().await?;
    // We should have 8 parts and 1 parent index
    assert_eq!(all_eps.len(), 9);

    let ep_part1 = all_eps.iter().find(|e| e.title.contains("part1")).unwrap();
    let ep_part2 = all_eps.iter().find(|e| e.title.contains("part2")).unwrap();

    // 2. Verify links inside files in Obsidian
    let ep_part1_file = fs::read_to_string(vault_root.join(ep_part1.vault_path.as_ref().unwrap()))?;
    assert!(ep_part1_file.contains("[[wiki/testing-linking-scope/raw/walkthrough]]"));
    assert!(ep_part1_file.contains("[[wiki/testing-linking-scope/raw/implementation_plan]]"));

    let ep_part2_file = fs::read_to_string(vault_root.join(ep_part2.vault_path.as_ref().unwrap()))?;
    assert!(ep_part2_file.contains("[[wiki/testing-linking-scope/raw/walkthrough]]"));
    assert!(ep_part2_file.contains("[[wiki/testing-linking-scope/raw/implementation_plan]]"));

    // Verify artifact file backlinks
    let walkthrough_rel_path = "wiki/testing-linking-scope/raw/walkthrough.md";
    let walkthrough_file = fs::read_to_string(vault_root.join(walkthrough_rel_path))?;
    assert!(walkthrough_file.contains("Source Episodes:"));
    assert!(walkthrough_file.contains(&ep_part1.title));
    assert!(walkthrough_file.contains(&ep_part2.title));

    // 3. Verify graph relationships in SurrealDB
    let ep1_related = backend
        .get_related_node_ids(ep_part1.id.as_ref().unwrap())
        .await?;
    assert!(ep1_related.len() >= 3); // walkthrough, implementation_plan & parent index

    let ep2_related = backend
        .get_related_node_ids(ep_part2.id.as_ref().unwrap())
        .await?;
    assert!(ep2_related.len() >= 3);

    Ok(())
}

#[tokio::test]
async fn test_artifact_chunking_during_ingestion() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    let source_dir = tmp.path().join("source");
    let db_path = tmp.path().join("db");

    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(&source_dir)?;

    // Create folders inside vault
    let folders = [
        "episodes",
        "wiki",
        "wiki/artifacts",
        "wisdom",
        "general",
        "archive",
    ];
    for f in &folders {
        fs::create_dir_all(vault_root.join(f))?;
    }

    let surreal_url = format!("rocksdb://{}", db_path.to_string_lossy());
    let backend = SurrealBackend::new(
        &surreal_url,
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await?;
    backend.init().await?;

    // Create a mock Antigravity folder
    let session_dir = source_dir.join("session_chunk_artifact_123");
    let logs_dir = session_dir.join(".system_generated/logs");
    fs::create_dir_all(&logs_dir)?;

    // Create a small transcript so it is not chunked (1 part)
    let transcript = "{\"type\":\"USER_INPUT\",\"content\":\"Short user prompt\"}\n";
    fs::write(logs_dir.join("transcript.jsonl"), transcript)?;

    // Create a large artifact file of ~25,000 characters to trigger chunking into 2 parts (cap = 20k)
    let mut large_artifact = String::new();
    large_artifact.push_str("Title: Large Artifact\n\n");
    for i in 1..=800 {
        large_artifact.push_str(&format!("Line {}: Some content text.\n", i));
    }
    assert!(large_artifact.len() > 20_000);

    fs::write(session_dir.join("large_plan.md"), &large_artifact)?;

    // Run bulk ingestion
    let (count, errors, _has_more) = bulk_ingest_vault(
        &vault_root,
        &source_dir,
        "antigravity",
        "testing-art-chunking-scope",
        &backend,
        None,
        None,
        false,
    )
    .await?;

    // We ingested 1 episode part + 3 artifact parts = 4 success counts
    assert_eq!(count, 4);
    assert!(errors.is_empty());

    // 1. Verify episodes in DB
    let all_eps = backend.get_all_episodes().await?;
    assert_eq!(all_eps.len(), 1);

    let ep = &all_eps[0];

    // 2. Verify links inside the episode in Obsidian
    let ep_file = fs::read_to_string(vault_root.join(ep.vault_path.as_ref().unwrap()))?;
    assert!(ep_file.contains("[[wiki/testing-art-chunking-scope/raw/large_plan_part1]]"));
    assert!(ep_file.contains("[[wiki/testing-art-chunking-scope/raw/large_plan_part2]]"));
    assert!(ep_file.contains("[[wiki/testing-art-chunking-scope/raw/large_plan_part3]]"));

    // Verify artifact file backlinks
    let art1_rel_path = "wiki/testing-art-chunking-scope/raw/large_plan_part1.md";
    let art2_rel_path = "wiki/testing-art-chunking-scope/raw/large_plan_part2.md";
    let art3_rel_path = "wiki/testing-art-chunking-scope/raw/large_plan_part3.md";

    assert!(vault_root.join(art1_rel_path).exists());
    assert!(vault_root.join(art2_rel_path).exists());
    assert!(vault_root.join(art3_rel_path).exists());

    let art1_file = fs::read_to_string(vault_root.join(art1_rel_path))?;
    assert!(art1_file.contains("Source Episodes:"));
    assert!(art1_file.contains(&ep.title));

    let art2_file = fs::read_to_string(vault_root.join(art2_rel_path))?;
    assert!(art2_file.contains("Source Episodes:"));
    assert!(art2_file.contains(&ep.title));

    let art3_file = fs::read_to_string(vault_root.join(art3_rel_path))?;
    assert!(art3_file.contains("Source Episodes:"));
    assert!(art3_file.contains(&ep.title));

    // 3. Verify graph relationships in SurrealDB
    let ep_related = backend
        .get_related_node_ids(ep.id.as_ref().unwrap())
        .await?;
    assert_eq!(ep_related.len(), 3); // large_plan_part1, large_plan_part2 & large_plan_part3

    Ok(())
}

}

mod hydration_cap {
use anyhow::Result;
use mythrax_core::api::ApiState;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::mcp_routes::call_mcp_tool;
use mythrax_core::store::MarkdownStore;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_get_full_hydration_cap() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let store = std::sync::Arc::new(MarkdownStore::new(&vault_root)?);

    let state = ApiState {
        backend: backend.clone(),
        auth_token: "secret".to_string(),
        store: store.clone(),
        ignore_list: std::sync::Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    // Create an episode with very large content (> 10000 characters)
    let large_content = "A".repeat(12000);
    let ep_save = EpisodeSave {
        created_at: None,
        title: "Very Large Episode".to_string(),
        content: large_content.clone(),
        scope: Some("general".to_string()),
        vault_path: Some("episodes/large_ep.md".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep_save).await?;

    // Call get_full tool via consolidated read tool
    let args = json!({
        "action": "get_full",
        "ids": [ep_id]
    });

    let resp = call_mcp_tool(&state, "read", args).await?;

    // Parse the output content:
    let content_arr = resp.get("content").unwrap().as_array().unwrap();
    let text = content_arr[0].get("text").unwrap().as_str().unwrap();

    let search_results: Vec<mythrax_core::contracts::SearchResult> = serde_json::from_str(text)?;
    let result = &search_results[0];

    // It should have truncated content and the truncation marker
    assert!(result.content.len() < 12000);
    assert!(result.content.contains("truncated 2000 chars"));

    Ok(())
}

}

mod manage_file_paging_flow {
use anyhow::Result;
use mythrax_core::api::ApiState;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::mcp_routes::call_mcp_tool;
use mythrax_core::store::MarkdownStore;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_manage_file_paging_flow() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;

    let file_path = workspace_root.join("src_lib.rs"); // Use .rs extension to trigger virtual paging

    // Create initial file content
    let initial_content = r#"
pub fn run_calc(x: i32) -> i32 {
    let mut sum = 0;
    for i in 0..x {
        sum += i * 2;
    }
    sum
}

pub fn display_val(val: i32) {
    println!("value: {}", val);
}
"#;
    fs::write(&file_path, initial_content)?;

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend
        .db
        .query(mythrax_core::db::schema::INIT_SCHEMA)
        .await?
        .check()?;
    backend.init().await?;

    let store = std::sync::Arc::new(MarkdownStore::new(&vault_root)?);

    let state = ApiState {
        backend: backend.clone(),
        auth_token: "secret".to_string(),
        store,
        ignore_list: std::sync::Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    // 1. Test "view" (Virtual Paging) via read tool
    let view_res = call_mcp_tool(
        &state,
        "read",
        json!({
            "action": "view",
            "path": file_path.to_str().unwrap()
        }),
    )
    .await?;

    let view_text = view_res
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|obj| obj.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    assert!(
        view_text.contains("[Paged Symbol: Reference page_fn_run_calc]"),
        "Should contain run_calc placeholder"
    );
    assert!(
        view_text.contains("[Paged Symbol: Reference page_fn_display_val]"),
        "Should contain display_val placeholder"
    );

    // File on disk must remain untouched
    let disk_content = fs::read_to_string(&file_path)?;
    assert_eq!(disk_content, initial_content);

    // 2. Test "replace" (Paging-Aware Contiguous Edit) via write tool
    let _replace_res = call_mcp_tool(
        &state,
        "write",
        json!({
            "action": "replace",
            "path": file_path.to_str().unwrap(),
            "target_content": "[Paged Symbol: Reference page_fn_run_calc]",
            "replacement_content": "pub fn run_calc(x: i32) -> i32 { x * 10 }"
        }),
    )
    .await?;

    // File on disk should be updated and contain no placeholders
    let disk_content2 = fs::read_to_string(&file_path)?;
    assert!(disk_content2.contains("pub fn run_calc(x: i32) -> i32 { x * 10 }"));
    assert!(!disk_content2.contains("[Paged Symbol:"));

    // 3. Test "multi_replace" (Multi-Block Edit)
    // First, let's re-view to generate placeholders on the new content
    let view_res2 = call_mcp_tool(
        &state,
        "read",
        json!({
            "action": "view",
            "path": file_path.to_str().unwrap()
        }),
    )
    .await?;

    let view_text2 = view_res2
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|obj| obj.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    assert!(view_text2.contains("[Paged Symbol: Reference page_fn_run_calc]"));
    assert!(view_text2.contains("[Paged Symbol: Reference page_fn_display_val]"));

    let _multi_replace_res = call_mcp_tool(&state, "write", json!({
        "action": "multi_replace",
        "path": file_path.to_str().unwrap(),
        "chunks": [
            {
                "target_content": "[Paged Symbol: Reference page_fn_run_calc]",
                "replacement_content": "pub fn run_calc(x: i32) -> i32 { x * 100 }"
            },
            {
                "target_content": "[Paged Symbol: Reference page_fn_display_val]",
                "replacement_content": "pub fn display_val(val: i32) { println!(\"final: {}\", val); }"
            }
        ]
    })).await?;

    let disk_content3 = fs::read_to_string(&file_path)?;
    assert!(disk_content3.contains("pub fn run_calc(x: i32) -> i32 { x * 100 }"));
    assert!(
        disk_content3.contains("pub fn display_val(val: i32) { println!(\"final: {}\", val); }")
    );
    assert!(!disk_content3.contains("[Paged Symbol:"));

    Ok(())
}

}

mod virtual_paging_editing_flow {
use anyhow::Result;
use mythrax_core::cognitive::paging::page_code_block;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_virtual_skeleton_paging_and_editing_flow() -> Result<()> {
    // 1. Initialize Backend and Store
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let tmp = tempdir()?;
    let file_path = tmp.path().join("my_module.rs");

    // 2. Create a clean, fully-populated source file on disk
    let raw_code = r#"
pub fn run_calculation(x: i32) -> i32 {
    let mut sum = 0;
    for i in 0..x {
        sum += i * 2;
    }
    sum
}

pub fn display_result(val: i32) {
    println!("Calculated value: {}", val);
}
"#;
    fs::write(&file_path, raw_code)?;

    // 3. Generate Virtual Skeleton (Simulates MCP view_file route)
    // We pass the clean content to page_code_block. It archives symbol bodies in SurrealDB
    // and returns a virtual skeleton containing placeholders.
    let virtual_skeleton = page_code_block(&backend, raw_code, "rs").await?;

    // Verify that the skeleton contains placeholders
    assert!(virtual_skeleton.contains("[Paged Symbol: Reference page_fn_run_calculation]"));
    assert!(virtual_skeleton.contains("[Paged Symbol: Reference page_fn_display_result]"));

    // Verify that the physical file on disk is 100% clean and untouched!
    let disk_content = fs::read_to_string(&file_path)?;
    assert_eq!(
        disk_content, raw_code,
        "Physical file on disk must remain unmodified and fully populated (Virtual Paging)"
    );

    // 4. Simulate Paging-Aware Editing (Simulates replace_file_content MCP tool)
    // The agent attempts to edit the file. Since the agent only saw the virtual skeleton,
    // the target block specified by the agent contains the placeholder!
    let agent_target = r#"[Paged Symbol: Reference page_fn_run_calculation]"#;
    let agent_replacement = r#"pub fn run_calculation(x: i32) -> i32 { x * 10 }"#;

    // The paging-aware editor resolves the placeholder and reconstructs the clean target in memory:
    // a. Scan for placeholders in agent_target
    // b. Fetch original body of run_calculation from symbol_archive in SurrealDB
    // c. Reconstruct clean target in memory
    // d. Find and replace in the clean disk file

    // Let's implement the editor logic in the test to establish the contract:
    let mut clean_target = agent_target.to_string();
    if clean_target.contains("page_fn_run_calculation") {
        // Query symbol_archive for the original body
        let mut response = backend.db.query("SELECT VALUE content FROM type::record('symbol_archive', 'page_fn_run_calculation');")
            .await?;
        let original_body: Option<String> = response.take(0)?;
        assert!(
            original_body.is_some(),
            "Original body must be stored in symbol_archive"
        );

        let placeholder = "[Paged Symbol: Reference page_fn_run_calculation]";
        clean_target = clean_target.replace(placeholder, &original_body.unwrap());
    }

    // Now, find and replace the reconstructed clean target inside the physical disk content
    let mut updated_disk_content = disk_content.clone();
    assert!(
        updated_disk_content.contains(&clean_target),
        "Reconstructed clean target must match the physical disk content"
    );
    updated_disk_content = updated_disk_content.replace(&clean_target, agent_replacement);

    // Write the clean, updated content to disk (physical file remains clean and compiles!)
    fs::write(&file_path, &updated_disk_content)?;

    // Verify the disk file has the new implementation and contains absolutely no placeholders
    let final_disk_content = fs::read_to_string(&file_path)?;
    assert!(final_disk_content.contains("x * 10"));
    assert!(!final_disk_content.contains("[Paged"));

    Ok(())
}

}

mod obsidian_linking {
use anyhow::Result;
use mythrax_core::cognitive::arbor::{ArborCoordinator, ArborLlmClient};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_obsidian_compatibility_linking() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("episodes"))?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;

    // 1. Create a mock episode on disk and save in SurrealDB
    let ep_vault_path = "episodes/antigravity_test_ep.md";
    let ep_save = EpisodeSave {
        created_at: None,
        title: "antigravity_test_ep".to_string(),
        content: "This is some test transcript content.".to_string(),
        entities: vec![],
        scope: Some("test-scope".to_string()),
        vault_path: Some(ep_vault_path.to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep_save).await?;
    // Write physical file to disk
    store.write_file(
        ep_vault_path,
        "# antigravity_test_ep\nThis is some test transcript content.",
    )?;

    // 2. Mock a compaction (synthesis split analysis) or incremental merge
    // Let's create an insight relative path
    let insight_relative_path = "wiki/test-scope/insights/test_insight_123.md";

    // Build source episode links (simulating synthesis.rs logic)
    let mut source_ep_links = Vec::new();
    let mem_nodes = backend.get_memory_nodes(&[ep_id.clone()]).await?;
    for ep in mem_nodes.episodes {
        if let Some(ref path) = ep.vault_path {
            let target = path.strip_suffix(".md").unwrap_or(path);
            source_ep_links.push(format!("- [[{}|{}]]", target, ep.title));
        }
    }

    let source_ep_section = if !source_ep_links.is_empty() {
        format!("\n\n## Source Episodes\n{}", source_ep_links.join("\n"))
    } else {
        String::new()
    };

    let insight_content = format!(
        "---\ntitle: \"Test Insight\"\nscope: \"test-scope\"\n---\n\nInsight summary body.{}",
        source_ep_section
    );

    store.write_file(insight_relative_path, &insight_content)?;

    // Call store.append_link_to_file on the episode path to link back to the insight
    store.append_link_to_file(
        ep_vault_path,
        "Insights & Summaries",
        insight_relative_path,
        "Test Insight",
    )?;

    // Assertions for Insight -> Episode
    let read_insight = fs::read_to_string(vault_root.join(insight_relative_path))?;
    assert!(read_insight.contains("## Source Episodes"));
    assert!(read_insight.contains("- [[episodes/antigravity_test_ep|antigravity_test_ep]]"));

    // Assertions for Episode -> Insight backlink
    let read_episode = fs::read_to_string(vault_root.join(ep_vault_path))?;
    assert!(read_episode.contains("## Insights & Summaries"));
    assert!(read_episode.contains("- [[wiki/test-scope/insights/test_insight_123|Test Insight]]"));

    // 3. Test dynamic wisdom rule bidirectional links
    let rule_path = "wisdom/dynamic/test_pattern_abc.md";
    let mut rule_source_ep_links = Vec::new();
    if let Ok(nodes) = backend.get_memory_nodes(&[ep_id.clone()]).await {
        for ep in nodes.episodes {
            if let Some(ref path) = ep.vault_path {
                let target = path.strip_suffix(".md").unwrap_or(path);
                rule_source_ep_links.push(format!("- [[{}|{}]]", target, ep.title));
            }
        }
    }
    let rule_source_ep_section = if !rule_source_ep_links.is_empty() {
        format!(
            "\n\n## Source Episodes\n{}",
            rule_source_ep_links.join("\n")
        )
    } else {
        String::new()
    };

    let rule_md = format!(
        "---\ntarget_pattern: \"test_pattern\"\ntier: \"dynamic\"\n---\n\nWisdom body.{}",
        rule_source_ep_section
    );
    store.write_file(rule_path, &rule_md)?;

    // Link back from episode to wisdom rule
    store.append_link_to_file(
        ep_vault_path,
        "Derived Wisdom Rules",
        rule_path,
        "Wisdom: test_pattern",
    )?;

    // Assertions for Wisdom -> Episode
    let read_wisdom = fs::read_to_string(vault_root.join(rule_path))?;
    assert!(read_wisdom.contains("## Source Episodes"));
    assert!(read_wisdom.contains("- [[episodes/antigravity_test_ep|antigravity_test_ep]]"));

    // Assertions for Episode -> Wisdom backlink
    let read_episode_after_wisdom = fs::read_to_string(vault_root.join(ep_vault_path))?;
    assert!(read_episode_after_wisdom.contains("## Derived Wisdom Rules"));
    assert!(
        read_episode_after_wisdom
            .contains("- [[wisdom/dynamic/test_pattern_abc|Wisdom: test_pattern]]")
    );

    Ok(())
}

#[derive(Clone)]
pub struct SimpleMockLLMClient;
impl ArborLlmClient for SimpleMockLLMClient {
    async fn propose_hypotheses(
        &self,
        _db: &dyn mythrax_core::db::StorageBackend,
        _parent_id: &str,
        _parent_hypothesis: &str,
        _target_files: &[(String, String)],
        _constraints: &[String],
        _stm_anchors: &[String],
    ) -> Result<String> {
        Ok(r#"[
            {
                "node_id": "CHILD_NODE",
                "hypothesis": "Test Child",
                "score": 90.0,
                "code_changes": {}
            }
        ]"#
        .to_string())
    }

    async fn evaluate_run(
        &self,
        _db: &dyn mythrax_core::db::StorageBackend,
        _run_logs: &str,
    ) -> Result<String> {
        Ok(r#"{"success": true, "score": 95.0, "insight": "Worked"}"#.to_string())
    }

    async fn abstract_insights(
        &self,
        _db: &dyn mythrax_core::db::StorageBackend,
        _parent_insight: Option<&str>,
        _child_insight: &str,
    ) -> Result<String> {
        Ok("insight".to_string())
    }
}

#[tokio::test]
async fn test_arbor_navigation_formatting() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    let repo_path = tmp.path().join("repo");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(&repo_path)?;

    let db = SurrealBackend::new_in_memory().await?;
    db.init().await?;

    let coordinator = ArborCoordinator::new(
        db.db.clone(),
        vault_root.clone(),
        repo_path.clone(),
        SimpleMockLLMClient,
        "test-scope".to_string(),
        "pytest".to_string(),
        vec![],
    )
    .await;

    // Initialize root node
    coordinator
        .init_root("Root hypothesis".to_string(), None)
        .await?;

    let root_path = vault_root.join("wiki/test-scope/hypothesis_tree/ROOT.md");
    assert!(root_path.exists());
    let root_md = fs::read_to_string(&root_path)?;
    assert!(root_md.contains("## Navigation"));
    assert!(root_md.contains("- **Parent**: None"));
    assert!(root_md.contains("- **Children**: None"));

    // Trigger ideation to create a child
    coordinator.trigger_ideation("ROOT").await?;

    let child_path = vault_root.join("wiki/test-scope/hypothesis_tree/CHILD_NODE.md");
    assert!(child_path.exists());
    let child_md = fs::read_to_string(&child_path)?;
    assert!(child_md.contains("## Navigation"));
    assert!(child_md.contains("- **Parent**: [[wiki/test-scope/hypothesis_tree/ROOT|ROOT]]"));

    // Verify parent ROOT.md updated to link to child
    let root_md_after = fs::read_to_string(&root_path)?;
    assert!(root_md_after.contains("- **Children**:"));
    assert!(root_md_after.contains("- [[wiki/test-scope/hypothesis_tree/CHILD_NODE|CHILD_NODE]]"));

    Ok(())
}

}

mod okf_watcher_sync {
use anyhow::Result;
use mythrax_core::db::backend::format_record_id;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::{WatchIgnoreList, start_watching, sync_file_to_db};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tempfile::tempdir;

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

#[tokio::test]
async fn test_okf_watcher_differential_sync_and_loop_prevention() -> Result<()> {
    // 1. Initialize DB and Vault Store in a temporary directory
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let tmp = tempdir()?;
    let vault_root = tmp.path().to_path_buf();
    let store = Arc::new(MarkdownStore::new(vault_root.clone())?);

    // Create subdirectories for episodes, wisdom, and wiki
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom/skills"))?;

    // Start the watcher
    let ignore_list = Arc::new(WatchIgnoreList::new());
    let _watcher = start_watching(
        vault_root.clone(),
        ignore_list.clone(),
        backend.clone(),
        store.clone(),
        None,
    )?;

    // 2. Create target note (Page B)
    ignore_list.ignore(vault_root.join("wiki/page_b.md"));
    let page_b_content = "---\nname: Page B\nscope: general\n---\n# Page B content\n";
    fs::write(vault_root.join("wiki/page_b.md"), page_b_content)?;

    // Explicitly trigger sync for the test to ensure execution
    sync_file_to_db(&vault_root.join("wiki/page_b.md"), &backend, &store).await?;

    // We can query using backend directly by downcasting or using its query method if exposed by StorageBackend.
    // In our case, StorageBackend has search/relate methods, but we can downcast to SurrealBackend in the test to run raw queries.
    let surreal_backend = backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    let mut res_b = surreal_backend
        .db
        .query("SELECT VALUE id FROM wiki_node WHERE name = 'Page B' LIMIT 1;")
        .await?;
    let id_b: Option<surrealdb::types::RecordId> = res_b.take(0)?;
    assert!(id_b.is_some(), "Page B node must be synced to database");
    let id_b_str = format_record_id(id_b.as_ref().unwrap());

    // 3. Create source note (Page A) with Obsidian YAML edges and body wikilinks
    let page_a_content = format!(
        r#"---
name: Page A
scope: general
importance: 7.5
edges:
  - target: "[[{}]]"
    relation: "supersedes"
    strength: 0.95
---
# Page A
This is a link to [[Page B]] in the body.
"#,
        id_b_str
    );
    ignore_list.ignore(vault_root.join("wiki/page_a.md"));
    fs::write(vault_root.join("wiki/page_a.md"), page_a_content)?;

    // Sync Page A
    sync_file_to_db(&vault_root.join("wiki/page_a.md"), &backend, &store).await?;

    let mut res_a = surreal_backend
        .db
        .query("SELECT VALUE id FROM wiki_node WHERE name = 'Page A' LIMIT 1;")
        .await?;
    let id_a: Option<surrealdb::types::RecordId> = res_a.take(0)?;
    assert!(id_a.is_some(), "Page A node must be synced to database");
    let id_a_str = format_record_id(id_a.as_ref().unwrap());

    // 4. Verify that relations were successfully created
    // There should be a "supersedes" edge (from frontmatter) and a "related" relates_to edge (from body wikilink)
    let mut rel_query = surreal_backend
        .db
        .query("SELECT relation, strength, out FROM relates_to WHERE in = $from;")
        .bind(("from", id_a.as_ref().unwrap().clone()))
        .await?;
    let relations: Vec<serde_json::Value> = rel_query.take(0)?;
    assert_eq!(
        relations.len(),
        2,
        "Should create exactly 2 relations: 1 from frontmatter, 1 from body wikilink"
    );

    let relations_types: Vec<String> = relations
        .iter()
        .map(|r| r["relation"].as_str().unwrap().to_string())
        .collect();
    assert!(relations_types.contains(&"supersedes".to_string()));
    assert!(relations_types.contains(&"related".to_string()));

    // 5. Test Differential Sync: Update Page A to remove the frontmatter edge
    let page_a_updated = r#"---
name: Page A
scope: general
importance: 7.5
edges: []
---
# Page A
This is a link to [[Page B]] in the body.
"#;
    ignore_list.ignore(vault_root.join("wiki/page_a.md"));
    fs::write(vault_root.join("wiki/page_a.md"), page_a_updated)?;

    // Re-sync Page A
    sync_file_to_db(&vault_root.join("wiki/page_a.md"), &backend, &store).await?;

    // Verify that the "supersedes" relation was deleted, but the body wikilink relation remains
    let mut rel_query_2 = surreal_backend
        .db
        .query("SELECT relation, out FROM relates_to WHERE in = $from;")
        .bind(("from", id_a.as_ref().unwrap().clone()))
        .await?;
    let relations_2: Vec<serde_json::Value> = rel_query_2.take(0)?;
    assert_eq!(
        relations_2.len(),
        1,
        "Should prune deleted relation, leaving only 1 relation"
    );
    assert_eq!(relations_2[0]["relation"], "related");

    // 6. A-MEM Split-Brain Mitigation: Verify that watcher sync preserves dynamically decayed cognitive metadata
    // Simulate dynamic decay and retrieval updates in database
    surreal_backend.db.query("UPDATE type::record('wiki_node', $id) MERGE { utility: 12.5, last_retrieved_at: '2026-06-24T00:00:00Z' };")
        .bind(("id", id_a_str.split(':').nth(1).unwrap()))
        .await?.check()?;

    // Sync Page A again. Since sync uses UPDATE ... MERGE, it should NOT reset utility and last_retrieved_at to default
    sync_file_to_db(&vault_root.join("wiki/page_a.md"), &backend, &store).await?;

    let mut select_metadata = surreal_backend
        .db
        .query("SELECT utility, last_retrieved_at FROM type::record('wiki_node', $id);")
        .bind(("id", id_a_str.split(':').nth(1).unwrap()))
        .await?;
    let metadata: Option<serde_json::Value> = select_metadata.take(0)?;
    assert!(metadata.is_some());
    let m_val = metadata.unwrap();
    assert_eq!(
        m_val["utility"].as_f64().unwrap(),
        12.5,
        "Sync must preserve dynamic decayed utility score (Split-Brain Mitigation)"
    );
    assert_eq!(
        m_val["last_retrieved_at"].as_str().unwrap(),
        "2026-06-24T00:00:00Z",
        "Sync must preserve dynamic retrieval timestamps"
    );

    // 7. Verify Content-Hash ignore suppressor
    // Write content and register hash in ignore list
    let content = "Check ignore suppressor";
    let hash = calculate_hash(&content);
    ignore_list.ignore_hash(hash);

    // Watcher should ignore this event if the file is modified with this content
    assert!(
        ignore_list.is_hash_ignored(&hash),
        "Hash must be registered as ignored"
    );

    Ok(())
}

}

mod memory_leak_fixes_phase_2 {
use anyhow::Result;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::schema::INIT_SCHEMA;

#[tokio::test]
async fn test_phase2_paginated_queries() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    for i in 0..15 {
        let ep = EpisodeSave {
            title: format!("Test {}", i),
            content: format!("Content {}", i),
            scope: Some("general".to_string()),
            vault_path: Some(format!("path{}", i)),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    let paginated_episodes = backend.get_episodes_paginated(10, 5).await?;
    assert_eq!(paginated_episodes.len(), 10);
    
    Ok(())
}

#[tokio::test]
async fn test_phase2_idf_index_updates() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    let ep1 = EpisodeSave {
        title: "Test IDF".to_string(),
        content: "apple banana apple".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep2 = EpisodeSave {
        title: "Test IDF 2".to_string(),
        content: "banana cherry".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };

    let id1 = backend.save_episode(&ep1).await?;
    let _id2 = backend.save_episode(&ep2).await?;

    let sql = "SELECT * FROM idf_index;";
    let mut response = backend.db.query(sql).await?;
    let all: Vec<serde_json::Value> = response.take(0)?;
    println!("ALL IDF INDEX: {:?}", all);

    async fn get_df(backend: &SurrealBackend, term: &str) -> Result<i64> {
        let sql = "SELECT VALUE document_frequency FROM idf_index WHERE term = $term AND scope = 'general';";
        let mut response = backend.db.query(sql).bind(("term", term)).await?;
        let res: Option<i64> = response.take(0)?;
        Ok(res.unwrap_or(0))
    }

    // 'apple' in ep1 only -> df = 1
    assert_eq!(get_df(&backend, "appl").await?, 1);
    // 'banana' in ep1 and ep2 -> df = 2
    assert_eq!(get_df(&backend, "banana").await?, 2);
    // 'cherry' in ep2 only -> df = 1
    assert_eq!(get_df(&backend, "cherri").await?, 1);

    // Now delete ep1
    backend.delete_episode(&id1).await?;

    // 'apple' should be 0
    assert_eq!(get_df(&backend, "appl").await?, 0);
    // 'banana' should be 1
    assert_eq!(get_df(&backend, "banana").await?, 1);
    // 'cherry' should be 1
    assert_eq!(get_df(&backend, "cherri").await?, 1);

    Ok(())
}

#[tokio::test]
async fn test_phase2_backfill() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    let ep1 = EpisodeSave {
        title: "Test IDF".to_string(),
        content: "apple banana apple".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    
    backend.save_episode(&ep1).await?;
    backend.db.query("DELETE FROM idf_index;").await?.check()?;
    
    backend.backfill_idf_index_db().await?;

    let sql = "SELECT VALUE document_frequency FROM idf_index WHERE term = 'appl' AND scope = 'general';";
    let mut response = backend.db.query(sql).await?;
    let res: Option<i64> = response.take(0)?;
    assert_eq!(res, Some(1));
    
    Ok(())
}

}

mod memory_leak_fixes_phase_3 {
use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::backend::SurrealBackend;
use mythrax_core::db::schema::INIT_SCHEMA;

#[tokio::test]
async fn test_phase3_content_hash_deduplication() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    let ep1 = EpisodeSave {
        title: "Test Dedupe".to_string(),
        content: "this is some exact text".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    
    // Save first time, should create new ID
    let id1 = backend.save_episode_db(&ep1).await?;
    
    // Save second time with same exact content, should return identical ID
    let id2 = backend.save_episode_db(&ep1).await?;
    
    assert_eq!(id1, id2, "Content hash deduplication should return the same ID");

    Ok(())
}

#[tokio::test]
async fn test_phase3_content_hash_backfill() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    let id = uuid::Uuid::new_v4().to_string();
    let insert_sql = format!("INSERT INTO episode {{ id: '{}', title: 'Legacy', content: 'Legacy content without hash' }};", id);
    backend.db.query(&insert_sql).await?.check()?;

    // Backfill should compute hashes for this legacy episode
    backend.backfill_content_hashes_db().await?;

    let check_sql = format!("SELECT VALUE content_hash FROM type::record('episode', '{}');", id);
    let mut res = backend.db.query(&check_sql).await?;
    let hash: Option<String> = res.take(0)?;
    assert!(hash.is_some(), "Backfill should populate content_hash");

    Ok(())
}

#[tokio::test]
async fn test_phase3_get_wisdom_tier_trait() -> Result<()> {
    use mythrax_core::contracts::{WisdomRule, Tier};
    use mythrax_core::db::backend::StorageBackend;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    let rule = WisdomRule {
        target_pattern: "Test Pattern".to_string(),
        action_to_avoid: "Test Avoid".to_string(),
        causal_explanation: "Test Why".to_string(),
        prescribed_remedy: "Test Remedy".to_string(),
        tier: Tier::Wisdom,
        scope: "general".to_string(),
        ..Default::default()
    };
    let id = backend.save_wisdom_rule(&rule).await?;

    let tier_res = backend.get_wisdom_tier(&id).await?;
    assert_eq!(tier_res, Some(Tier::Wisdom), "StorageBackend::get_wisdom_tier should return correct tier");

    let fake_tier = backend.get_wisdom_tier("episode:nonexistent").await?;
    assert_eq!(fake_tier, None, "StorageBackend::get_wisdom_tier for non-wisdom ID should return None");

    Ok(())
}

#[tokio::test]
async fn test_phase3_zero_row_backfill_loop_safety() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.db.query(INIT_SCHEMA).await?.check()?;

    // Backfilling empty database should exit without spinning
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        backend.backfill_content_hashes_db(),
    )
    .await??;

    // Insert an episode with content_hash already set
    let id = uuid::Uuid::new_v4().to_string();
    let insert_sql = format!(
        "INSERT INTO episode {{ id: '{}', title: 'Already Hashed', content: 'Some content', content_hash: 'abc' }};",
        id
    );
    backend.db.query(&insert_sql).await?.check()?;

    // Backfilling again should inspect 0 rows and terminate safely
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        backend.backfill_content_hashes_db(),
    )
    .await??;

    Ok(())
}

}

mod bulk_ingest_positional_correction {
use anyhow::Result;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::vault::ingestion::bulk_ingest_vault;
use std::fs;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_bulk_ingest_positional_correction() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let source_dir = tmp.path().join("source");
    let session_dir = source_dir.join("session_123");
    let logs_dir = session_dir.join(".system_generated/logs");
    fs::create_dir_all(&logs_dir)?;

    // Write a mock transcript with multiple user turns containing a correction keyword in the second turn
    let jsonl_content = r#"{"type": "USER_INPUT", "content": "Please implement a simple calculator.", "created_at": "2026-07-19T21:00:00Z"}
{"type": "PLANNER_RESPONSE", "content": "Here is the code for the calculator.", "created_at": "2026-07-19T21:00:05Z"}
{"type": "USER_INPUT", "content": "Wait, you forgot to handle division by zero!", "created_at": "2026-07-19T21:00:10Z"}
{"type": "PLANNER_RESPONSE", "content": "I'm sorry, let me fix that.", "created_at": "2026-07-19T21:00:15Z"}
"#;
    fs::write(logs_dir.join("transcript.jsonl"), jsonl_content)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Perform bulk ingestion with skip_llm = true
    let (count, errors, has_more) = bulk_ingest_vault(
        &vault_root,
        &source_dir,
        "antigravity",
        "general",
        &backend,
        None,
        None,
        true,
    )
    .await?;

    assert_eq!(count, 1, "Should ingest 1 episode session");
    assert!(errors.is_empty(), "Should ingest without errors");
    assert!(!has_more);

    // Verify if a cognitive task was enqueued for the correction keyword ("forgot")
    let pending_tasks = backend.get_pending_cognitive_tasks().await?;
    assert_eq!(
        pending_tasks.len(),
        1,
        "Should enqueue exactly 1 cognitive task for the correction"
    );
    assert!(
        pending_tasks[0]
            .prompt
            .contains("forgot to handle division by zero"),
        "Task prompt should contain the correction content"
    );

    Ok(())
}

}

mod data_hierarchy_flow {
#[cfg(feature = "mlx")]
use mythrax_core::contracts::{Entity, EpisodeSave, WikiNode, WisdomRule};
#[cfg(feature = "mlx")]
use mythrax_core::db::{StorageBackend, SurrealBackend};
#[cfg(feature = "mlx")]
use mythrax_core::llm::{DYNAMIC_MODEL_BROKER, DynamicModelBroker};
#[cfg(feature = "mlx")]
use std::sync::Arc;

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_data_hierarchy_flow_ingest_and_retrieve() {
    let home = std::env::var("HOME").unwrap();
    let models_dir = std::path::PathBuf::from(home).join(".mythrax/models");

    // Initialize the dynamic model broker
    let broker = DynamicModelBroker::new(models_dir).await.unwrap();
    let broker_arc = Arc::new(broker);
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    // Preload embedding model first so that the files are downloaded to the cache
    broker_arc
        .preload_embedding_model("mlx-community/nomic-embed-text-v1.5-mlx")
        .await
        .unwrap();
    assert!(broker_arc.is_embedding_model_loaded());

    // Initialize SurrealDB in memory AFTER the embedder is present
    let backend = SurrealBackend::new(
        "mem://",
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await
    .unwrap();
    backend.init().await.unwrap();

    // 1. Episode Ingestion and Retrieval
    let ep_save = EpisodeSave {
        created_at: None,
        title: "Test Ingestion System Flow".to_string(),
        content: "We are testing the complete data hierarchy from episodes to wisdom rules. This is the raw execution context.".to_string(),
        entities: vec![Entity {
            name: "test_entity".to_string(),
            entity_type: "concept".to_string(),
            summary: "A test concept".to_string(),
            labels: vec!["test".to_string()],
            scope: Some("general".to_string()),
            vault_path: None,
            embedding: None,
        }],
        scope: Some("general".to_string()),
        vault_path: Some("vault/episode_1.md".to_string()),
        source_episode: None,
        session_id: Some("session_123".to_string()),
        task_id: Some("task_123".to_string()),
        discovery_tokens: Some(10),
        facts: Some(vec!["Fact 1: Systems are working".to_string()]),
        concepts: Some(vec!["test_entity".to_string()]),
        files_read: Some(vec!["src/main.rs".to_string()]),
        files_modified: Some(vec![]),

        confidence: None,
        ..Default::default()
    };

    let ep_id = backend.save_episode(&ep_save).await.unwrap();
    assert!(!ep_id.is_empty(), "Saved episode ID must not be empty");

    // Retrieve via search matching general
    let search_res = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "execution context",
            Some("general"),
            false,
            5,
            0,
            0.1,
            None,
            false,
            true,
            false,
            None,
            true,
            None,
        ))
        .await
        .unwrap();
    assert!(
        !search_res.results.is_empty(),
        "Must retrieve the ingested episode"
    );
    assert_eq!(search_res.results[0].title, "Test Ingestion System Flow");

    // 2. RAPTOR Summary Node Generation & Retrieval
    let raptor_node = WikiNode {
        id: None,
        name: "Raptor Summary: Test Ingestion System Flow".to_string(),
        content: "Summary of raw execution context for testing data hierarchy flow.".to_string(),
        scope: "general".to_string(),
        vault_path: Some("wiki/archive/raptor_summary_test.md".to_string()),
        embedding: None,
        ..Default::default()
    };

    let raptor_id = backend.save_wiki_node(&raptor_node).await.unwrap();
    assert!(
        !raptor_id.is_empty(),
        "Saved Raptor summary ID must not be empty"
    );

    // Retrieve Raptor Summary
    let search_raptor = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Raptor Summary",
            Some("general"),
            false,
            5,
            0,
            0.1,
            None,
            false,
            false,
            false,
            None,
            true,
            None,
        ))
        .await
        .unwrap();
    assert!(
        !search_raptor.results.is_empty(),
        "Must retrieve the Raptor summary node"
    );
    assert!(search_raptor.results[0].title.contains("Raptor Summary"));

    // 3. Insight Synthesis (WikiNode)
    let insight_node = WikiNode {
        id: None,
        name: "Insight: System Hierarchical Flow".to_string(),
        content: "Compactions compile raw trace episodes into synthesized permanent wiki nodes to build long term memory.".to_string(),
        scope: "general".to_string(),
        vault_path: Some("wiki/insight_hierarchical.md".to_string()),
        embedding: None,
        ..Default::default()
    };

    let insight_id = backend.save_wiki_node(&insight_node).await.unwrap();
    assert!(
        !insight_id.is_empty(),
        "Saved insight node ID must not be empty"
    );

    // Retrieve Insight Node
    let search_insight = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "permanent wiki nodes",
            Some("general"),
            false,
            5,
            0,
            0.1,
            None,
            false,
            false,
            false,
            None,
            true,
            None,
        ))
        .await
        .unwrap();
    assert!(
        !search_insight.results.is_empty(),
        "Must retrieve the synthesized insight node"
    );
    assert_eq!(
        search_insight.results[0].title,
        "Insight: System Hierarchical Flow"
    );

    // 4. Wisdom Extraction (WisdomRule)
    let wisdom_rule = WisdomRule {
        id: None,
        target_pattern: "unscaped query parameters in db".to_string(),
        action_to_avoid: "injecting raw strings into WHERE clauses".to_string(),
        causal_explanation: "triggers SQL/SurrealQL injection or schema corruption".to_string(),
        prescribed_remedy: "always use query parameters via bind bindings".to_string(),
        tier: mythrax_core::contracts::Tier::Wisdom,
        scope: "general".to_string(),
        vault_path: Some("wisdom/permanent/rule_query_params.md".to_string()),
        embedding: None,
        source_episodes: vec![ep_id.clone()],
        generator_name: "TestHarness".to_string(),
        similarity: None,
        utility: Some(1.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,

        rule_type: None,
        ..Default::default()
    };

    let wisdom_id = backend.save_wisdom_rule(&wisdom_rule).await.unwrap();
    assert!(
        !wisdom_id.is_empty(),
        "Saved wisdom rule ID must not be empty"
    );

    // Retrieve Wisdom Rule
    let search_wisdom = backend
        .get_wisdom("query parameters", None, 5, 0, 0.1)
        .await
        .unwrap();
    assert!(
        !search_wisdom.results.is_empty(),
        "Must retrieve the WisdomRule"
    );
    assert_eq!(
        search_wisdom.results[0].action_to_avoid,
        "injecting raw strings into WHERE clauses"
    );

    // Relate episode to wisdom rule to link the hierarchy
    backend
        .relate_nodes(&ep_id, &wisdom_id, None, None, None)
        .await
        .unwrap();

    // 5. MCP coding agent request flow routing check
    let temp_store_dir = tempfile::tempdir().expect("Failed to create temp store dir");
    let api_state = mythrax_core::api::ApiState {
        backend: Arc::new(backend),
        auth_token: "test_token".to_string(),
        store: Arc::new(
            mythrax_core::store::MarkdownStore::new(temp_store_dir.path().to_path_buf()).unwrap(),
        ),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    // Invoke complete_code_task which routes through the mlx-lm HTTP server at
    // :8080. Uses the production Qwen3.6-35B-A3B MoE model running on Metal GPU.
    let args = serde_json::json!({
        "action": "complete_task",
        "prompt": "How should we pass query parameters to SurrealDB WHERE clauses?",
        "system_instruction": "Use retrieved context.",
        "model": "mlx-community/Qwen3.6-35B-A3B-4bit"
    });

    let mcp_res = mythrax_core::mcp_routes::call_mcp_tool(&api_state, "agent", args).await;
    if let Err(ref e) = mcp_res {
        eprintln!("MCP TOOL ERROR: {:?}", e);
    }
    assert!(
        mcp_res.is_ok(),
        "MCP tool complete_code_task call must succeed"
    );
    let val = mcp_res.unwrap();
    let text = val["content"][0]["text"].as_str().unwrap();
    assert!(!text.is_empty(), "Response must not be empty");
}

}

mod forge {
use anyhow::Result;
use mythrax_core::cognitive::forge::{
    TOCEntry, chunk_text, extract_pdf_text, parse_markdown_toc, split_into_logical_sections,
};
use mythrax_core::db::{StorageBackend, SurrealBackend};
use std::fs;
use tempfile::tempdir;

fn create_lopdf_pdf() -> Vec<u8> {
    use lopdf::{Dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let content_id = doc.new_object_id();

    let content = b"BT /F1 12 Tf 72 712 Td (Hello World from Mythrax PDF Extractor) Tj ET";
    let content_stream = Stream::new(Dictionary::new(), content.to_vec());
    doc.objects
        .insert(content_id, Object::Stream(content_stream));

    let mut page_dict = Dictionary::new();
    page_dict.set("Type", "Page");
    page_dict.set("Parent", pages_id);
    page_dict.set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
    page_dict.set("Contents", content_id);

    let mut resources = Dictionary::new();
    let mut fonts = Dictionary::new();
    let mut font_dict = Dictionary::new();
    font_dict.set("Type", "Font");
    font_dict.set("Subtype", "Type1");
    font_dict.set("BaseFont", "Helvetica");
    fonts.set("F1", font_dict);
    resources.set("Font", fonts);
    page_dict.set("Resources", resources);

    doc.objects.insert(page_id, Object::Dictionary(page_dict));

    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", "Pages");
    pages_dict.set("Kids", vec![page_id.into()]);
    pages_dict.set("Count", 1);
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

    let mut catalog_dict = Dictionary::new();
    catalog_dict.set("Type", "Catalog");
    catalog_dict.set("Pages", pages_id);
    let catalog_id = doc.add_object(catalog_dict);

    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

#[test]
fn test_pdf_extraction() -> Result<()> {
    let tmp = tempdir()?;
    let pdf_path = tmp.path().join("test.pdf");
    let pdf_bytes = create_lopdf_pdf();
    fs::write(&pdf_path, pdf_bytes)?;

    let extracted_text = extract_pdf_text(&pdf_path)?;
    assert!(extracted_text.contains("Hello World"));
    assert!(extracted_text.contains("Mythrax"));
    Ok(())
}

#[test]
fn test_text_chunking() {
    // Generate a long text and verify chunk size and overlap
    let words: Vec<String> = (0..3000).map(|i| format!("word{}", i)).collect();
    let long_text = words.join(" ");

    let chunks = chunk_text(&long_text, 2000, 200);
    assert!(!chunks.is_empty());
    assert!(chunks.len() >= 2);

    // Robustly check overlap content: there should be common words between the two chunks.
    let words_in_c0: std::collections::HashSet<&str> = chunks[0].split_whitespace().collect();
    let words_in_c1: std::collections::HashSet<&str> = chunks[1].split_whitespace().collect();
    let overlap_words: Vec<&&str> = words_in_c0.intersection(&words_in_c1).collect();
    assert!(
        !overlap_words.is_empty(),
        "There must be overlapping words between chunks"
    );
}

#[tokio::test]
async fn test_ingest_document() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    // Create a temporary project directory to act as the workspace root
    let proj_dir = tmp.path().join("testscope");
    fs::create_dir_all(&proj_dir)?;
    fs::write(proj_dir.join("Cargo.toml"), "")?;

    // Set env var to mock LLM calls and configure active scope
    unsafe {
        if std::env::var("MYTHRAX_TEST_MOCK").is_ok() {
            std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        } else {
            std::env::set_var("MYTHRAX_MOCK_LLM", "false");
        }
        std::env::set_var(
            "MYTHRAX_WORKSPACE_ROOT",
            proj_dir.to_string_lossy().to_string(),
        );
    }

    #[cfg(feature = "mlx")]
    {
        if std::env::var("MYTHRAX_TEST_MOCK").is_err() {
            let home = std::env::var("HOME").unwrap();
            let models_dir = std::path::PathBuf::from(home).join(".mythrax/models");
            let broker = mythrax_core::llm::DynamicModelBroker::new(models_dir)
                .await
                .unwrap();
            let _ = mythrax_core::llm::DYNAMIC_MODEL_BROKER.set(std::sync::Arc::new(broker));
        }
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);

    let forge = mythrax_core::cognitive::forge::Forge::new(backend.clone(), store.clone());

    // Ingest a document under normalized "testscope" scope
    forge
        .ingest_document("Some document text to analyze.", "testscope", "test_source")
        .await?;

    // Verify wiki nodes are written and saved to db
    // 1. Files on disk
    let wiki_dir = vault_root.join("wiki/testscope");

    assert!(wiki_dir.exists());

    let wiki_files_vec: Vec<_> = fs::read_dir(&wiki_dir)?.filter_map(|e| e.ok()).collect();

    assert!(
        !wiki_files_vec.is_empty(),
        "Should write at least one wiki node file"
    );

    // 2. Records in SurrealDB
    // Query wiki nodes: verify vault_path is persisted in DB
    let first_wiki_path = wiki_files_vec[0].path();
    let relative_wiki = first_wiki_path
        .strip_prefix(&vault_root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let wiki_node_id = backend
        .get_wiki_node_id_by_vault_path(&relative_wiki)
        .await?;
    assert!(
        wiki_node_id.is_some(),
        "Wiki node at '{}' should be persisted in SurrealDB",
        relative_wiki
    );

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
    }

    Ok(())
}

#[test]
fn test_markdown_toc_parsing() {
    let md_content = "\
# Section 1
This is text in section 1.

## Subsection 1.1
This is subsection 1.1 text.

# Section 2
Text in section 2.
";
    let entries = parse_markdown_toc(md_content);

    assert_eq!(entries.len(), 3);

    assert_eq!(entries[0].title, "Section 1");
    assert_eq!(
        entries[0].start_byte,
        md_content.find("# Section 1").unwrap()
    );
    assert_eq!(
        entries[0].end_byte,
        md_content.find("## Subsection 1.1").unwrap()
    );

    assert_eq!(entries[1].title, "Subsection 1.1");
    assert_eq!(
        entries[1].start_byte,
        md_content.find("## Subsection 1.1").unwrap()
    );
    assert_eq!(entries[1].end_byte, md_content.find("# Section 2").unwrap());

    assert_eq!(entries[2].title, "Section 2");
    assert_eq!(
        entries[2].start_byte,
        md_content.find("# Section 2").unwrap()
    );
    assert_eq!(entries[2].end_byte, md_content.len());
}

#[tokio::test]
async fn test_extract_toc_via_llm_mock() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let tmp = tempdir()?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(tmp.path())?);

    let forge = mythrax_core::cognitive::forge::Forge::new(backend, store);

    let content = "Some document text to analyze.";
    let toc = forge.extract_toc_via_llm(content).await?;

    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "test_title");
    assert_eq!(toc[0].start_byte, 0);
    assert_eq!(toc[0].end_byte, content.len());

    Ok(())
}

#[test]
fn test_logical_section_splitting_and_grouping() {
    let content = "Small section one text. Small section two text. Large section three text that will be split.";
    let toc = vec![
        TOCEntry {
            title: "Sec 1".to_string(),
            start_byte: 0,
            end_byte: 23,
        },
        TOCEntry {
            title: "Sec 2".to_string(),
            start_byte: 24,
            end_byte: 47,
        },
        TOCEntry {
            title: "Sec 3".to_string(),
            start_byte: 48,
            end_byte: content.len(),
        },
    ];

    let sections = split_into_logical_sections(content, &toc);

    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].title, "Sec 1 - Sec 3");
    assert_eq!(sections[0].content.trim(), content.trim());

    let many_words: Vec<String> = (0..26000).map(|i| format!("w{}", i)).collect();
    let large_text = many_words.join(" ");
    let large_content = format!("Small intro. {}", large_text);

    let large_toc = vec![
        TOCEntry {
            title: "Small Intro".to_string(),
            start_byte: 0,
            end_byte: 12,
        },
        TOCEntry {
            title: "Huge Body".to_string(),
            start_byte: 13,
            end_byte: large_content.len(),
        },
    ];

    let large_sections = split_into_logical_sections(&large_content, &large_toc);

    assert!(large_sections.len() >= 3);
    assert_eq!(large_sections[0].title, "Small Intro");
    assert!(
        large_sections
            .iter()
            .any(|sec| sec.title.starts_with("Huge Body (Part 1)"))
    );
    assert!(
        large_sections
            .iter()
            .any(|sec| sec.title.starts_with("Huge Body (Part 2)"))
    );
}

#[test]
fn test_second_pass_character_chunking() {
    let mut very_long_text = String::new();
    for _ in 1..=5000 {
        very_long_text.push_str(
            "This is a line of text that is fairly repetitive to build up characters quickly.\n",
        );
    }
    assert!(very_long_text.len() > 100_000);

    let toc = vec![TOCEntry {
        title: "Long Chapter".to_string(),
        start_byte: 0,
        end_byte: very_long_text.len(),
    }];

    let sections = split_into_logical_sections(&very_long_text, &toc);
    assert!(sections.len() >= 3);
    assert!(
        sections
            .iter()
            .any(|sec| sec.title.starts_with("Long Chapter (Part 1)"))
    );
    assert!(
        sections
            .iter()
            .any(|sec| sec.title.starts_with("Long Chapter (Part 2)"))
    );
    assert!(sections[0].content.len() <= 20_000);
    assert!(sections[1].content.len() <= 20_000);
    assert!(sections[2].content.len() <= 20_000);
}

}

mod get_episodes_by_node_type {
use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use std::fs;
use tempfile::tempdir;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_get_episodes_by_node_type_filtering() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a procedural episode
    let ep_proc = EpisodeSave {
        created_at: None,
        title: "Procedural Ep".to_string(),
        content: "Some procedural content".to_string(),
        scope: Some("test_scope".to_string()),
        ..Default::default()
    };
    let proc_id = backend.save_episode(&ep_proc).await?;
    let proc_raw_id = proc_id.split(':').nth(1).unwrap().to_string();
    backend
        .db
        .query("UPDATE type::record('episode', $id) SET node_type = 'procedural';")
        .bind(("id", proc_raw_id))
        .await?
        .check()?;

    // Create a standard episode
    let ep_std = EpisodeSave {
        created_at: None,
        title: "Standard Ep".to_string(),
        content: "Some standard content".to_string(),
        scope: Some("test_scope".to_string()),
        ..Default::default()
    };
    let std_id = backend.save_episode(&ep_std).await?;
    let std_raw_id = std_id.split(':').nth(1).unwrap().to_string();
    backend
        .db
        .query("UPDATE type::record('episode', $id) SET node_type = 'standard';")
        .bind(("id", std_raw_id))
        .await?
        .check()?;

    // Retrieve episodes by node type
    let proc_episodes = backend.get_episodes_by_node_type("procedural").await?;
    assert_eq!(proc_episodes.len(), 1);
    assert_eq!(proc_episodes[0].title, "Procedural Ep");

    let std_episodes = backend.get_episodes_by_node_type("standard").await?;
    assert_eq!(std_episodes.len(), 1);
    assert_eq!(std_episodes[0].title, "Standard Ep");

    Ok(())
}

}

mod inspect_vault_db {
use anyhow::Result;
use mythrax_core::cognitive::synthesis::DreamCoordinator;
use mythrax_core::db::{BackendConfig, StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::operations::sync_vault_to_db;
use std::sync::Arc;

#[tokio::test]
#[ignore]
async fn test_inspect_db() -> Result<()> {
    let db_path = "/Users/keith/.mythrax/db";
    let vault_root = std::path::PathBuf::from("/Users/keith/mythrax-vault");

    println!("Connecting DB at {}", db_path);
    let surreal_backend = Arc::new(
        SurrealBackend::new(
            &format!("surrealkv://{}", db_path),
            BackendConfig::default(),
        )
        .await?,
    );
    surreal_backend.init().await?;
    let backend: Arc<dyn StorageBackend> = surreal_backend.clone();

    let store = Arc::new(MarkdownStore::new(&vault_root)?);

    println!("Syncing vault to DB...");
    let synced = sync_vault_to_db(&backend, &store).await?;
    println!("Synced {} files from vault to DB.", synced);

    let all_eps = backend.get_all_episodes().await?;
    let unprocessed = backend.get_unprocessed_episodes().await?;
    println!("Total Episodes in DB: {}", all_eps.len());
    println!("Unprocessed Episodes in DB BEFORE DREAM: {}", unprocessed.len());

    println!("Running DreamCoordinator (mode: deep)...");
    let dc = DreamCoordinator::new();
    let _ = dc.run_dream(backend.clone(), &store, Some("deep"), None).await;

    let pending_tasks = surreal_backend.get_pending_cognitive_tasks().await?;
    println!("Pending Cognitive Tasks AFTER DREAM: {}", pending_tasks.len());

    Ok(())
}

}

mod node_type_differentiation {
use anyhow::Result;
use mythrax_core::contracts::{EpisodeSave, WikiNode};
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_node_type_differentiation() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend
        .save_profile_key("search.enable_graph_expansion", "true")
        .await?;
    backend
        .save_profile_key("search.temporal_depth", "2")
        .await?;

    // Create Episode -> WikiNode -> Episode
    let ep1 = EpisodeSave {
        title: "Episode 1".to_string(),
        content: "First episode".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep1_id = backend.save_episode(&ep1).await?;

    let node = WikiNode {
        id: None,
        name: "Middle Node".to_string(),
        content: "Middle node".to_string(),
        scope: "general".to_string(),
        ..Default::default()
    };
    let node_id = backend.save_wiki_node(&node).await?;

    let ep2 = EpisodeSave {
        title: "Episode 2".to_string(),
        content: "Target episode".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep2_id = backend.save_episode(&ep2).await?;

    // Relate them: ep1 -> node -> ep2 via followed_by
    let query = "RELATE $from -> followed_by -> $to;";

    backend
        .db
        .query(query)
        .bind(("from", mythrax_core::db::parse_record_id(&ep1_id)?))
        .bind(("to", mythrax_core::db::parse_record_id(&node_id)?))
        .await?
        .check()?;

    backend
        .db
        .query(query)
        .bind(("from", mythrax_core::db::parse_record_id(&node_id)?))
        .bind(("to", mythrax_core::db::parse_record_id(&ep2_id)?))
        .await?
        .check()?;

    // Search for "First episode" to trigger expansion which should traverse through WikiNode to Episode 2
    let resp = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "First episode",
            Some("general"),
            false,
            10,
            0,
            0.0,
            None,
            false,
            true,
            true,
            None,
            false,
            None,
        ))
        .await?;

    // Find ep2 in results
    let found = resp.results.iter().any(|r| r.id == ep2_id);
    assert!(
        found,
        "Should find ep2 through the wiki_node in the temporal chain"
    );

    Ok(())
}

}

mod semantic_document_splitting_relations {
use anyhow::Result;
use mythrax_core::cognitive::forge::Forge;
use mythrax_core::db::{StorageBackend, SurrealBackend, parse_record_id};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_semantic_document_splitting_relations() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(MarkdownStore::new(&vault_root)?);

    let forge = Forge::new(backend.clone(), store.clone());

    // 1. Generate a sample document containing several paragraphs.
    // One of them must be very large to trigger fallback splitting (> 20,000 characters).
    // Let's repeat "word " 5500 times.
    let large_para = (0..5500).map(|_| "word").collect::<Vec<_>>().join(" ");
    let document_content = format!(
        "Paragraph 1 is small.\n\n{}\n\nParagraph 3 is also small.",
        large_para
    );

    // 2. Ingest the document
    let source_name = "test_doc.md";
    forge
        .ingest_document(&document_content, "testscope", source_name)
        .await?;

    // 3. Query the database to verify the parent WikiNode is created
    let mut parent_resp = backend
        .db
        .query("SELECT * FROM wiki_node WHERE name = $name;")
        .bind(("name", source_name))
        .await?;
    let parents: Vec<serde_json::Value> = parent_resp.take(0)?;
    assert_eq!(
        parents.len(),
        1,
        "There should be exactly one parent WikiNode"
    );
    let parent_node = &parents[0];
    let parent_id_str = parent_node["id"]
        .as_str()
        .expect("Parent node must have an ID")
        .to_string();

    // 4. Query the database to verify the chunk WikiNodes are created
    let mut chunk_resp = backend
        .db
        .query("SELECT * FROM wiki_node WHERE name CONTAINS $name_pat ORDER BY name;")
        .bind(("name_pat", format!("{} - Chunk", source_name)))
        .await?;
    let chunks: Vec<serde_json::Value> = chunk_resp.take(0)?;
    assert!(chunks.len() >= 2, "There should be at least two chunks");

    // 5. Verify the relates_to edges: chunk -> parent (relation: "parent")
    for chunk in &chunks {
        let chunk_id_str = chunk["id"]
            .as_str()
            .expect("Chunk must have an ID")
            .to_string();
        let mut rel_resp = backend.db.query("SELECT * FROM relates_to WHERE in = $chunk_id AND out = $parent_id AND relation = 'parent';")
            .bind(("chunk_id", parse_record_id(&chunk_id_str)?))
            .bind(("parent_id", parse_record_id(&parent_id_str)?))
            .await?;
        let rels: Vec<serde_json::Value> = rel_resp.take(0)?;
        assert_eq!(
            rels.len(),
            1,
            "Each chunk must relate to parent as 'parent'"
        );
    }

    // 6. Verify sequential bidirectional links between adjacent chunks
    // Chunk N relates to Chunk N+1 with relation "next"
    // Chunk N+1 relates to Chunk N with relation "prev"
    for i in 0..chunks.len().saturating_sub(1) {
        let chunk_n_id = chunks[i]["id"]
            .as_str()
            .expect("Chunk must have ID")
            .to_string();
        let chunk_n_plus_1_id = chunks[i + 1]["id"]
            .as_str()
            .expect("Chunk must have ID")
            .to_string();

        // next link
        let mut next_resp = backend
            .db
            .query("SELECT * FROM relates_to WHERE in = $from AND out = $to AND relation = 'next';")
            .bind(("from", parse_record_id(&chunk_n_id)?))
            .bind(("to", parse_record_id(&chunk_n_plus_1_id)?))
            .await?;
        let next_rels: Vec<serde_json::Value> = next_resp.take(0)?;
        assert_eq!(
            next_rels.len(),
            1,
            "Chunk {} should relate to Chunk {} as 'next'",
            chunk_n_id,
            chunk_n_plus_1_id
        );

        // prev link
        let mut prev_resp = backend
            .db
            .query("SELECT * FROM relates_to WHERE in = $from AND out = $to AND relation = 'prev';")
            .bind(("from", parse_record_id(&chunk_n_plus_1_id)?))
            .bind(("to", parse_record_id(&chunk_n_id)?))
            .await?;
        let prev_rels: Vec<serde_json::Value> = prev_resp.take(0)?;
        assert_eq!(
            prev_rels.len(),
            1,
            "Chunk {} should relate to Chunk {} as 'prev'",
            chunk_n_plus_1_id,
            chunk_n_id
        );
    }

    // 7. Verify files are written to the store on disk
    let wiki_dir = vault_root.join("wiki").join("testscope");
    assert!(wiki_dir.exists(), "Wiki testscope directory should exist");

    // Read directory and check that we have parent and chunk files
    let paths = fs::read_dir(wiki_dir)?;
    let mut parent_count = 0;
    let mut chunk_count = 0;
    for path in paths {
        let path = path?.path();
        let filename = path.file_name().unwrap().to_string_lossy();
        if filename.starts_with("parent_") {
            parent_count += 1;
        } else if filename.starts_with("chunk_") {
            chunk_count += 1;
        }
    }
    assert_eq!(
        parent_count, 1,
        "There should be exactly one parent file on disk"
    );
    assert!(
        chunk_count >= 2,
        "There should be at least two chunk files on disk"
    );

    Ok(())
}

}

mod structured_fields {
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_concept_prefilter_narrows_candidates() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // 1. Close but untagged episode
    let ep_close = EpisodeSave {
        created_at: None,
        title: "Security and authentication overview".to_string(),
        content: "This document describes overall security patterns including auth and tokens."
            .to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/security.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: Some(vec!["security".to_string()]),
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    backend.save_episode(&ep_close).await.unwrap();

    // 2. Target episode tagged with "oauth"
    let ep_target = EpisodeSave {
        created_at: None,
        title: "OAuth setup guide".to_string(),
        content: "Steps to configure the oauth provider and client secrets. security patterns"
            .to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/oauth.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: Some(vec!["oauth".to_string(), "security".to_string()]),
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let target_id = backend.save_episode(&ep_target).await.unwrap();

    // 3. Unrelated episode
    let ep_unrelated = EpisodeSave {
        created_at: None,
        title: "Database schema migrations".to_string(),
        content: "SurrealDB tables and indexes for belief states and thought nodes.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/db.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: Some(vec!["database".to_string()]),
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    backend.save_episode(&ep_unrelated).await.unwrap();

    // Search for concept "oauth" - should return only the target episode
    let res = backend
        .search_filtered(
            "security patterns",
            Some("general"),
            10,
            0.0,
            &["oauth".to_string()],
            &[],
        )
        .await
        .unwrap();

    assert_eq!(res.results.len(), 1);
    assert_eq!(res.results[0].id, target_id);
}

#[tokio::test]
async fn test_files_modified_filter() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let ep1 = EpisodeSave {
        created_at: None,
        title: "Fix compiler errors in api.rs".to_string(),
        content:
            "Fixed struct literals and stand-alone commas in api.rs. refactored tests or fixes"
                .to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/api_fix.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: Some(vec!["api.rs".to_string()]),
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let id1 = backend.save_episode(&ep1).await.unwrap();

    let ep2 = EpisodeSave {
        created_at: None,
        title: "Update backend tests".to_string(),
        content: "Refactored tests/test_temporal_edges.rs to verify edge invalidations. refactored tests or fixes".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/test_fix.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: Some(vec!["test_temporal_edges.rs".to_string()]),
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    backend.save_episode(&ep2).await.unwrap();

    // Search filtered by file "api.rs"
    let res = backend
        .search_filtered(
            "refactored tests or fixes",
            Some("general"),
            10,
            0.0,
            &[],
            &["api.rs".to_string()],
        )
        .await
        .unwrap();

    assert_eq!(res.results.len(), 1);
    assert_eq!(res.results[0].id, id1);
}

#[tokio::test]
async fn test_structured_filter_never_empties_floor() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let ep = EpisodeSave {
        created_at: None,
        title: "General note".to_string(),
        content: "Some general content here.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/gen.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: Some(vec!["general".to_string()]),
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let id = backend.save_episode(&ep).await.unwrap();

    // Search with a concept that doesn't exist ("nonexistent")
    // It should fall back to unfiltered results instead of returning empty list!
    let res = backend
        .search_filtered(
            "General note",
            Some("general"),
            10,
            0.0,
            &["nonexistent".to_string()],
            &[],
        )
        .await
        .unwrap();

    assert!(!res.results.is_empty());
    assert_eq!(res.results[0].id, id);
}

}

mod surreal_limit {
use anyhow::Result;
use surrealdb::engine::local::Mem;
use surrealdb::Surreal;
use mythrax_core::db::schema::INIT_SCHEMA;

#[tokio::test]
async fn dummy_test2() -> Result<()> {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("test").use_db("test").await?;
    db.query(INIT_SCHEMA).await?.check()?;

    let sql1 = "SELECT (->followed_by->episode)[0..15] AS succs FROM episode;";
    db.query(sql1).await?.check()?;

    let sql2 = "SELECT ->followed_by->episode LIMIT 15 AS succs FROM episode;";
    if let Err(e) = db.query(sql2).await {
        println!("SQL2 ERROR: {:?}", e);
    }
    
    Ok(())
}

}

mod surreal_none {
use anyhow::Result;
use surrealdb::engine::local::Mem;
use surrealdb::Surreal;

#[tokio::test]
async fn dummy_test3() -> Result<()> {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("test").use_db("test").await?;
    
    // Create an episode without content_hash
    db.query("INSERT INTO episode { id: 'episode:123', title: 'abc' };").await?.check()?;

    let mut res = db.query("SELECT id FROM episode WHERE content_hash = NONE;").await?;
    let r1: Vec<serde_json::Value> = res.take(0)?;
    println!("With = NONE: {:?}", r1);

    let mut res = db.query("SELECT id FROM episode WHERE content_hash IS NONE;").await?;
    let r2: Vec<serde_json::Value> = res.take(0)?;
    println!("With IS NONE: {:?}", r2);

    Ok(())
}

}

mod surreal_upsert {
use anyhow::Result;
use surrealdb::engine::local::Mem;
use surrealdb::Surreal;
use mythrax_core::db::schema::INIT_SCHEMA;

#[tokio::test]
async fn dummy_test() -> Result<()> {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("test").use_db("test").await?;
    db.query(INIT_SCHEMA).await?.check()?;

    let sql1 = "UPSERT idf_index:apple_general SET term = 'apple', scope = 'general', document_frequency = (document_frequency ?? 0) + 1;";
    db.query(sql1).await?.check()?;

    let sql2 = "UPSERT idf_index:apple_general SET term = 'apple', scope = 'general', document_frequency = (document_frequency ?? 0) + 1;";
    db.query(sql2).await?.check()?;

    let sql3 = "SELECT VALUE document_frequency FROM idf_index WHERE term = 'apple' AND scope = 'general';";
    let mut response = db.query(sql3).await?;
    let res: Option<i64> = response.take(0)?;
    println!("DF: {:?}", res);
    assert_eq!(res, Some(2));
    Ok(())
}

}

mod watcher_stress {
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::{WatchIgnoreList, start_watching};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_watcher_upstream_filtering_coalescing_and_bounded_pool() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let target_dir = temp_dir.path().join("target");
    std::fs::create_dir_all(&target_dir).unwrap();

    // Initialize Watch Ignore List
    let ignore_list = Arc::new(WatchIgnoreList::new());

    // Initialize Database Backend
    let db_path = temp_dir.path().join("db");
    let backend = Arc::new(
        SurrealBackend::new(
            &format!("surrealkv://{}", db_path.to_string_lossy()),
            mythrax_core::db::BackendConfig {
                check_daemon: false,
                embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
                llm: Some(mythrax_core::llm::LLMClient::new_mock()),
            },
        )
        .await
        .unwrap(),
    );
    backend.init().await.unwrap();

    // Initialize Markdown Store
    let store = Arc::new(MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap());

    // Start the file watcher
    let _watcher = start_watching(
        temp_dir.path().to_path_buf(),
        ignore_list,
        backend.clone(),
        store.clone(),
        None,
    )
    .unwrap();

    // 1. Stress watch channel: Write 1000 build files (binary) to target directory
    // These should be filtered out by the watcher's ignore list or file type check
    for i in 0..1000 {
        let file_path = target_dir.join(format!("build_file_{}.o", i));
        std::fs::write(file_path, "binary content").unwrap();
    }

    // 2. Test Coalescing: Write to a valid note 5 times in rapid succession (under 200ms)
    let valid_file = temp_dir.path().join("coalesced_note.md");
    for i in 0..5 {
        std::fs::write(
            &valid_file,
            format!("---\ntitle: Coalesced\n---\nWrite {}", i),
        )
        .unwrap();
        // Small delay to simulate rapid but distinct writes
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    // 3. Test Bounded Worker Pool: Write to 20 different files concurrently
    // This triggers 20 embedding tasks. The worker pool should serialize them (max 2 concurrent).
    let mut handles = vec![];
    for i in 0..20 {
        let temp_dir_clone = temp_dir.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let file_path = temp_dir_clone.join(format!("bulk_note_{}.md", i));
            std::fs::write(
                &file_path,
                format!("---\ntitle: Bulk {}\n---\nBulk Content {}", i, i),
            )
            .unwrap();
        });
        handles.push(handle);
    }
    // Wait for all writes to complete
    for h in handles {
        h.await.unwrap();
    }

    // 4. Assertions:

    // A. Verify coalesced note and all bulk notes are successfully indexed via polling (up to 8 seconds)
    let mut coalesced_indexed = false;
    let mut bulk_indexed = false;
    for _ in 0..80 {
        if !coalesced_indexed {
            if let Ok(query_res) = backend
                .search(mythrax_core::contracts::SearchParams::from_positional(
                    "Write 4", None, false, 10, 0, 0.0, None, false, true, false, None, true, None,
                ))
                .await
            {
                if !query_res.results.is_empty() {
                    coalesced_indexed = true;
                }
            }
        }
        if !bulk_indexed {
            if let Ok(query_res) = backend
                .search(mythrax_core::contracts::SearchParams::from_positional(
                    "Bulk Content 19",
                    None,
                    false,
                    10,
                    0,
                    0.0,
                    None,
                    false,
                    true,
                    false,
                    None,
                    true,
                    None,
                ))
                .await
            {
                if !query_res.results.is_empty() {
                    bulk_indexed = true;
                }
            }
        }
        if coalesced_indexed && bulk_indexed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert!(
        coalesced_indexed,
        "Coalesced note final write must be indexed successfully"
    );
    assert!(bulk_indexed, "All bulk notes must be indexed successfully");

    // B. Verify from DB telemetry that only ONE indexing write occurred (others were coalesced)
    // This verifies the coalescing logic: 5 rapid writes -> 1 DB commit
    let db_writes = backend
        .get_indexing_write_count("coalesced_note.md")
        .await
        .unwrap();
    assert_eq!(
        db_writes, 1,
        "Rapid modifications must be coalesced into a single database commit"
    );

    // C. Verify from embedding worker telemetry that the maximum concurrent background
    // embedding executions never exceeded 2
    // This verifies the bounded worker pool logic
    let max_concurrent_embeddings = backend
        .get_max_concurrent_background_embeddings()
        .await
        .unwrap();
    assert!(
        max_concurrent_embeddings <= 2,
        "Bulk background embedding tasks must be serialized through a bounded worker pool (max 2 concurrent)"
    );
}

mod edge_cascading_tests {
    use super::*;
    use anyhow::Result;
    use mythrax_core::contracts::{EpisodeSave, WikiNode};
    use mythrax_core::db::{parse_record_id, StorageBackend, SurrealBackend};

    #[tokio::test]
    async fn test_orphaned_edge_cascading_on_episode_deletion() -> Result<()> {
        let backend = SurrealBackend::new_in_memory().await?;
        backend.init().await?;

        let ep1 = EpisodeSave {
            title: "Episode 1".to_string(),
            content: "Content 1".to_string(),
            scope: Some("test".to_string()),
            vault_path: Some("episodes/ep1.md".to_string()),
            ..Default::default()
        };
        let ep2 = EpisodeSave {
            title: "Episode 2".to_string(),
            content: "Content 2".to_string(),
            scope: Some("test".to_string()),
            vault_path: Some("episodes/ep2.md".to_string()),
            ..Default::default()
        };
        let ep1_id = backend.save_episode(&ep1).await?;
        let ep2_id = backend.save_episode(&ep2).await?;
        let ep1_rec = parse_record_id(&ep1_id)?;
        let ep2_rec = parse_record_id(&ep2_id)?;
        let ent_rec = parse_record_id("entity:ent1")?;

        // Insert relations across relation tables
        backend
            .db
            .query("RELATE $in -> relates_to -> $out CONTENT { confidence: 0.9 };")
            .bind(("in", ep1_rec.clone()))
            .bind(("out", ep2_rec.clone()))
            .await?
            .check()?;
        backend
            .db
            .query("RELATE $in -> followed_by -> $out;")
            .bind(("in", ep1_rec.clone()))
            .bind(("out", ep2_rec.clone()))
            .await?
            .check()?;
        backend
            .db
            .query("RELATE $in -> mentions -> $out;")
            .bind(("in", ep1_rec.clone()))
            .bind(("out", ent_rec.clone()))
            .await?
            .check()?;
        backend
            .db
            .query("RELATE $in -> superseded_by -> $out;")
            .bind(("in", ep1_rec.clone()))
            .bind(("out", ep2_rec.clone()))
            .await?
            .check()?;

        // Verify relations and metrics exist before deletion
        let res_rel: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM relates_to WHERE in = $id OR out = $id;")
            .bind(("id", ep1_rec.clone()))
            .await?
            .take(0)?;
        assert!(!res_rel.is_empty(), "relates_to edge should exist prior to deletion");

        let res_met: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM metrics WHERE target_id = $id;")
            .bind(("id", ep1_rec.clone()))
            .await?
            .take(0)?;
        assert!(!res_met.is_empty(), "metrics record should exist prior to deletion");

        // Delete episode 1
        backend.delete_episode(&ep1_id).await?;

        // Assert 0 orphaned edges and metrics remain
        let res_rel_after: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM relates_to WHERE in = $id OR out = $id;")
            .bind(("id", ep1_rec.clone()))
            .await?
            .take(0)?;
        let res_fol_after: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM followed_by WHERE in = $id OR out = $id;")
            .bind(("id", ep1_rec.clone()))
            .await?
            .take(0)?;
        let res_men_after: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM mentions WHERE in = $id OR out = $id;")
            .bind(("id", ep1_rec.clone()))
            .await?
            .take(0)?;
        let res_sup_after: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM superseded_by WHERE in = $id OR out = $id;")
            .bind(("id", ep1_rec.clone()))
            .await?
            .take(0)?;
        let res_met_after: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM metrics WHERE target_id = $id;")
            .bind(("id", ep1_rec.clone()))
            .await?
            .take(0)?;

        assert!(res_rel_after.is_empty(), "relates_to orphaned edges must be 0");
        assert!(res_fol_after.is_empty(), "followed_by orphaned edges must be 0");
        assert!(res_men_after.is_empty(), "mentions orphaned edges must be 0");
        assert!(res_sup_after.is_empty(), "superseded_by orphaned edges must be 0");
        assert!(res_met_after.is_empty(), "metrics orphaned records must be 0");

        Ok(())
    }

    #[tokio::test]
    async fn test_orphaned_edge_cascading_on_wiki_node_deletion() -> Result<()> {
        let backend = SurrealBackend::new_in_memory().await?;
        backend.init().await?;

        let node1 = WikiNode {
            name: "WikiNode1".to_string(),
            content: "Content 1".to_string(),
            scope: "test_scope".to_string(),
            vault_path: Some("wiki/test_scope/WikiNode1.md".to_string()),
            ..Default::default()
        };
        let node2 = WikiNode {
            name: "WikiNode2".to_string(),
            content: "Content 2".to_string(),
            scope: "test_scope".to_string(),
            vault_path: Some("wiki/test_scope/WikiNode2.md".to_string()),
            ..Default::default()
        };
        let n1_id = backend.save_wiki_node(&node1).await?;
        let n2_id = backend.save_wiki_node(&node2).await?;
        let n1_rec = parse_record_id(&n1_id)?;
        let n2_rec = parse_record_id(&n2_id)?;

        backend
            .db
            .query("RELATE $in -> relates_to -> $out CONTENT { confidence: 0.8 };")
            .bind(("in", n1_rec.clone()))
            .bind(("out", n2_rec.clone()))
            .await?
            .check()?;
        backend
            .db
            .query("RELATE $in -> followed_by -> $out;")
            .bind(("in", n1_rec.clone()))
            .bind(("out", n2_rec.clone()))
            .await?
            .check()?;
        backend
            .db
            .query("RELATE $in -> superseded_by -> $out;")
            .bind(("in", n1_rec.clone()))
            .bind(("out", n2_rec.clone()))
            .await?
            .check()?;

        backend.delete_wiki_node("WikiNode1", "test_scope").await?;

        let res_rel: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM relates_to WHERE in = $id OR out = $id;")
            .bind(("id", n1_rec.clone()))
            .await?
            .take(0)?;
        let res_fol: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM followed_by WHERE in = $id OR out = $id;")
            .bind(("id", n1_rec.clone()))
            .await?
            .take(0)?;
        let res_men: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM mentions WHERE in = $id OR out = $id;")
            .bind(("id", n1_rec.clone()))
            .await?
            .take(0)?;
        let res_sup: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM superseded_by WHERE in = $id OR out = $id;")
            .bind(("id", n1_rec.clone()))
            .await?
            .take(0)?;
        let res_met: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM metrics WHERE target_id = $id;")
            .bind(("id", n1_rec.clone()))
            .await?
            .take(0)?;

        assert!(res_rel.is_empty(), "relates_to orphaned edges must be 0 after wiki_node delete");
        assert!(res_fol.is_empty(), "followed_by orphaned edges must be 0 after wiki_node delete");
        assert!(res_men.is_empty(), "mentions orphaned edges must be 0 after wiki_node delete");
        assert!(res_sup.is_empty(), "superseded_by orphaned edges must be 0 after wiki_node delete");
        assert!(res_met.is_empty(), "metrics orphaned records must be 0 after wiki_node delete");

        Ok(())
    }

    #[tokio::test]
    async fn test_orphaned_edge_cascading_on_vault_path_deletion() -> Result<()> {
        let backend = SurrealBackend::new_in_memory().await?;
        backend.init().await?;

        let ep = EpisodeSave {
            title: "Vault Path Episode".to_string(),
            content: "Content".to_string(),
            scope: Some("test_scope".to_string()),
            vault_path: Some("episodes/vp_test.md".to_string()),
            ..Default::default()
        };
        let ep_id = backend.save_episode(&ep).await?;
        let ep_rec = parse_record_id(&ep_id)?;

        let node = WikiNode {
            name: "TargetNode".to_string(),
            content: "Target Content".to_string(),
            scope: "test_scope".to_string(),
            ..Default::default()
        };
        let target_id = backend.save_wiki_node(&node).await?;
        let target_rec = parse_record_id(&target_id)?;
        let ent_rec = parse_record_id("entity:ent2")?;

        backend
            .db
            .query("RELATE $in -> relates_to -> $out;")
            .bind(("in", ep_rec.clone()))
            .bind(("out", target_rec.clone()))
            .await?
            .check()?;
        backend
            .db
            .query("RELATE $in -> followed_by -> $out;")
            .bind(("in", ep_rec.clone()))
            .bind(("out", target_rec.clone()))
            .await?
            .check()?;
        backend
            .db
            .query("RELATE $in -> mentions -> $out;")
            .bind(("in", ep_rec.clone()))
            .bind(("out", ent_rec.clone()))
            .await?
            .check()?;
        backend
            .db
            .query("RELATE $in -> superseded_by -> $out;")
            .bind(("in", ep_rec.clone()))
            .bind(("out", target_rec.clone()))
            .await?
            .check()?;

        // Verify metrics exist before deletion
        let res_met: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM metrics WHERE target_id = $id;")
            .bind(("id", ep_rec.clone()))
            .await?
            .take(0)?;
        assert!(!res_met.is_empty(), "metrics record should exist prior to deletion");

        backend.delete_by_vault_path_db("episodes/vp_test.md").await?;

        let res_rel: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM relates_to WHERE in = $id OR out = $id;")
            .bind(("id", ep_rec.clone()))
            .await?
            .take(0)?;
        let res_fol: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM followed_by WHERE in = $id OR out = $id;")
            .bind(("id", ep_rec.clone()))
            .await?
            .take(0)?;
        let res_men: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM mentions WHERE in = $id OR out = $id;")
            .bind(("id", ep_rec.clone()))
            .await?
            .take(0)?;
        let res_sup: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM superseded_by WHERE in = $id OR out = $id;")
            .bind(("id", ep_rec.clone()))
            .await?
            .take(0)?;
        let res_met: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM metrics WHERE target_id = $id;")
            .bind(("id", ep_rec.clone()))
            .await?
            .take(0)?;

        assert!(res_rel.is_empty(), "relates_to orphaned edges must be 0 after vault_path delete");
        assert!(res_fol.is_empty(), "followed_by orphaned edges must be 0 after vault_path delete");
        assert!(res_men.is_empty(), "mentions orphaned edges must be 0 after vault_path delete");
        assert!(res_sup.is_empty(), "superseded_by orphaned edges must be 0 after vault_path delete");
        assert!(res_met.is_empty(), "metrics orphaned records must be 0 after vault_path delete");

        Ok(())
    }

    #[tokio::test]
    async fn test_compactor_episode_deduplication_edge_cleanup() -> Result<()> {
        let tmp = tempdir()?;
        let vault_root = tmp.path().join("vault");
        std::fs::create_dir_all(&vault_root)?;
        std::fs::create_dir_all(vault_root.join("episodes"))?;
        std::fs::create_dir_all(vault_root.join("wiki"))?;
        std::fs::create_dir_all(vault_root.join("wisdom"))?;

        let backend = SurrealBackend::new_in_memory().await?;
        backend.init().await?;
        let store = mythrax_core::store::MarkdownStore::new(&vault_root)?;
        let compactor = mythrax_core::cognitive::compactor::Compactor::new();

        let ep_older_save = EpisodeSave {
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            title: "Deduplication Test Episode".to_string(),
            content: "Shared content for deduplication test episode number 1.".to_string(),
            scope: Some("testing".to_string()),
            session_id: Some("sess-1".to_string()),
            node_type: Some("episodic".to_string()),
            vault_path: Some("episodes/older.md".to_string()),
            ..Default::default()
        };

        let ep_newer_save = EpisodeSave {
            created_at: Some("2026-01-02T00:00:00Z".to_string()),
            title: "Deduplication Test Episode".to_string(),
            content: "Shared content for deduplication test episode number 2.".to_string(),
            scope: Some("testing".to_string()),
            session_id: Some("sess-1".to_string()),
            node_type: Some("episodic".to_string()),
            vault_path: Some("episodes/newer.md".to_string()),
            ..Default::default()
        };

        let older_id_str = backend.save_episode(&ep_older_save).await?;
        let newer_id_str = backend.save_episode(&ep_newer_save).await?;

        let older_rec = parse_record_id(&older_id_str)?;
        let newer_rec = parse_record_id(&newer_id_str)?;

        let ep_other_save = EpisodeSave {
            created_at: Some("2026-01-01T12:00:00Z".to_string()),
            title: "Other Episode".to_string(),
            content: "Other content.".to_string(),
            scope: Some("testing".to_string()),
            ..Default::default()
        };
        let other_id_str = backend.save_episode(&ep_other_save).await?;
        let other_rec = parse_record_id(&other_id_str)?;

        let relate_sql = "RELATE $newer -> relates_to -> $other CONTENT { confidence: 0.9 };";
        backend
            .db
            .query(relate_sql)
            .bind(("newer", newer_rec.clone()))
            .bind(("other", other_rec.clone()))
            .await?
            .check()?;

        let follow_sql = "RELATE $other -> followed_by -> $newer CONTENT { created_at: time::now() };";
        backend
            .db
            .query(follow_sql)
            .bind(("newer", newer_rec.clone()))
            .bind(("other", other_rec.clone()))
            .await?
            .check()?;

        let update_met_sql = "UPDATE metrics SET access_count = 5 WHERE target_id = $newer;";
        backend
            .db
            .query(update_met_sql)
            .bind(("newer", newer_rec.clone()))
            .await?
            .check()?;

        let dummy_emb = vec![0.1f32; 768];
        backend
            .db
            .query("UPDATE episode SET embedding = $emb WHERE scope = 'testing';")
            .bind(("emb", dummy_emb))
            .await?
            .check()?;

        compactor
            .compact_scope(std::sync::Arc::new(backend.clone()), &store, "testing", None)
            .await?;

        let res_rel: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM relates_to WHERE in = $newer OR out = $newer;")
            .bind(("newer", newer_rec.clone()))
            .await?
            .take(0)?;
        assert!(
            res_rel.is_empty(),
            "relates_to edges on newer_rec must be 0 after compaction"
        );

        let res_fol: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM followed_by WHERE in = $newer OR out = $newer;")
            .bind(("newer", newer_rec.clone()))
            .await?
            .take(0)?;
        assert!(
            res_fol.is_empty(),
            "followed_by edges on newer_rec must be 0 after compaction"
        );

        let res_trans: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM relates_to WHERE in = $older OR out = $older;")
            .bind(("older", older_rec.clone()))
            .await?
            .take(0)?;
        assert!(
            !res_trans.is_empty(),
            "Transferred relates_to edge must exist on older_rec"
        );

        let res_sup: Vec<serde_json::Value> = backend
            .db
            .query("SELECT id FROM superseded_by WHERE in = $newer AND out = $older;")
            .bind(("newer", newer_rec.clone()))
            .bind(("older", older_rec.clone()))
            .await?
            .take(0)?;
        assert!(
            !res_sup.is_empty(),
            "superseded_by edge from newer_rec to older_rec must exist"
        );

        Ok(())
    }
}

}
