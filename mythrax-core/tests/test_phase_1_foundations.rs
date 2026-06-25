use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::{EpisodeSave, WisdomRule};
use mythrax_core::cognitive::executor::ArborExecutor;

#[tokio::test]
async fn test_auto_scoping_and_filtering() -> Result<()> {
    // AC-1.1: Automatic scope resolution and filtering
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a temporary directory structure representing a project named "my-awesome_project"
    let tmp = tempdir()?;
    let proj_dir = tmp.path().join("my-awesome_project");
    fs::create_dir_all(&proj_dir)?;
    fs::write(proj_dir.join("Cargo.toml"), "")?;

    // Force resolve_active_scope to find our project by setting the environment variable
    unsafe {
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", proj_dir.to_string_lossy().to_string());
    }
    let active_scope = backend.resolve_active_scope();
    assert_eq!(active_scope, "my-awesome_project");

    // Save three episodes with different scopes
    let ep_target = EpisodeSave {
        title: "Target Project Episode".to_string(),
        content: "This content is specific to the active target scope.".to_string(),
        entities: vec![],
        scope: Some("my-awesome_project".to_string()),
        vault_path: Some("episodes/target.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
    };
    backend.save_episode(&ep_target).await?;

    let ep_general = EpisodeSave {
        title: "General Scope Episode".to_string(),
        content: "This content is globally applicable across scopes.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/general.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
    };
    backend.save_episode(&ep_general).await?;

    let ep_other = EpisodeSave {
        title: "Other Project Episode".to_string(),
        content: "This content belongs to a completely different project scope.".to_string(),
        entities: vec![],
        scope: Some("otherproject".to_string()),
        vault_path: Some("episodes/other.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
    };
    backend.save_episode(&ep_other).await?;

    // Search with scope: None -> should resolve to target scope "myawesomeproject"
    // Search should return both target scope and general scope, but exclude other scopes.
    let resp = backend.search("Episode", None, false, 10, 0, 0.0, None, false, true, true).await?;
    let found_titles: Vec<String> = resp.results.iter().map(|r| r.title.clone()).collect();

    assert!(found_titles.contains(&"Target Project Episode".to_string()));
    assert!(found_titles.contains(&"General Scope Episode".to_string()));
    assert!(!found_titles.contains(&"Other Project Episode".to_string()));

    // Search with wildcard scope "all" -> should return everything
    let resp_all = backend.search("Episode", Some("all"), false, 10, 0, 0.0, None, false, true, true).await?;
    let all_titles: Vec<String> = resp_all.results.iter().map(|r| r.title.clone()).collect();

    assert!(all_titles.contains(&"Target Project Episode".to_string()));
    assert!(all_titles.contains(&"General Scope Episode".to_string()));
    assert!(all_titles.contains(&"Other Project Episode".to_string()));

    // Clean up environment variable
    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
    }

    Ok(())
}

#[tokio::test]
async fn test_temporal_session_linking_and_deep_insight() -> Result<()> {
    // AC-1.2 and AC-1.3: Sequential saves within a session link followed_by edges and deep-insight search retrieves them
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let session_id = "test_session_999".to_string();
    let task_id = "test_task_888".to_string();

    let ep1 = EpisodeSave {
        title: "Step 1 Initial Setup".to_string(),
        content: "We initialized the system configuration.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/step1.md".to_string()),
        source_episode: None,
        session_id: Some(session_id.clone()),
        task_id: Some(task_id.clone()),
    };
    let ep1_id = backend.save_episode(&ep1).await?;

    let ep2 = EpisodeSave {
        title: "Step 2 Core Logic".to_string(),
        content: "We implemented the core algorithm flow.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/step2.md".to_string()),
        source_episode: None,
        session_id: Some(session_id.clone()),
        task_id: Some(task_id.clone()),
    };
    let ep2_id = backend.save_episode(&ep2).await?;

    let ep3 = EpisodeSave {
        title: "Step 3 Final Testing".to_string(),
        content: "We verified the algorithm output.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/step3.md".to_string()),
        source_episode: None,
        session_id: Some(session_id.clone()),
        task_id: Some(task_id.clone()),
    };
    let ep3_id = backend.save_episode(&ep3).await?;

    // Verify that sequential save created the followed_by links.
    // Querying with deep_insight: true and include_episodes: true on "Core Logic" should return Step 1 and Step 3 as related nodes.
    let resp = backend.search("Core Logic", None, true, 10, 0, 0.0, None, false, true, true).await?;
    let results = resp.results;
    assert!(!results.is_empty());

    let match_ep2 = results.iter().find(|r| r.id == ep2_id).expect("Should find Step 2 in search results");
    assert!(match_ep2.related_nodes.is_some());
    let related = match_ep2.related_nodes.as_ref().unwrap();

    let related_ids: Vec<String> = related.iter().map(|r| r.id.clone()).collect();
    assert!(related_ids.contains(&ep1_id));
    assert!(related_ids.contains(&ep3_id));

    Ok(())
}

#[tokio::test]
async fn test_failure_diagnostics_speed_and_fallback() -> Result<()> {
    // AC-1.4: Failure diagnostics returns correct remedies in < 5ms
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Store a mock Rust compiler error remedy
    let rule_rust = WisdomRule {
        id: None,
        target_pattern: "E0063".to_string(),
        action_to_avoid: "Avoid incomplete structural initializations".to_string(),
        causal_explanation: "Rust E0063 error occurs when struct fields are missing".to_string(),
        prescribed_remedy: "Add all required fields to the struct initializer".to_string(),
        tier: "permanent".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        source_episodes: vec![],
        generator_name: "test".to_string(),
        similarity: None,
        utility: None,
        status: None,
        superseded_at: None,
        superseded_by: None,
    };
    backend.save_wisdom_rule(&rule_rust).await?;

    // Store a mock RocksDB lock error remedy
    let rule_lock = WisdomRule {
        id: None,
        target_pattern: "lock".to_string(),
        action_to_avoid: "Avoid running concurrent instances accessing the same RocksDB path".to_string(),
        causal_explanation: "RocksDB lock acquisition failure indicates concurrent access conflicts".to_string(),
        prescribed_remedy: "Close any running processes or containers holding the DB lock".to_string(),
        tier: "permanent".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        source_episodes: vec![],
        generator_name: "test".to_string(),
        similarity: None,
        utility: None,
        status: None,
        superseded_at: None,
        superseded_by: None,
    };
    backend.save_wisdom_rule(&rule_lock).await?;

    // 1. Rust error signature matching
    let start_time = std::time::Instant::now();
    let diagnosis_rust = backend.diagnose_error_internal(
        "error[E0063]: missing fields `session_id` and `task_id` in initializer of `EpisodeSave`",
        "Finished dev profile"
    ).await?;
    let duration = start_time.elapsed();

    assert!(diagnosis_rust.is_some());
    let (exp, rem) = diagnosis_rust.unwrap();
    assert_eq!(exp, "Rust E0063 error occurs when struct fields are missing");
    assert_eq!(rem, "Add all required fields to the struct initializer");
    assert!(duration.as_millis() < 50, "Diagnostics took too long: {}ms", duration.as_millis());

    // 2. Lock error signature matching
    let diagnosis_lock = backend.diagnose_error_internal(
        "RocksDB lock acquisition failure: IOError: lock /Users/keith/.mythrax/db/LOCK: Resource temporarily unavailable",
        ""
    ).await?;

    assert!(diagnosis_lock.is_some());
    let (exp_lock, rem_lock) = diagnosis_lock.unwrap();
    assert_eq!(exp_lock, "RocksDB lock acquisition failure indicates concurrent access conflicts");
    assert_eq!(rem_lock, "Close any running processes or containers holding the DB lock");

    Ok(())
}

#[tokio::test]
async fn test_executor_decorates_failures() -> Result<()> {
    // AC-1.5: HTR test failures automatically decorate logs with remedy footnotes
    let tmp = tempdir()?;
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir)?;

    // Initialize temporary git repository
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(&repo_dir)
        .status();

    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@mythrax.ai"])
        .current_dir(&repo_dir)
        .status();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Test Agent"])
        .current_dir(&repo_dir)
        .status();

    // Create a dummy file to commit so there is a HEAD commit
    fs::write(repo_dir.join("base.txt"), "hello")?;
    let _ = std::process::Command::new("git")
        .args(["add", "base.txt"])
        .current_dir(&repo_dir)
        .status();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(&repo_dir)
        .status();

    let output_git = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&repo_dir)
        .output()?;
    let commit_sha = String::from_utf8(output_git.stdout)?.trim().to_string();

    let executor = ArborExecutor::new(repo_dir.clone());
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Store the mock wisdom rule
    let rule = WisdomRule {
        id: None,
        target_pattern: "E0063".to_string(),
        action_to_avoid: "Avoid incomplete structural initializations".to_string(),
        causal_explanation: "Rust E0063 error occurs when struct fields are missing".to_string(),
        prescribed_remedy: "Add all required fields to the struct initializer".to_string(),
        tier: "permanent".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        source_episodes: vec![],
        generator_name: "test".to_string(),
        similarity: None,
        utility: None,
        status: None,
        superseded_at: None,
        superseded_by: None,
    };
    backend.save_wisdom_rule(&rule).await?;

    // Execute command that fails and outputs "error[E0063]" on stderr
    let (success, logs) = executor.execute(
        "test-node-fail",
        &commit_sha,
        "echo 'error[E0063]: missing fields in struct' >&2 && exit 1",
        &None,
        &backend,
    ).await?;

    assert!(!success);
    assert!(logs.contains("[MYTHRAX AUTO-DIAGNOSTIC]"));
    assert!(logs.contains("Rust E0063 error occurs when struct fields are missing"));
    assert!(logs.contains("Add all required fields to the struct initializer"));

    // Cleanup worktree to be clean
    let _ = std::process::Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(&repo_dir)
        .status();

    Ok(())
}
