use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::vault::ingestion::bulk_ingest_vault;

#[tokio::test]
async fn test_rocksdb_connection_and_persistence() -> Result<()> {
    let tmp = tempdir()?;
    let db_path = tmp.path().join("db");
    let surreal_url = format!("rocksdb://{}", db_path.to_string_lossy());

    // 1. Initialize persistent backend
    let backend = SurrealBackend::new(&surreal_url).await?;
    backend.init().await?;

    // 2. Save an episode
    let ep = EpisodeSave {
        title: "Persistence Test".to_string(),
        content: "Verifying persistent storage in rocksdb.".to_string(),
        entities: vec![],
        scope: Some("testing".to_string()),
        vault_path: Some("episodes/persist_test.md".to_string()),
        source_episode: None,
    };
    let ep_id = backend.save_episode(&ep).await?;
    assert!(ep_id.contains("episode:"));

    // 3. Drop connection and reconnect
    drop(backend);
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let backend2 = SurrealBackend::new(&surreal_url).await?;
    backend2.init().await?;

    // 4. Retrieve saved episode and assert it exists
    let all_eps = backend2.get_all_episodes().await?;
    assert_eq!(all_eps.len(), 1);
    assert_eq!(all_eps[0].title, "Persistence Test");
    assert_eq!(all_eps[0].content, "Verifying persistent storage in rocksdb.");

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
    let backend = SurrealBackend::new(&surreal_url).await?;
    backend.init().await?;

    // Create a mock Antigravity transcript structure
    let session_dir = source_dir.join("session_123");
    let logs_dir = session_dir.join(".system_generated/logs");
    fs::create_dir_all(&logs_dir)?;
    
    let transcript_content = r#"{"type":"USER_INPUT","content":"Please write a function to search."}
{"type":"PLANNER_RESPONSE","content":"I will write a grep search helper."}"#;
    fs::write(logs_dir.join("transcript.jsonl"), transcript_content)?;

    // Run bulk ingestion for Antigravity
    let (count, errors) = bulk_ingest_vault(
        &vault_root,
        &source_dir,
        "antigravity",
        "antigravity-scope",
        &backend
    ).await?;

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
        title: "Stub note".to_string(),
        content: "Some dummy content.".to_string(),
        entities: vec![],
        scope: Some("reprocess-test".to_string()),
        vault_path: Some("episodes/stub.md".to_string()),
        source_episode: None,
    };
    let stub_id = backend.save_episode(&save_stub).await?;
    
    // Explicitly update db to clear its embedding to simulate missing models
    let record_id = mythrax_core::db::parse_record_id(&stub_id)?;
    let _ = backend.db.query("UPDATE $id SET embedding = NONE;")
        .bind(("id", record_id))
        .await?.check()?;

    let ep_before = backend.get_all_episodes().await?
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
                title: ep.title.clone(),
                content: ep.content.clone(),
                entities: vec![],
                scope: ep.scope.clone(),
                vault_path: ep.vault_path.clone(),
                source_episode: ep.source_episode.clone(),
            };
            backend.save_episode(&s).await?;
            reprocess_count += 1;
        }
    }

    assert_eq!(reprocess_count, 1);

    // Verify embedding generated (or remains None if models are missing, but connection doesn't crash)
    let ep_after = backend.get_all_episodes().await?
        .into_iter()
        .find(|e| e.id.as_ref() == Some(&stub_id))
        .unwrap();
    
    if backend.embedder.is_some() {
        assert!(ep_after.embedding.is_some());
        assert_eq!(ep_after.embedding.unwrap().len(), 768);
    }

    Ok(())
}

#[test]
fn test_executor_applies_code_changes() -> Result<()> {
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

    // Create a base file
    fs::write(repo_dir.join("base.txt"), "hello world")?;
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
    code_changes.insert("src/calc.rs".to_string(), "pub fn add(a: i32, b: i32) -> i32 { a + b }".to_string());

    // Execute test command
    let (success, logs) = executor.execute(
        "test-node",
        &commit_sha,
        "cat src/calc.rs",
        &Some(code_changes),
    )?;

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
    let folders = ["episodes", "wiki", "wiki/artifacts", "wisdom", "general", "archive"];
    for f in &folders {
        fs::create_dir_all(vault_root.join(f))?;
    }

    let surreal_url = format!("rocksdb://{}", db_path.to_string_lossy());
    let backend = SurrealBackend::new(&surreal_url).await?;
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
    fs::write(session_dir.join("walkthrough.md"), "Walkthrough artifact content")?;
    fs::write(session_dir.join("implementation_plan.md"), "Plan artifact content")?;

    // Run bulk ingestion
    let (count, errors) = bulk_ingest_vault(
        &vault_root,
        &source_dir,
        "antigravity",
        "testing-linking-scope",
        &backend
    ).await?;

    // We ingested 2 episode parts + 2 artifacts = 4 success counts
    assert_eq!(count, 4);
    assert!(errors.is_empty());

    // 1. Verify episodes in DB
    let all_eps = backend.get_all_episodes().await?;
    // We should have part 1 and part 2
    assert_eq!(all_eps.len(), 2);
    
    let ep_part1 = all_eps.iter().find(|e| e.title.contains("part1")).unwrap();
    let ep_part2 = all_eps.iter().find(|e| e.title.contains("part2")).unwrap();
    
    // 2. Verify links inside files in Obsidian
    let ep_part1_file = fs::read_to_string(vault_root.join(ep_part1.vault_path.as_ref().unwrap()))?;
    assert!(ep_part1_file.contains("[[wiki/artifacts/session_linking_123/walkthrough]]"));
    assert!(ep_part1_file.contains("[[wiki/artifacts/session_linking_123/implementation_plan]]"));

    let ep_part2_file = fs::read_to_string(vault_root.join(ep_part2.vault_path.as_ref().unwrap()))?;
    assert!(ep_part2_file.contains("[[wiki/artifacts/session_linking_123/walkthrough]]"));
    assert!(ep_part2_file.contains("[[wiki/artifacts/session_linking_123/implementation_plan]]"));

    // Verify artifact file backlinks
    let walkthrough_rel_path = "wiki/artifacts/session_linking_123/walkthrough.md";
    let walkthrough_file = fs::read_to_string(vault_root.join(walkthrough_rel_path))?;
    assert!(walkthrough_file.contains("Source Episodes:"));
    assert!(walkthrough_file.contains(&ep_part1.title));
    assert!(walkthrough_file.contains(&ep_part2.title));

    // 3. Verify graph relationships in SurrealDB
    let ep1_related = backend.get_related_node_ids(ep_part1.id.as_ref().unwrap()).await?;
    assert_eq!(ep1_related.len(), 2); // walkthrough & implementation_plan
    
    let ep2_related = backend.get_related_node_ids(ep_part2.id.as_ref().unwrap()).await?;
    assert_eq!(ep2_related.len(), 2);

    Ok(())
}

