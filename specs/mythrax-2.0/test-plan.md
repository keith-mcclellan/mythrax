# Test Plan: Mythrax 2.0 (TDD Specification)

This document defines the complete Test-Driven Development (TDD) specification for Mythrax 2.0. All tests are automated, in-process, and must compile and pass cleanly via `cargo test`. 

To achieve 100% offline autonomy and speed, all tests utilize SurrealDB's in-memory engine (`mem://`) to guarantee clean state isolation between test runs.

All Metal/MLX-dependent integration tests are annotated with `#[cfg(feature = "mlx")]` to ensure they compile and are skipped on non-macOS CI/CD platforms.

---

## Part 1: Unit Tests

### U1: Grammar Engine Tests
*   **File:** `mythrax-core/tests/unit/grammar_tests.rs`
*   **Target:** `src/llm/grammar.rs`
*   **Purpose:** Prove that the GBNF compiler translates JSON schemas and tool structs into syntactically valid GBNF grammar strings.

```rust
use mythrax_core::llm::grammar::{json_schema_to_gbnf, tool_to_gbnf};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Serialize, Deserialize, JsonSchema)]
struct MockToolArgs {
    query: String,
    limit: usize,
    scope: Option<String>,
}

#[test]
fn test_json_schema_to_gbnf_translation() {
    let gen = schemars::gen::SchemaSettings::draft07().into_generator();
    let schema = gen.into_root_schema_for::<MockToolArgs>();
    let gbnf = json_schema_to_gbnf(&schema).expect("Failed to compile schema to GBNF");
    
    assert!(gbnf.contains("root ::= "), "GBNF must contain root definition");
    assert!(gbnf.contains("\"query\""), "GBNF must define query key rule");
    assert!(gbnf.contains("\"limit\""), "GBNF must define limit key rule");
}
```

---

### U2: RRF Fusion Core Tests
*   **File:** `mythrax-core/tests/unit/rrf_tests.rs`
*   **Target:** `src/db/backend.rs` (RRF merging function)
*   **Purpose:** Verify that Reciprocal Rank Fusion correctly merges and ranks vector similarity and full-text search result sets.

```rust
use mythrax_core::db::backend::reciprocal_rank_fusion;
use mythrax_core::contracts::SearchResult;

#[test]
fn test_rrf_scoring_and_rank_merging() {
    let vector_results = vec![
        SearchResult { id: "doc_a".to_string(), similarity: Some(0.90), content: "A".to_string(), score: None },
        SearchResult { id: "doc_b".to_string(), similarity: Some(0.80), content: "B".to_string(), score: None },
    ];
    
    let fts_results = vec![
        SearchResult { id: "doc_b".to_string(), similarity: None, content: "B".to_string(), score: Some(3.5) },
        SearchResult { id: "doc_c".to_string(), similarity: None, content: "C".to_string(), score: Some(2.1) },
    ];

    let merged = reciprocal_rank_fusion(vector_results, fts_results, 60);

    assert_eq!(merged[0].id, "doc_b");
    assert_eq!(merged[1].id, "doc_a");
    assert_eq!(merged[2].id, "doc_c");
}
```

---

### U3: Temporal Decay & History Pruning Tests
*   **File:** `mythrax-core/tests/unit/decay_tests.rs`
*   **Target:** `src/cognitive/compactor.rs` (Ebbinghaus decay formula)
*   **Purpose:** Prove that episodic memories decay exponentially based on the Ebbinghaus retention curve, and history pruning cleans records older than 30 days.

```rust
use mythrax_core::cognitive::compactor::{calculate_decay_factor, should_prune_history};
use std::time::{SystemTime, Duration};

#[test]
fn test_ebbinghaus_decay_exponential_curve() {
    let now = SystemTime::now();
    
    let decay_0 = calculate_decay_factor(now, now);
    assert!((decay_0 - 1.0).abs() < 1e-5);

    let thirty_days_ago = now - Duration::from_secs(30 * 24 * 3600);
    let decay_30 = calculate_decay_factor(now, thirty_days_ago);
    assert!((decay_30 - 0.50).abs() < 0.05);

    let ninety_days_ago = now - Duration::from_secs(90 * 24 * 3600);
    let decay_90 = calculate_decay_factor(now, ninety_days_ago);
    assert!(decay_90 < 0.15);
}

#[test]
fn test_history_pruning_threshold() {
    let now = SystemTime::now();
    
    let record_10 = now - Duration::from_secs(10 * 24 * 3600);
    assert!(!should_prune_history(now, record_10));

    let record_35 = now - Duration::from_secs(35 * 24 * 3600);
    assert!(should_prune_history(now, record_35));
}
```

---

### U4: Dynamic Threshold Elbow Finder Tests
*   **File:** `mythrax-core/tests/unit/calibration_tests.rs`
*   **Target:** `src/cognitive/synthesis.rs` (k-distance elbow finder)
*   **Purpose:** Verify the mathematical correctness of the elbow-finding algorithm, and verify YAML override configs.

```rust
use mythrax_core::cognitive::synthesis::{find_elbow_point, calibrate_epsilon_fallback};

#[test]
fn test_elbow_point_mathematical_detection() {
    let k_distances = vec![
        0.10, 0.12, 0.15, 0.18, 0.22, 0.25, 0.29, 0.35, 0.50, 0.65, 0.80, 0.95
    ];
    let elbow = find_elbow_point(&k_distances);
    assert_eq!(elbow, 0.35);
}

#[test]
fn test_elbow_calibration_insufficient_data_fallback() {
    let fallback_eps = calibrate_epsilon_fallback("nomic-embed-text-v1.5-mlx", None);
    assert_eq!(fallback_eps, 0.55);

    let user_override = Some(0.42);
    let fallback_override = calibrate_epsilon_fallback("nomic-embed-text-v1.5-mlx", user_override);
    assert_eq!(fallback_override, 0.42);
}
```

---

## Part 2: Integration Tests

### I1: Hardware Profiling E2E
*   **File:** `mythrax-core/tests/test_hardware_profiling.rs`
*   **Purpose:** Prove that on startup the daemon correctly reads host RAM and GPU specifications, and configures the `ModelProfile` without error.

```rust
use mythrax_core::daemon::profile::{detect_hardware_profile, ModelProfile};
use sysinfo::System;

#[test]
fn test_hardware_profile_auto_detection() {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    let profile = detect_hardware_profile(&sys);
    
    match profile {
        ModelProfile::Lightweight => {
            assert!(sys.total_memory() < 16 * 1024 * 1024 * 1024);
        }
        ModelProfile::Pro | ModelProfile::Max => {
            assert!(sys.total_memory() >= 16 * 1024 * 1024 * 1024);
        }
    }
}
```

---

### I2: Dynamic Model Broker, Swapping & Shader Cache Panic Fallback E2E
*   **File:** `mythrax-core/tests/test_model_broker.rs`
*   **Purpose:** Verify the 2-tiered model loading lifecycle, dynamic model selection (7B vs 35B) based on configuration, dynamic stop token resolver, pre-inference shader warm-up, and weak-pointer tracking. Additionally, prove that if the Metal shader cache is corrupted (simulated FFI panic), the warm-up sequence catches the error gracefully and falls back to CPU-only execution mode instead of crashing.
*   **Target Platform:** Apple Silicon macOS.

```rust
#[cfg(feature = "mlx")]
use mythrax_core::llm::{DynamicModelBroker, ModelTier, ModelConfig};
#[cfg(feature = "mlx")]
use tempfile::tempdir;

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_model_broker_lifecycle_and_warmup_fallback() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let broker = DynamicModelBroker::new(temp_dir.path().to_path_buf()).await.unwrap();

    // 1. Preload pinned embedding model
    broker.preload_embedding_model("mlx-community/nomic-embed-text-v1.5-mlx").await.unwrap();
    assert!(broker.is_embedding_model_loaded());

    // 2. Load the default Coder LLM (7B)
    let coder_model = broker.acquire_llm(ModelTier::Tier2).await.unwrap();
    assert_eq!(coder_model.name(), "Qwen2.5-Coder-7B-Instruct-MLX-4bit");
    
    // Verify pre-inference shader warm-up was executed
    assert!(coder_model.is_warmed_up());

    // 3. Verify dynamic stop tokens are parsed from tokenizer_config.json
    let stop_tokens = coder_model.stop_tokens();
    assert!(stop_tokens.contains(&"<|im_end|>".to_string()));

    // 4. Verify weak-pointer tracking: dropping the reference unloads the model from VRAM
    let weak_ref = broker.get_weak_llm_reference();
    drop(coder_model);
    
    broker.evict_unused_models().await;
    assert!(weak_ref.upgrade().is_none(), "Model must be evicted from VRAM when strong reference count drops to 0");

    // 5. Verify dynamic model selection: update config to 35B model and load
    broker.update_config_model("mlx-community/Qwen3.6-35B-A3B-4bit").await.unwrap();
    let model_35b = broker.acquire_llm(ModelTier::Tier2).await.unwrap();
    assert_eq!(model_35b.name(), "mlx-community/Qwen3.6-35B-A3B-4bit");
    drop(model_35b);
    broker.evict_unused_models().await;

    // 6. Simulate Metal shader cache corruption / warm-up panic
    let corrupt_broker = DynamicModelBroker::new_corrupt_mock().await.unwrap();
    let res = corrupt_broker.acquire_llm_with_warmup_fallback(ModelTier::Tier2).await;
    
    assert!(res.is_ok(), "Warmup fallback must catch shader cache panics and succeed");
    let fallback_model = res.unwrap();
    assert_eq!(fallback_model.execution_mode(), "cpu", "Must fallback to CPU execution mode");
}
```

---

### I3: Stateful HTR TDD Escalation E2E
*   **File:** `mythrax-core/tests/test_tdd_escalation.rs`
*   **Purpose:** Prove that the HTR engine successfully runs compiler tests offline, isolates target directories, composes post-mortems, and enforces the 3-5 attempt fail-safe boundary.

```rust
use mythrax_core::cognitive::arbor::{HtrEngine, TddState, TddResult};
use tempfile::tempdir;

#[tokio::test]
async fn test_stateful_tdd_compilation_escalation() {
    let temp_workspace = tempdir().expect("Failed to create temp workspace");
    let htr_engine = HtrEngine::new(temp_workspace.path().to_path_buf());

    let state = TddState {
        attempts: 0,
        max_attempts: 3,
        code_content: "fn buggy() { panic!(\"Intentional\") }".to_string(),
    };

    let result = htr_engine.execute_tdd_step(state).await.unwrap();

    match result {
        TddResult::Escalated(post_mortem) => {
            assert!(post_mortem.attempts >= 3);
            assert!(post_mortem.compacted_stderr.contains("... [Truncated for Context Protection] ..."));
            assert!(post_mortem.target_dir.to_string_lossy().contains("target/mythrax_htr_"));
        }
        _ => panic!("TDD loop must escalate after maximum failed attempts"),
    }
}
```

---

### I4: CLI Redirection, Token Persistence & Strict Security E2E
*   **File:** `mythrax-core/tests/test_cli_redirection.rs`
*   **Purpose:** Prove that `mythrax exec` spawns the target child process directly (no shell), validates Bearer token authentication, and enforces token persistence (reusing keys across daemon restarts without 401 crashes) with strict owner-only file permissions (`0600`).

```rust
use mythrax_core::cli::execute_redirection;
use reqwest::Client;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_token_persistence_reuse_and_strict_permissions() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let token_path = temp_dir.path().join("token");

    // 1. First Boot: Daemon generates token and writes it
    let daemon_mock = mythrax_core::daemon::mock::spawn_mock_daemon(8090, None, &token_path).await.unwrap();
    let generated_token = std::fs::read_to_string(&token_path).unwrap();
    
    // Assert strict Unix owner-only file permissions (0600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&token_path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600, "Token file must have strict 0600 permissions");
    }

    // 2. Kill the first daemon mock
    daemon_mock.abort();

    // 3. Second Boot (Daemon Restart): Verify daemon reads and REUSES the existing token
    let restarted_daemon_mock = mythrax_core::daemon::mock::spawn_mock_daemon(8090, Some(&generated_token), &token_path).await.unwrap();
    let current_token = std::fs::read_to_string(&token_path).unwrap();
    assert_eq!(generated_token, current_token, "Token must be reused across restarts to prevent client 401s");

    // 4. Verify client calls succeed with the persistent token
    let client = Client::new();
    let resp = client.post("http://127.0.0.1:8090/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", current_token))
        .body("{\"prompt\": \"hello\"}")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    // 5. Verify direct execution (bypassing shell command injection)
    let args = vec!["env".to_string()];
    let output = execute_redirection(args, &token_path, 8090).await.unwrap();
    assert!(output.contains(&format!("OPENAI_API_KEY={}", current_token)));
    assert!(output.contains("OPENAI_BASE_URL=http://127.0.0.1:8090/v1"));

    restarted_daemon_mock.abort();
}
```

---

### I5: Active Swap Monitor & Canonicalized Disk Check E2E
*   **File:** `mythrax-core/tests/test_swap_monitor.rs`
*   **Purpose:** Prove that the model resolver resolves symlinks and canonicalizes paths before checking partition disk space, and memory pressure triggers eviction using 3-tiered model-aware active swap thresholds.

```rust
use mythrax_core::daemon::monitor::{check_disk_space, check_memory_pressure, check_swap_pressure};
use mythrax_core::llm::ModelTier;
use tempfile::tempdir;

#[test]
fn test_canonicalized_mount_point_disk_check() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let target_dir = temp_dir.path().join("download_dir");
    std::fs::create_dir_all(&target_dir).unwrap();

    let symlink_dir = temp_dir.path().join("symlinked_dir");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_dir, &symlink_dir).unwrap();

    let massive_bytes = 10 * 1024 * 1024 * 1024 * 1024; // 10 Terabytes
    let res = check_disk_space(&symlink_dir, massive_bytes);
    assert!(res.is_err(), "Must correctly canonicalize symlink and fail disk space check on partition");
}

#[test]
fn test_model_aware_swap_eviction_thresholds() {
    // Tier 1: 1.5B (Threshold 2.0 GB)
    let evict_tier1_high = check_swap_pressure(ModelTier::Tier1, 2_100 * 1024 * 1024);
    assert!(evict_tier1_high, "Tier 1 must evict at 2.1 GB swap");
    let evict_tier1_low = check_swap_pressure(ModelTier::Tier1, 1_500 * 1024 * 1024);
    assert!(!evict_tier1_low, "Tier 1 must not evict at 1.5 GB swap");

    // Tier 2: 7B Coder (Threshold 3.0 GB)
    let evict_tier2_high = check_swap_pressure(ModelTier::Tier2, 3_100 * 1024 * 1024);
    assert!(evict_tier2_high, "Tier 2 must evict at 3.1 GB swap");
    let evict_tier2_low = check_swap_pressure(ModelTier::Tier2, 2_500 * 1024 * 1024);
    assert!(!evict_tier2_low, "Tier 2 must not evict at 2.5 GB swap");

    // Tier 3: 35B Deep Reason (Threshold 6.0 GB)
    let evict_tier3_high = check_swap_pressure(ModelTier::Tier3, 6_100 * 1024 * 1024);
    assert!(evict_tier3_high, "Tier 3 must evict at 6.1 GB swap");
    let evict_tier3_low = check_swap_pressure(ModelTier::Tier3, 5_500 * 1024 * 1024);
    assert!(!evict_tier3_low, "Tier 3 must not evict at 5.5 GB swap");
}
```

---

### I6: Stale Locks, Thread-Safe WAL, Replay Marker & Compaction E2E
*   **File:** `mythrax-core/tests/test_non_blocking_daemon.rs`
*   **Purpose:** Prove that concurrent saves to the thread-safe WAL actor are written sequentially, recovery replay is triggered ONLY on fresh boot, history triggers work, WAL replay robustly skips malformed JSON, and dreaming triggers WAL compaction to prevent indefinite file growth.

```rust
use mythrax_core::db::SurrealBackend;
use mythrax_core::contracts::{EpisodeSave, SearchResult};
use tempfile::tempdir;
use std::sync::Arc;

#[tokio::test]
async fn test_thread_safe_wal_concurrency_and_robust_replay_marker_compaction() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("db");
    let wal_path = temp_dir.path().join("episodes.jsonl");
    let initialized_marker = db_path.join(".initialized");

    // 1. Boot primary backend
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await.unwrap();
    backend.init().await.unwrap();

    assert!(initialized_marker.exists());

    // 2. Simulate concurrent saves: write 10 episodes in parallel
    let mut handles = vec![];
    for i in 0..10 {
        let backend_clone = backend.clone();
        let wal_clone = wal_path.clone();
        let handle = tokio::spawn(async move {
            let episode = EpisodeSave {
                title: format!("Concurrent Episode {}", i),
                content: format!("Content {}", i),
                entities: vec![],
                scope: "general".to_string(),
                vault_path: Some(format!("notes/note_{}.md", i)),
                source_episode: None,
                session_id: Some("test_session".to_string()),
                task_id: None,
            };
            backend_clone.save_episode_with_wal_actor(&episode, &wal_clone).await.unwrap();
        });
        handles.push(handle);
    }
    for h in handles {
        h.await.unwrap();
    }

    // Verify WAL has 10 valid lines
    let wal_content = std::fs::read_to_string(&wal_path).unwrap();
    let lines_count = wal_content.lines().count();
    assert_eq!(lines_count, 10);

    // 3. Save a duplicate update to verify WAL compaction
    let duplicate_episode = EpisodeSave {
        title: "Concurrent Episode 0".to_string(), // Duplicate ID/Title
        content: "Updated Content 0".to_string(),
        entities: vec![],
        scope: "general".to_string(),
        vault_path: Some("notes/note_0.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
    };
    backend.save_episode_with_wal_actor(&duplicate_episode, &wal_path).await.unwrap();

    // Verify raw WAL lines count is now 11
    let wal_content_2 = std::fs::read_to_string(&wal_path).unwrap();
    assert_eq!(wal_content_2.lines().count(), 11);

    // Trigger WAL Compaction (simulating background dreaming compaction)
    backend.compact_wal_file(&wal_path).await.unwrap();

    // Assert that the compacted WAL file contains exactly 10 lines (the duplicate was collapsed to its latest version)
    let compacted_content = std::fs::read_to_string(&wal_path).unwrap();
    assert_eq!(compacted_content.lines().count(), 10, "WAL compaction must collapse duplicate records to their latest state");

    // 4. Inject a corrupted / malformed line to verify robust WAL recovery parsing
    let mut wal_file = std::fs::OpenOptions::new().append(true).open(&wal_path).unwrap();
    use std::io::Write;
    writeln!(wal_file, "{{ \"title\": \"Malformed JSON\", \"content\": ").unwrap();

    // 5. Test Recovery Replay Trigger:
    // Drop backend, delete the database directory BUT keep the WAL log file
    drop(backend);
    std::fs::remove_dir_all(&db_path).unwrap();

    assert!(!initialized_marker.exists());

    // Boot fresh backend
    let recovered_backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await.unwrap();
    recovered_backend.init().await.unwrap();

    // Trigger recovery replay: must successfully replay the 10 episodes, skip the malformed line with a warning, and write the marker
    recovered_backend.replay_wal_if_fresh(&wal_path, &initialized_marker).await.unwrap();
    assert!(initialized_marker.exists());

    // Verify all 10 episodes were successfully recovered
    for i in 0..10 {
        let search_res = recovered_backend.search(&format!("Concurrent Episode {}", i), None, false, 10, 0, 0.0, None, false, true, false).await.unwrap();
        assert!(!search_res.results.is_empty(), "Episode {} must be recovered successfully", i);
    }
}
```

---

### I7: Pre-Invocation Hook & RRF Soft Threshold E2E
*   **File:** `mythrax-core/tests/test_pre_invocation_hook.rs`
*   **Purpose:** Prove that the pre-invocation hook applies a soft similarity threshold (sigmoid scaling) and injects the compliant local model status.

```rust
use mythrax_core::mcp_routes::handle_pre_invocation_hook;
use mythrax_core::db::SurrealBackend;
use mythrax_core::llm::{DynamicModelBroker, ModelTier};
use mythrax_core::api::ApiState;
use tempfile::tempdir;
use std::sync::Arc;

#[tokio::test]
async fn test_soft_thresholding_and_hook_injection() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("db");
    
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await.unwrap();
    backend.init().await.unwrap();

    let episode = mythrax_core::contracts::EpisodeSave {
        title: "Borderline Note".to_string(),
        content: "Soft threshold test content".to_string(),
        entities: vec![],
        scope: "general".to_string(),
        vault_path: Some("notes/borderline.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
    };
    backend.save_episode(&episode).await.unwrap();

    let broker = DynamicModelBroker::new_mock().await.unwrap();
    broker.preload_model(ModelTier::Tier2).await.unwrap();

    let state = ApiState {
        backend: Arc::new(backend),
        model_broker: Arc::new(broker),
    };

    let payload = serde_json::json!({
        "session_id": "test_session",
        "query": "threshold test",
        "workspace_path": temp_dir.path().to_string_lossy()
    });

    let result = handle_pre_invocation_hook(&state, payload).await.unwrap();
    let text_content = result["content"][0]["text"].as_str().unwrap();

    assert!(text_content.contains("Soft threshold test content"), "Borderline candidate must be preserved and ranked");
    assert!(text_content.contains("### 🤖 Local Inference & Model Broker Status"), "Hook must inject local model status");
}
```

---

### I8: Watcher Upstream Filtering, 500ms Coalescing & Bounded Worker Pool Stress Test
*   **File:** `mythrax-core/tests/test_watcher_stress.rs`
*   **Purpose:** Prove that writing thousands of build files to the `target/` directory does not block the notify channel, rapid modifications are debounced/coalesced (using the 500ms write-behind queue), and bulk background embedding generations are serialized through a bounded pool (max 1 or 2 concurrent) to prevent thread pool starvation.

```rust
use mythrax_core::vault::watcher::{start_watching, WatchIgnoreList};
use mythrax_core::store::MarkdownStore;
use mythrax_core::db::SurrealBackend;
use tempfile::tempdir;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_watcher_upstream_filtering_coalescing_and_bounded_pool() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let target_dir = temp_dir.path().join("target");
    std::fs::create_dir_all(&target_dir).unwrap();

    let ignore_list = Arc::new(WatchIgnoreList::new());
    
    let db_path = temp_dir.path().join("db");
    let backend = Arc::new(SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await.unwrap());
    backend.init().await.unwrap();

    let store = Arc::new(MarkdownStore::new(temp_dir.path().to_path_buf()));
    
    let _watcher = start_watching(
        temp_dir.path().to_path_buf(),
        ignore_list,
        backend.clone(),
        store,
        None
    ).unwrap();

    // 1. Stress watch channel: write 1000 build files
    for i in 0..1000 {
        let file_path = target_dir.join(format!("build_file_{}.o", i));
        std::fs::write(file_path, "binary content").unwrap();
    }

    // 2. Test Coalescing: Write to a valid note 5 times in rapid succession (under 200ms)
    let valid_file = temp_dir.path().join("coalesced_note.md");
    for i in 0..5 {
        std::fs::write(&valid_file, format!("---\ntitle: Coalesced\n---\nWrite {}", i)).unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    // 3. Test Bounded Worker Pool: Write to 20 different files concurrently to trigger 20 embedding tasks
    let mut handles = vec![];
    for i in 0..20 {
        let temp_dir_clone = temp_dir.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let file_path = temp_dir_clone.join(format!("bulk_note_{}.md", i));
            std::fs::write(&file_path, format!("---\ntitle: Bulk {}\n---\nBulk Content {}", i, i)).unwrap();
        });
        handles.push(handle);
    }
    for h in handles {
        h.await.unwrap();
    }

    // Allow time for coalescing and bounded background workers to execute sequentially
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // 4. Assertions:
    // A. Verify coalesced note was successfully indexed
    let query_res = backend.search("Write 4", None, false, 10, 0, 0.0, None, false, true, false).await.unwrap();
    assert!(!query_res.results.is_empty(), "Coalesced note final write must be indexed successfully");

    // B. Verify from DB telemetry that only ONE indexing write occurred (others were coalesced)
    let db_writes = backend.get_indexing_write_count("coalesced_note.md").await.unwrap();
    assert_eq!(db_writes, 1, "Rapid modifications must be coalesced into a single database commit");

    // C. Verify from embedding worker telemetry that the maximum concurrent background embedding executions never exceeded 2
    let max_concurrent_embeddings = backend.get_max_concurrent_background_embeddings().await.unwrap();
    assert!(max_concurrent_embeddings <= 2, "Bulk background embedding tasks must be serialized through a bounded worker pool (max 2 concurrent)");
}
```

---

### I9: Graceful Shutdown & Write Flush E2E
*   **File:** `mythrax-core/tests/test_graceful_shutdown.rs`
*   **Purpose:** Prove that when a termination signal (SIGINT/SIGTERM) is received, the daemon flushes all pending file watcher writes, evicts models and purges Metal cache, cleans up stale daemon lock files, and terminates cleanly, enforcing the 5-second timeout.

```rust
use mythrax_core::db::SurrealBackend;
use mythrax_core::llm::{DynamicModelBroker, ModelTier};
use mythrax_core::vault::watcher::{start_watching, WatchIgnoreList};
use mythrax_core::store::MarkdownStore;
use tempfile::tempdir;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_graceful_shutdown_and_flush() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let mythrax_dir = temp_dir.path().join(".mythrax");
    std::fs::create_dir_all(&mythrax_dir).unwrap();
    
    let pid_file = mythrax_dir.join("daemon.pid");
    let lock_file = mythrax_dir.join("daemon.lock");
    std::fs::write(&pid_file, "12345").unwrap();
    std::fs::write(&lock_file, "lock").unwrap();

    let db_path = temp_dir.path().join("db");
    let backend = Arc::new(SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy())).await.unwrap());
    backend.init().await.unwrap();

    let ignore_list = Arc::new(WatchIgnoreList::new());
    let store = Arc::new(MarkdownStore::new(temp_dir.path().to_path_buf()));
    
    let _watcher = start_watching(
        temp_dir.path().to_path_buf(),
        ignore_list,
        backend.clone(),
        store,
        None
    ).unwrap();

    let valid_file = temp_dir.path().join("shutdown_note.md");
    std::fs::write(&valid_file, "---\ntitle: Shutdown Note\n---\nPending commit content").unwrap();

    let broker = DynamicModelBroker::new_mock().await.unwrap();
    broker.preload_model(ModelTier::Tier2).await.unwrap();

    mythrax_core::daemon::trigger_graceful_shutdown(
        backend.clone(),
        broker.clone(),
        &pid_file,
        &lock_file
    ).await.unwrap();

    let query_res = backend.search("Pending commit content", None, false, 10, 0, 0.0, None, false, true, false).await.unwrap();
    assert!(!query_res.results.is_empty(), "Pending note must be flushed and indexed on shutdown");
    assert!(broker.get_active_model_info().is_none(), "Active model must be evicted on shutdown");
    assert!(!pid_file.exists(), "PID file must be deleted on shutdown");
    assert!(!lock_file.exists(), "Lock file must be deleted on shutdown");
}
```
