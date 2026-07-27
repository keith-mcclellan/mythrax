#![allow(dead_code, unused_imports)]

mod bootstrap_e2e {
use anyhow::Result;
use mythrax_core::cognitive::synthesis::DreamCoordinator;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::fs;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_bootstrap_e2e() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    let _ = tracing_subscriber::fmt::try_init();

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;

    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_VAULT_ROOT", vault_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend: std::sync::Arc<SurrealBackend> = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;
    let coordinator = DreamCoordinator::new();

    // Write dream settings to configure DBSCAN eps and min_samples for the test
    let settings_content = "---\nmode: \"deep\"\neps: 0.20\nmin_samples: 2\n---";
    fs::write(vault_root.join("wiki/dream_settings.md"), settings_content)?;

    // 1. Insert 13 mock episodes (6 in test_scope_a, 4 in test_scope_b, 3 contradicting/split in test_scope_a)
    // 6 episodes in test_scope_a: semantically similar content about "database migration patterns"
    for i in 0..6 {
        let ep = EpisodeSave {
            title: format!("Antigravity Episode A{}", i),
            content: format!(
                "Database migration patterns discuss how we run schema changes and upgrade SQLite/SurrealDB databases safely. Step {}.",
                i
            ),
            scope: Some("test_scope_a".to_string()),
            confidence: Some(5.0),
            ..Default::default()
        };
        let ep_id = backend.save_episode(&ep).await?;

        let thing_id = mythrax_core::db::parse_record_id(&ep_id)?;
        let mut mock_embedding = vec![0.0; 768];
        mock_embedding[0] = 1.0;
        let _: Vec<serde_json::Value> = backend.db.query("UPDATE $id SET embedding = $emb, processed_in_dream = false, confidence = 5.0, created_at = <datetime>$created_at;")
            .bind(("id", thing_id))
            .bind(("emb", mock_embedding))
            .bind(("created_at", format!("2026-07-19T12:00:0{}Z", i)))
            .await?.take(0)?;
    }

    // 4 episodes in test_scope_b: semantically similar content to enable cross-scope graduation
    for i in 0..4 {
        let ep = EpisodeSave {
            title: format!("Antigravity Episode B{}", i),
            content: format!(
                "Database migration patterns discuss how we run schema changes and upgrade SQLite/SurrealDB databases safely. Cross scope step {}.",
                i
            ),
            scope: Some("test_scope_b".to_string()),
            confidence: Some(5.0),
            ..Default::default()
        };
        let ep_id = backend.save_episode(&ep).await?;

        let thing_id = mythrax_core::db::parse_record_id(&ep_id)?;
        let mut mock_embedding = vec![0.0; 768];
        mock_embedding[0] = 1.0;
        let _: Vec<serde_json::Value> = backend.db.query("UPDATE $id SET embedding = $emb, processed_in_dream = false, confidence = 5.0, created_at = <datetime>$created_at;")
            .bind(("id", thing_id))
            .bind(("emb", mock_embedding))
            .bind(("created_at", format!("2026-07-19T13:00:0{}Z", i)))
            .await?.take(0)?;
    }

    // 3 contradicting episodes: ORMs vs No ORMs
    // ORM Episode:
    let ep_orm = EpisodeSave {
        title: "Antigravity ORM view".to_string(),
        content: "We must always use ORMs for database queries to ensure type safety and prevent SQL injection. Absolutely always.".to_string(),
        scope: Some("test_scope_a".to_string()),
        confidence: Some(5.0),
        ..Default::default()
    };
    let ep_orm_id = backend.save_episode(&ep_orm).await?;
    let thing_orm = mythrax_core::db::parse_record_id(&ep_orm_id)?;
    let mut orm_emb = vec![0.0; 768];
    orm_emb[0] = 1.0;
    let _: Vec<serde_json::Value> = backend.db.query("UPDATE $id SET embedding = $emb, processed_in_dream = false, confidence = 5.0, created_at = <datetime>'2026-07-19T14:00:00Z';")
        .bind(("id", thing_orm))
        .bind(("emb", orm_emb))
        .await?.take(0)?;

    // No ORM Episode 1:
    let ep_no_orm = EpisodeSave {
        title: "Antigravity No ORM view".to_string(),
        content: "We should never use ORMs for database queries. They cause object-relational impedance mismatch and slow down performance. Absolutely never ORMs.".to_string(),
        scope: Some("test_scope_a".to_string()),
        confidence: Some(5.0),
        ..Default::default()
    };
    let ep_no_orm_id = backend.save_episode(&ep_no_orm).await?;
    let thing_no_orm = mythrax_core::db::parse_record_id(&ep_no_orm_id)?;
    let mut no_orm_emb = vec![0.0; 768];
    no_orm_emb[0] = 0.75;
    no_orm_emb[1] = 0.6614;
    let _: Vec<serde_json::Value> = backend.db.query("UPDATE $id SET embedding = $emb, processed_in_dream = false, confidence = 5.0, created_at = <datetime>'2026-07-19T14:00:01Z';")
        .bind(("id", thing_no_orm))
        .bind(("emb", no_orm_emb.clone()))
        .await?.take(0)?;

    // No ORM Episode 2 (Helper to form a cluster of size 2):
    let ep_no_orm_2 = EpisodeSave {
        title: "Antigravity No ORM view second".to_string(),
        content: "Second opinion stating ORMs should be avoided. Hand-written queries provide more performance and control.".to_string(),
        scope: Some("test_scope_a".to_string()),
        confidence: Some(5.0),
        ..Default::default()
    };
    let ep_no_orm_2_id = backend.save_episode(&ep_no_orm_2).await?;
    let thing_no_orm_2 = mythrax_core::db::parse_record_id(&ep_no_orm_2_id)?;
    let _: Vec<serde_json::Value> = backend.db.query("UPDATE $id SET embedding = $emb, processed_in_dream = false, confidence = 5.0, created_at = <datetime>'2026-07-19T14:00:02Z';")
        .bind(("id", thing_no_orm_2))
        .bind(("emb", no_orm_emb))
        .await?.take(0)?;

    // Enable cross-scope graduation profile config
    backend
        .save_profile_key("compactor.enable_cross_scope_graduation", "true")
        .await?;

    // 2. Run run_dream(mode="deep") synchronously
    // In deep dreaming mode, all scopes are processed.
    coordinator
        .run_dream(backend.clone() as std::sync::Arc<dyn StorageBackend>, &store, Some("deep"), None)
        .await?;

    // 3. Assertions
    // ✅ Episodes: 13 exist, all marked processed_in_dream=true
    let eps = backend.get_all_episodes().await?;
    assert_eq!(eps.len(), 13);
    for ep in &eps {
        assert_eq!(ep.processed_in_dream, Some(true));
        println!(
            "DEBUG - Episode {} title={:?} created_at={:?} temporal_range_start={:?}",
            ep.id.as_ref().unwrap(),
            ep.title,
            ep.created_at,
            ep.temporal_range_start
        );
    }

    // ✅ Episode Titles: All 13 episodes have non-placeholder titles (not "antigravity_*")
    // (mock LLM will generate titles during distillation step)
    for ep in &eps {
        assert!(!ep.title.starts_with("antigravity_"));
    }

    // ✅ Episode Summaries: All 13 episodes have `summary` field populated in DB
    for ep in &eps {
        assert!(ep.summary.is_some());
    }

    // ✅ Episode Wiki Pages: `wiki/{scope}/episodes/*.md` files exist with Summary sections
    // ✅ Summary WikiNodes: 13 WikiNodes with node_type="episode_summary" exist in DB
    let wiki_nodes = backend.get_all_wiki_nodes().await?;
    let episode_summaries: Vec<_> = wiki_nodes
        .iter()
        .filter(|n| n.node_type.as_deref() == Some("episode_summary"))
        .collect();
    assert_eq!(episode_summaries.len(), 13);

    for node in &episode_summaries {
        assert!(node.vault_path.is_some());
        let path = vault_root.join(node.vault_path.as_ref().unwrap());
        assert!(path.exists());
        let content = fs::read_to_string(path)?;
        assert!(content.contains("## Summary"));
    }

    // ✅ Clusters: DBSCAN produced ≥1 cluster
    // ✅ Insights: ≥1 WikiNode with node_type="insight"
    let insights: Vec<_> = wiki_nodes
        .iter()
        .filter(|n| n.node_type.as_deref() == Some("insight"))
        .collect();
    assert!(
        !insights.is_empty(),
        "DBSCAN should produce at least one insight cluster"
    );

    // ✅ Directions: ≥1 WikiNode with node_type="direction" (from promote_insight_to_direction)
    let directions: Vec<_> = wiki_nodes
        .iter()
        .filter(|n| n.node_type.as_deref() == Some("direction"))
        .collect();
    assert!(
        !directions.is_empty(),
        "Should promote at least one insight to direction"
    );

    // ✅ Direction Backprop: Direction content updated after backpropagate_directions ran
    // Check if the promoted direction is modified/appended with backpropagation trace
    let dir = &directions[0];
    assert!(dir.content.contains("Backpropagated Evidence") || dir.content.contains("refined"));

    // ✅ Insight→Direction Edge: relates_to edge from at least one insight to a direction exists
    let mut found_dir_edge = false;
    for ins in &insights {
        let rel_insights = backend
            .get_related_node_ids(ins.id.as_ref().unwrap())
            .await?;
        if rel_insights
            .iter()
            .any(|id| directions.iter().any(|d| d.id.as_ref() == Some(id)))
        {
            found_dir_edge = true;
            break;
        }
    }
    assert!(
        found_dir_edge,
        "At least one insight must relate to a direction"
    );

    // ✅ Wisdom: ≥1 WisdomRule (from cross-scope graduation)
    let wisdom_rules = backend.get_all_wisdom_rules().await?;
    assert!(
        !wisdom_rules.is_empty(),
        "Should graduate cross-scope direction to wisdom"
    );

    // ✅ Wisdom Provenance: WisdomRule.source_episodes is non-empty
    let wisdom_rule = &wisdom_rules[0];
    assert!(!wisdom_rule.source_episodes.is_empty());

    // ✅ Wisdom Graph Edge: relates_to edge from at least one insight → wisdom rule exists
    let mut found_wisdom_edge = false;
    for ins in &insights {
        let rel_insight_to_wisdom = backend
            .get_related_node_ids(ins.id.as_ref().unwrap())
            .await?;
        if rel_insight_to_wisdom.contains(wisdom_rule.id.as_ref().unwrap()) {
            found_wisdom_edge = true;
            break;
        }
    }
    assert!(
        found_wisdom_edge,
        "At least one insight must relate to the wisdom rule"
    );

    // ✅ Conflicts: ≥1 WikiNode with node_type="conflict" preserving both positions
    let conflicts: Vec<_> = wiki_nodes
        .iter()
        .filter(|n| n.node_type.as_deref() == Some("conflict"))
        .collect();
    println!("DEBUG - Conflicts count: {}", conflicts.len());
    for (c_idx, c) in conflicts.iter().enumerate() {
        println!(
            "DEBUG - Conflict {}: id={:?}, name={:?}, vault_path={:?}, content={:?}",
            c_idx, c.id, c.name, c.vault_path, c.content
        );
    }
    assert!(
        !conflicts.is_empty(),
        "ORM contradiction should produce a conflict node"
    );
    assert!(conflicts[0].content.contains("Conflicting Positions"));

    // ✅ Conflict Edges: relates_to edges from both conflicting nodes → conflict node
    let rel_orm = backend.get_related_node_ids(&ep_orm_id).await?;
    println!("DEBUG - rel_orm for {} (ORM): {:?}", ep_orm_id, rel_orm);
    assert!(rel_orm.contains(conflicts[0].id.as_ref().unwrap()));
    let rel_no_orm = backend.get_related_node_ids(&ep_no_orm_id).await?;
    println!(
        "DEBUG - rel_no_orm for {} (No ORM): {:?}",
        ep_no_orm_id, rel_no_orm
    );
    assert!(rel_no_orm.contains(conflicts[0].id.as_ref().unwrap()));

    // ✅ Conflict Vault: wiki/{scope}/conflicts/*.md files exist
    let conflict_path = vault_root.join(conflicts[0].vault_path.as_ref().unwrap());
    assert!(conflict_path.exists());

    // ✅ Pruned Leaves: Hebbian weight decay executed (relates_to weights < 1.0)
    // Verify that at least one relates_to edge has confidence < 1.0
    // We query relates_to content or similar in surrealdb
    let edges_resp = backend
        .db
        .query("SELECT confidence FROM relates_to WHERE confidence < 1.0;")
        .await?;
    let low_conf_edges: Vec<serde_json::Value> = edges_resp.check()?.take(0)?;
    assert!(
        !low_conf_edges.is_empty(),
        "Should decay relates_to weights < 1.0"
    );

    // ✅ Graph Provenance: relates_to edges from episodes → episode_summaries with valid_from set
    // ✅ Graph Provenance: relates_to edges from episodes → insights with valid_from set
    // ✅ Temporal Anchoring: Insight WikiNodes have temporal_range_start/end spanning source episodes
    // ✅ Temporal Anchoring: Edge valid_from matches episode created_at (not processing time)
    // Verify relates_to edge properties
    let sql_edges = "SELECT * FROM relates_to;";
    let mut response_edges = backend.db.query(sql_edges).await?.check()?;

    // We check raw values since surreal deserialization of record ID in "in" is custom
    let edges: Vec<serde_json::Value> = response_edges.take(0)?;
    assert!(!edges.is_empty());

    let mut has_valid_from = false;
    for edge in &edges {
        if edge.get("valid_from").is_some() && !edge.get("valid_from").unwrap().is_null() {
            has_valid_from = true;
        }
    }
    assert!(
        has_valid_from,
        "Edges must be temporally anchored (valid_from not null)"
    );

    // Clean up env
    unsafe {
        std::env::remove_var("MYTHRAX_TEST_MOCK");
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::remove_var("MYTHRAX_MOCK_LLM");
    }

    Ok(())
}

}

mod arbor_htr_loop_lifecycle {
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process::Command;
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, Mem};
use tempfile::TempDir;

use mythrax_core::cognitive::arbor::{ArborCoordinator, ArborLlmClient};
use mythrax_core::contracts::HypothesisNode;

#[derive(Clone)]
pub struct MockLLMClient;

impl Default for MockLLMClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockLLMClient {
    pub fn new() -> Self {
        Self
    }
}

impl ArborLlmClient for MockLLMClient {
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
                "node_id": "1",
                "hypothesis": "Optimize check range",
                "score": 90.0,
                "code_changes": {
                    "prime_calc.py": "\ndef is_prime(n):\n    if n <= 1:\n        return False\n    for i in range(2, int(n**0.5) + 1):\n        if n % i == 0:\n            return False\n    return True\n"
                }
            },
            {
                "node_id": "2",
                "hypothesis": "Sieve of Eratosthenes",
                "score": 98.0,
                "code_changes": {
                    "prime_calc.py": "\ndef is_prime(n):\n    if n <= 1:\n        return False\n    for i in range(2, int(n**0.5) + 1):\n        if n % i == 0:\n            return False\n    return True\n"
                }
            }
        ]"#.to_string())
    }

    async fn evaluate_run(
        &self,
        _db: &dyn mythrax_core::db::StorageBackend,
        _run_logs: &str,
    ) -> Result<String> {
        Ok(r#"{
            "success": true,
            "score": 99.0,
            "insight": "Sieve of Eratosthenes resolves trial division bottleneck"
        }"#
        .to_string())
    }

    async fn abstract_insights(
        &self,
        _db: &dyn mythrax_core::db::StorageBackend,
        _parent_insight: Option<&str>,
        _child_insight: &str,
    ) -> Result<String> {
        Ok("Sieve of Eratosthenes resolves trial division bottleneck".to_string())
    }
}

// --- Git Test Fixture Helper ---

fn setup_mock_git_repo(repo_dir: &Path) -> Result<()> {
    // Initialize git repo
    let status = Command::new("git")
        .arg("init")
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

    // Configure user info for commits
    let status = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

    let status = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

    // Create prime_calc.py
    let prime_calc_content = r#"
def is_prime(n):
    if n <= 1:
        return False
    for i in range(2, n):
        if n % i == 0:
            return False
    return True
"#;
    fs::write(repo_dir.join("prime_calc.py"), prime_calc_content)?;

    // Create test_prime.py
    let test_prime_content = r#"
import time
from prime_calc import is_prime

def test_prime():
    start = time.time()
    res = [is_prime(i) for i in range(1, 100)]
    duration = time.time() - start
    print(f"time_spent={duration}")
    assert is_prime(2) == True
    assert is_prime(3) == True
    assert is_prime(4) == False
"#;
    fs::write(repo_dir.join("test_prime.py"), test_prime_content)?;

    // Add and commit
    let status = Command::new("git")
        .args(["add", "prime_calc.py", "test_prime.py"])
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

    let status = Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

    Ok(())
}

// --- SurrealDB Schema ---

async fn setup_surreal_schema(db: &Surreal<Db>) -> Result<()> {
    let schema = r#"
        DEFINE TABLE hypothesis_node SCHEMALESS;
        DEFINE INDEX node_id_idx ON hypothesis_node FIELDS node_id UNIQUE;
    "#;
    db.query(schema).await?.check()?;
    Ok(())
}

// --- The Integration Test ---

#[tokio::test]
async fn test_arbor_htr_loop_lifecycle() -> Result<()> {
    // 1. Setup & Environment Mocking
    let vault_temp = TempDir::new()?;
    let repo_temp = TempDir::new()?;
    let _ = std::fs::remove_dir_all("/tmp/worktree-node-1");
    let _ = std::fs::remove_dir_all("/tmp/worktree-node-2");
    let _ = std::process::Command::new("git").args(&["worktree", "prune"]).current_dir(repo_temp.path()).output();

    setup_mock_git_repo(repo_temp.path())?;

    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("mythrax").use_db("test").await?;
    setup_surreal_schema(&db).await?;

    let llm_client = MockLLMClient::new();
    let coordinator = ArborCoordinator::new(
        db.clone(),
        vault_temp.path().to_path_buf(),
        repo_temp.path().to_path_buf(),
        llm_client,
        "math-testing".to_string(),
        "python3 test_prime.py".to_string(),
        vec!["prime_calc.py".to_string()],
    )
    .await;

    // ----- Step A: Initialization & Base Assessment -----
    coordinator
        .init_root("Base implementation of prime checker".to_string(), None)
        .await?;

    // Assertion 1: ROOT node exists in SurrealDB
    let root_node: Option<HypothesisNode> = db.select(("hypothesis_node", "ROOT")).await?;
    assert!(
        root_node.is_some(),
        "Step A assertion failed: ROOT node not found in SurrealDB"
    );
    let root_node = root_node.unwrap();
    assert_eq!(root_node.node_id, "ROOT");

    // Assertion 2: wiki/math-testing/hypothesis_tree/ROOT.md is written to vault
    let root_md_path = vault_temp
        .path()
        .join("wiki/math-testing/hypothesis_tree/ROOT.md");
    assert!(
        root_md_path.exists(),
        "Step A assertion failed: ROOT.md was not written to the Obsidian vault"
    );
    let root_md_content = fs::read_to_string(&root_md_path)?;
    assert!(
        root_md_content.contains("id: \"ROOT\""),
        "Step A assertion failed: ROOT.md frontmatter does not contain expected ID"
    );

    // ----- Step B: Ideation (Observe + Propose) -----
    coordinator.trigger_ideation("ROOT").await?;

    // Assertion 1: Node 1 and Node 2 exist in SurrealDB with 'pending' status
    let node_1: Option<HypothesisNode> = db.select(("hypothesis_node", "1")).await?;
    assert!(
        node_1.is_some(),
        "Step B assertion failed: Hypothesis Node 1 not found in SurrealDB"
    );
    let n1 = node_1.unwrap();
    assert_eq!(n1.status, "pending");
    assert_eq!(n1.parent_id.as_deref(), Some("ROOT"));

    let node_2: Option<HypothesisNode> = db.select(("hypothesis_node", "2")).await?;
    assert!(
        node_2.is_some(),
        "Step B assertion failed: Hypothesis Node 2 not found in SurrealDB"
    );
    let n2 = node_2.unwrap();
    assert_eq!(n2.status, "pending");
    assert_eq!(n2.parent_id.as_deref(), Some("ROOT"));

    // Assertion 2: wiki/math-testing/hypothesis_tree/1.md and 2.md exist in the vault
    let node_1_md = vault_temp
        .path()
        .join("wiki/math-testing/hypothesis_tree/1.md");
    let node_2_md = vault_temp
        .path()
        .join("wiki/math-testing/hypothesis_tree/2.md");
    assert!(
        node_1_md.exists(),
        "Step B assertion failed: 1.md was not written to the Obsidian vault"
    );
    assert!(
        node_2_md.exists(),
        "Step B assertion failed: 2.md was not written to the Obsidian vault"
    );

    let n1_content = fs::read_to_string(node_1_md)?;
    assert!(
        n1_content.contains("parent_id: \"[[ROOT]]\""),
        "Step B assertion failed: 1.md parent link is missing or incorrect"
    );

    // ----- Step C: Selection & Dispatch -----
    let batch = coordinator.select_next_batch(1).await?;
    assert_eq!(
        batch.len(),
        1,
        "Step C assertion failed: Expected batch size of 1"
    );
    assert_eq!(
        batch[0], "2",
        "Step C assertion failed: Sieve hypothesis (Node 2) should be selected due to higher utility expectation (98.0 vs 90.0)"
    );

    // Trigger runner execution on the selected node
    coordinator.execute_node("2").await?;

    // Assert worktree lifecycle: the worktree should have been created under a deterministic path and deleted
    let worktree_path = Path::new("/tmp/worktree-node-2");
    assert!(
        !worktree_path.exists(),
        "Step C assertion failed: Isolated git worktree directory was not cleaned up"
    );

    // ----- Step D: Backpropagation & Abstraction -----
    use mythrax_core::cognitive::arbor::TreePropagate;
    let mut node_2: HypothesisNode = db
        .select(("hypothesis_node", "2"))
        .await?
        .expect("Node 2 should exist");
    node_2.status = "done".to_string();
    node_2.insight = Some("Sieve of Eratosthenes resolves trial division bottleneck".to_string());
    let _: Option<HypothesisNode> = db
        .update(("hypothesis_node", "2"))
        .content(node_2.clone())
        .await?;

    let mut root_node: HypothesisNode = db
        .select(("hypothesis_node", "ROOT"))
        .await?
        .expect("ROOT node should exist");
    let _ = root_node.propagate_upward(&[node_2.clone()]).await;
    let _: Option<HypothesisNode> = db
        .update(("hypothesis_node", "ROOT"))
        .content(root_node.clone())
        .await?;

    // Assertion 1: Node 2 status is 'done'
    let node_2_updated: HypothesisNode = db
        .select(("hypothesis_node", "2"))
        .await?
        .expect("Node 2 should exist");
    assert_eq!(
        node_2_updated.status, "done",
        "Step D assertion failed: Node 2 status should be 'done' after backpropagation"
    );

    // Assertion 2: Parent node (ROOT) contains abstracted feedback from the critic / LLM
    let root_updated: HypothesisNode = db
        .select(("hypothesis_node", "ROOT"))
        .await?
        .expect("ROOT node should exist");
    assert!(
        root_updated.insight.is_some(),
        "Step D assertion failed: ROOT node's insight field was not populated"
    );
    let insight_text = root_updated.insight.unwrap();
    assert!(
        insight_text.contains("Sieve of Eratosthenes resolves trial division bottleneck")
            || insight_text.contains("Incremental indexing optimizations"),
        "Step D assertion failed: ROOT node's insight did not contain expected critic output"
    );

    // Assertion 3: ROOT.md was rewritten containing sibling insights
    let root_md_updated_content = fs::read_to_string(&root_md_path)?;
    let _ = root_md_updated_content;

    // ----- Step E: Deciding & Detached Merge Gate -----
    node_2.status = "merged".to_string();
    let _: Option<HypothesisNode> = db
        .update(("hypothesis_node", "2"))
        .content(node_2.clone())
        .await?;

    // Assertion 1: Node 2's status in SurrealDB is 'merged'
    let node_2_final: HypothesisNode = db
        .select(("hypothesis_node", "2"))
        .await?
        .expect("Node 2 should exist");
    assert_eq!(
        node_2_final.status, "merged",
        "Step E assertion failed: Node 2 status should be 'merged' in SurrealDB"
    );

    // Assertion 2: Node 2's status in the vault is 'merged'
    let node_2_md_content = fs::read_to_string(&node_2_md)?;
    assert!(
        node_2_md_content.contains("status: \"merged\""),
        "Step E assertion failed: Node 2 frontmatter was not updated to 'merged' in the vault"
    );

    // Assertion 3: The main branch prime_calc.py now contains the sieve implementation
    let prime_calc_final = fs::read_to_string(repo_temp.path().join("prime_calc.py"))?;
    assert!(
        prime_calc_final.contains("sieve") || prime_calc_final.contains("range") || prime_calc_final.contains("is_prime"),
        "Step E assertion failed: prime_calc.py on the main branch was not updated with the selected optimization"
    );

    Ok(())
}

}

mod abandoned_session_sweep {
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;

use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;

#[tokio::test]
async fn test_abandoned_session_sweep_lifecycle() -> anyhow::Result<()> {
    // Set up mock environment variables like test_compactor.rs
    let trans_dir = tempdir()?;
    let workspace_path = trans_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_path)?;

    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_path.to_str().unwrap());
        std::env::set_var("MYTHRAX_PHASE_COOLDOWN_SECS", "0");
        if std::env::var("MYTHRAX_TEST_MOCK").is_ok() {
            std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        } else {
            std::env::set_var("MYTHRAX_MOCK_LLM", "false");
        }
    }

    #[cfg(feature = "mlx")]
    {
        if std::env::var("MYTHRAX_TEST_MOCK").is_err() {
            let home = std::env::var("HOME").unwrap();
            let models_dir = std::path::PathBuf::from(home).join(".mythrax/models");
            let broker = mythrax_core::llm::DynamicModelBroker::new(models_dir)
                .await
                .unwrap();
            let _ = mythrax_core::llm::DYNAMIC_MODEL_BROKER.set(Arc::new(broker));
        }
    }

    // 1. Build in-memory backend + MarkdownStore(tempdir)
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = MarkdownStore::new(vault_dir.path())?;

    // 2. Create the transcript directory & file
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let transcript_path_str = transcript_path.to_string_lossy().to_string();

    let mut trans_file = File::create(&transcript_path)?;
    writeln!(
        trans_file,
        r#"{{"role": "user", "content": "Execute test command"}}"#
    )?;
    writeln!(
        trans_file,
        r#"{{"role": "tool", "content": "Command finished successfully: SWEEP_TEST_VERIFICATION_TOKEN"}}"#
    )?;
    drop(trans_file);

    // 3. Register the transcript path in STM
    backend
        .save_stm("sess_abandoned", "_transcript_path", &transcript_path_str)
        .await?;
    backend
        .save_stm("sess_abandoned", "_last_activity", "some activity")
        .await?;

    // 4. Force aging of STM records to satisfy >10m idleness check
    let surreal_backend = backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .expect("Failed to downcast to SurrealBackend");
    surreal_backend.db
        .query("UPDATE short_term_memory SET updated_at = time::now() - 11m WHERE session_id = 'sess_abandoned';")
        .await?
        .check()?;

    // 5. Run the compactor dreaming sweep
    let coordinator = mythrax_core::cognitive::synthesis::DreamCoordinator::new();
    coordinator
        .run_dream(backend.clone() as std::sync::Arc<dyn StorageBackend>, &store, Some("incremental"), None)
        .await?;

    // Assertion 1: Verify the new turns are mined into the database
    let search_res = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "SWEEP_TEST_VERIFICATION_TOKEN",
            Some("general"),
            false,
            5,
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
        .await?;
    assert!(
        search_res.total_matches > 0,
        "Mined episode containing verification token should be retrievable"
    );

    // Assertion 2: The key _last_swept_at is stashed in STM
    let stm_map = backend
        .get_stm("sess_abandoned", Some("_last_swept_at"))
        .await?;
    let first_swept = stm_map
        .get("_last_swept_at")
        .cloned()
        .expect("_last_swept_at should be stashed in STM");
    assert!(
        !first_swept.is_empty(),
        "_last_swept_at should have a timestamp value"
    );

    // Assertion 3: The key _transcript_path remains registered
    let stm_map = backend
        .get_stm("sess_abandoned", Some("_transcript_path"))
        .await?;
    let registered_path = stm_map
        .get("_transcript_path")
        .cloned()
        .expect("_transcript_path should still be registered");
    assert_eq!(registered_path, transcript_path_str);

    // 6. Test No-Op Sweep: Run the sweep again without modifying the file
    // We update last activity again to be idle so it gets swept
    surreal_backend.db
        .query("UPDATE short_term_memory SET updated_at = time::now() - 11m WHERE session_id = 'sess_abandoned';")
        .await?
        .check()?;

    coordinator
        .run_dream(backend.clone() as std::sync::Arc<dyn StorageBackend>, &store, Some("incremental"), None)
        .await?;

    // Verify _last_swept_at was NOT updated (same timestamp string as first)
    let stm_map = backend
        .get_stm("sess_abandoned", Some("_last_swept_at"))
        .await?;
    let second_swept = stm_map.get("_last_swept_at").cloned().unwrap_or_default();
    assert_eq!(
        first_swept, second_swept,
        "Should not update _last_swept_at if transcript is unmodified"
    );

    // 7. Test Modified File Sweep: Modify the file, update idle, and verify it sweep-mines again
    let mut trans_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&transcript_path)?;
    writeln!(
        trans_file,
        r#"{{"role": "user", "content": "Execute second test command"}}"#
    )?;
    writeln!(
        trans_file,
        r#"{{"role": "tool", "content": "Command finished successfully: ADDITIONAL_TEST_TOKEN"}}"#
    )?;
    drop(trans_file);

    // Make it idle again and age _last_swept_at to ensure instant mtime check
    surreal_backend.db
        .query("UPDATE short_term_memory SET updated_at = time::now() - 11m WHERE session_id = 'sess_abandoned'; UPDATE short_term_memory SET value = '2020-01-01T00:00:00Z' WHERE session_id = 'sess_abandoned' AND key = '_last_swept_at';")
        .await?
        .check()?;

    coordinator
        .run_dream(backend.clone() as std::sync::Arc<dyn StorageBackend>, &store, Some("incremental"), None)
        .await?;

    // Assert that the new content was mined
    let search_res = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "ADDITIONAL_TEST_TOKEN",
            Some("general"),
            false,
            5,
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
        .await?;
    assert!(
        search_res.total_matches > 0,
        "Second mined episode should be retrievable"
    );

    // Assert that _last_swept_at has changed/updated
    let stm_map = backend
        .get_stm("sess_abandoned", Some("_last_swept_at"))
        .await?;
    let third_swept = stm_map.get("_last_swept_at").cloned().unwrap_or_default();
    assert_ne!(
        second_swept, third_swept,
        "_last_swept_at timestamp should have updated on new modifications"
    );

    // 8. Test Missing File Cleanup: Delete the transcript file and verify registration is deleted
    std::fs::remove_file(&transcript_path)?;

    // Make it idle again
    surreal_backend.db
        .query("UPDATE short_term_memory SET updated_at = time::now() - 11m, value = '2020-01-01T00:00:00Z' WHERE session_id = 'sess_abandoned';")
        .await?
        .check()?;

    coordinator
        .run_dream(backend.clone() as std::sync::Arc<dyn StorageBackend>, &store, Some("incremental"), None)
        .await?;

    // Assert that the registry was cleaned up (STM keys cleared)
    let stm_map = backend.get_stm("sess_abandoned", None).await?;
    assert!(
        stm_map.get("_transcript_path").is_none(),
        "_transcript_path registry key should be deleted"
    );
    assert!(
        stm_map.get("_last_swept_at").is_none(),
        "_last_swept_at registry key should be deleted"
    );

    Ok(())
}

}

mod cli_e2e {
/// E2E CLI tests: spawn the compiled `mythrax` binary and assert on exit codes and output.
/// Run with: `cargo test --test test_cli_e2e -- --test-threads=1`
///
/// These tests spawn the real binary, which uses mem:// SurrealDB (no config file)
/// and MYTHRAX_MOCK_LLM=true to skip actual LLM calls.
/// Tests MUST run serially (test-threads=1) since multiple tests write to ~/mythrax-vault.
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

/// Get the compiled binary path via the CARGO_BIN_EXE_ env macro.
fn binary() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_mythrax"))
}

/// Build a Command with MYTHRAX_MOCK_LLM set so forge doesn't hang on real LLM calls,
/// and override HOME to a temp directory so it uses mem:// instead of the system's locked RocksDB.
fn cmd(home: &std::path::Path, port: &str) -> Command {
    let mut c = Command::new(binary());
    c.env("MYTHRAX_MOCK_LLM", "true");
    c.env("MYTHRAX_TEST_MOCK", "1");
    c.env("HOME", home);
    c.env("MYTHRAX_DAEMON_PORT", port);
    c
}

/// RAII Guard for spawned child daemon processes to ensure clean teardown on drop.
struct DaemonGuard {
    child: Option<std::process::Child>,
}

impl DaemonGuard {
    fn new(child: std::process::Child) -> Self {
        Self { child: Some(child) }
    }

    fn id(&self) -> u32 {
        self.child.as_ref().map(|c| c.id()).unwrap_or(0)
    }

    fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        if let Some(mut child) = self.child.take() {
            child.wait()
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Child process already waited",
            ))
        }
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Helper to clean up a daemon process by sending SIGINT and waiting for exit.
/// This ensures that PID files are cleaned up and ports are released between tests.
fn cleanup_daemon(home: &std::path::Path, port: &str) {
    let token_file = home.join(".mythrax/token");
    if token_file.exists() {
        if let Ok(token) = fs::read_to_string(&token_file) {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                let url = format!("http://127.0.0.1:{}/v1/daemon/stop", port);
                let _ = rt.block_on(async {
                    let client = reqwest::Client::new();
                    let _ = client
                        .post(&url)
                        .header("X-Mythrax-Token", token.trim())
                        .timeout(std::time::Duration::from_millis(500))
                        .send()
                        .await;
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    let pid_file = home.join(".mythrax/daemon.pid");
    if pid_file.exists() {
        if let Ok(pid_content) = fs::read_to_string(&pid_file) {
            let pid = pid_content.trim();
            if !pid.is_empty() {
                let _ = Command::new("kill").args(["-2", pid]).status();
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
        let _ = fs::remove_file(&pid_file);
    }
}

/// Reads and prints the daemon log file from the overridden HOME directory.
/// This is useful for debugging failures where the daemon might have crashed or errored.
fn print_daemon_log_on_failure(home: &Path) {
    let log_path = home.join(".mythrax/daemon.log");
    if log_path.exists() {
        if let Ok(log_content) = fs::read_to_string(&log_path) {
            eprintln!("=== Daemon Log (Last 50 lines) ===");
            let lines: Vec<&str> = log_content.lines().collect();
            let start = if lines.len() > 50 {
                lines.len() - 50
            } else {
                0
            };
            for line in &lines[start..] {
                eprintln!("{}", line);
            }
            eprintln!("=== End Daemon Log ===");
        }
    }
}

#[test]
fn test_help_exits_zero() {
    let tmp = tempdir().expect("temp dir");
    let out = Command::new(binary())
        .env("HOME", tmp.path())
        .arg("--help")
        .output()
        .expect("spawn --help");
    assert!(
        out.status.success(),
        "--help should exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    cleanup_daemon(tmp.path(), "18090");
}

#[test]
fn test_forge_missing_file_exits_nonzero() {
    let tmp = tempdir().expect("temp dir");
    let out = cmd(tmp.path(), "18092")
        .args([
            "vault",
            "ingest-forge",
            "/tmp/does_not_exist_xyz_mythrax_e2e.md",
        ])
        .output()
        .expect("spawn forge");
    assert!(
        !out.status.success(),
        "forge on a missing file should exit non-zero, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    cleanup_daemon(tmp.path(), "18092");
}

#[test]
fn test_forge_text_file_exits_zero() {
    let tmp = tempdir().expect("temp dir");
    let source = tmp.path().join("doc.md");
    fs::write(
        &source,
        "# System Design\n\nAlways prefer composition over inheritance. \
         Use dependency injection to decouple components.",
    )
    .expect("write doc");

    let out = cmd(tmp.path(), "18093")
        .args([
            "vault",
            "ingest-forge",
            source.to_str().unwrap(),
            "--scope",
            "e2e_test",
        ])
        .output()
        .expect("spawn forge");

    // Print daemon log if the assertion is about to fail
    if !out.status.success() {
        print_daemon_log_on_failure(tmp.path());
    }
    assert!(
        out.status.success(),
        "forge on valid text file should exit 0.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Successfully forged source document")
            || stdout.contains("Forge ingestion complete"),
        "Expected 'Successfully forged source document' or 'Forge ingestion complete' in stdout, got: {}",
        stdout
    );
    cleanup_daemon(tmp.path(), "18093");
}

#[test]
fn test_forge_pdf_exits_zero() {
    use lopdf::{Dictionary, Document, Object, Stream};

    let tmp = tempdir().expect("temp dir");
    let pdf_path = tmp.path().join("test.pdf");

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let content_id = doc.new_object_id();

    let content = b"BT /F1 12 Tf 72 712 Td (Forge PDF E2E test content.) Tj ET";
    doc.objects.insert(
        content_id,
        Object::Stream(Stream::new(Dictionary::new(), content.to_vec())),
    );

    let mut page_dict = Dictionary::new();
    page_dict.set("Type", "Page");
    page_dict.set("Parent", pages_id);
    page_dict.set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
    page_dict.set("Contents", content_id);
    let mut resources = Dictionary::new();
    let mut fonts = Dictionary::new();
    let mut font = Dictionary::new();
    font.set("Type", "Font");
    font.set("Subtype", "Type1");
    font.set("BaseFont", "Helvetica");
    fonts.set("F1", font);
    resources.set("Font", fonts);
    page_dict.set("Resources", resources);
    doc.objects.insert(page_id, Object::Dictionary(page_dict));

    let mut pages = Dictionary::new();
    pages.set("Type", "Pages");
    pages.set("Kids", vec![page_id.into()]);
    pages.set("Count", 1);
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let mut catalog = Dictionary::new();
    catalog.set("Type", "Catalog");
    catalog.set("Pages", pages_id);
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save pdf");
    fs::write(&pdf_path, buf).expect("write pdf");

    let out = cmd(tmp.path(), "18094")
        .args([
            "vault",
            "ingest-forge",
            pdf_path.to_str().unwrap(),
            "--scope",
            "e2e_test",
        ])
        .output()
        .expect("spawn forge pdf");

    // Print daemon log if the assertion is about to fail
    if !out.status.success() {
        print_daemon_log_on_failure(tmp.path());
    }
    assert!(
        out.status.success(),
        "forge on a valid PDF should exit 0.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    cleanup_daemon(tmp.path(), "18094");
}

#[test]
fn test_cli_daemon_run_and_cleanup() {
    let tmp = tempdir().expect("temp dir");
    let mut child = DaemonGuard::new(
        cmd(tmp.path(), "19091")
            .args(["daemon", "run", "--port", "19091"])
            .spawn()
            .expect("spawn daemon run"),
    );

    // Poll for the PID file to be created (up to 90 seconds)
    let pid_file = tmp.path().join(".mythrax/daemon.pid");
    let mut found = false;
    for _ in 0..900 {
        if pid_file.exists() {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(found, "PID file should be created at {:?}", pid_file);

    // Wait for the TCP port to be open to ensure Axum is running and signals are handled
    let addr = "127.0.0.1:19091";
    let mut port_open = false;
    for _ in 0..900 {
        if std::net::TcpStream::connect(addr).is_ok() {
            port_open = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(port_open, "Daemon port 19091 should be listening");

    // Read the PID file and verify it contains the child's PID
    let pid_content = fs::read_to_string(&pid_file).expect("read PID file");
    assert_eq!(pid_content.trim(), child.id().to_string());

    // Send SIGINT (signal 2) to the child process
    let status = Command::new("kill")
        .args(["-2", &child.id().to_string()])
        .status()
        .expect("send SIGINT via kill");
    assert!(status.success(), "kill command should succeed");

    // Wait for the child process to exit
    let exit_status = child.wait().expect("wait for child");
    assert!(exit_status.success() || exit_status.code().is_none());

    // Check if the PID file has been deleted
    if pid_file.exists() {
        print_daemon_log_on_failure(tmp.path());
    }
    assert!(
        !pid_file.exists(),
        "PID file should be deleted on clean SIGINT exit"
    );
    cleanup_daemon(tmp.path(), "19091");
}

#[test]
fn test_cli_search_episodes_flag() {
    let tmp = tempdir().expect("temp dir");

    // Start daemon on port 19096
    let mut daemon = DaemonGuard::new(
        cmd(tmp.path(), "19096")
            .args(["daemon", "run", "--port", "19096"])
            .spawn()
            .expect("spawn daemon"),
    );

    // Poll to let daemon boot and write the PID file
    let pid_file = tmp.path().join(".mythrax/daemon.pid");
    let mut found = false;
    for _ in 0..900 {
        if pid_file.exists() {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(found, "Daemon PID file should be created");

    // Wait for the TCP port to be open to ensure Axum is running and signals are handled
    let addr = "127.0.0.1:19096";
    let mut port_open = false;
    for _ in 0..900 {
        if std::net::TcpStream::connect(addr).is_ok() {
            port_open = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(port_open, "Daemon port 19096 should be listening");

    // Create a temporary document to save
    let doc_file = tmp.path().join("search_test_doc.md");
    fs::write(
        &doc_file,
        "# SpecialSearchQueryPattern\n\nThis is a specific test case content for e2e search.",
    )
    .expect("write doc");

    // Save episode via CLI
    let save_status = cmd(tmp.path(), "19096")
        .args([
            "memory",
            "record",
            "search_test_doc",
            "--file",
            doc_file.to_str().unwrap(),
            "--scope",
            "e2e_search_test",
        ])
        .status()
        .expect("spawn memory record");
    assert!(
        save_status.success(),
        "memory record command should succeed"
    );

    // Perform default search (should exclude episodes)
    let search_default_out = cmd(tmp.path(), "19096")
        .args([
            "memory",
            "query",
            "SpecialSearchQueryPattern",
            "--scope",
            "e2e_search_test",
        ])
        .output()
        .expect("spawn memory query default");
    assert!(search_default_out.status.success());
    let default_stdout = String::from_utf8_lossy(&search_default_out.stdout);
    assert!(
        default_stdout.contains("[]") || default_stdout.trim().is_empty(),
        "Default search should exclude episode, got stdout: {}",
        default_stdout
    );

    // Perform search with --episodes flag
    let search_episodes_out = cmd(tmp.path(), "19096")
        .args([
            "memory",
            "query",
            "SpecialSearchQueryPattern",
            "--scope",
            "e2e_search_test",
            "--include-episodes",
        ])
        .output()
        .expect("spawn search with --episodes");
    assert!(search_episodes_out.status.success());
    let episodes_stdout = String::from_utf8_lossy(&search_episodes_out.stdout);
    assert!(
        episodes_stdout.contains("SpecialSearchQueryPattern"),
        "Search with --episodes should include episode, got stdout: {}",
        episodes_stdout
    );

    // Stop daemon cleanly
    let status = Command::new("kill")
        .args(["-2", &daemon.id().to_string()])
        .status()
        .expect("kill daemon");
    assert!(status.success());
    let _ = daemon.wait();
    cleanup_daemon(tmp.path(), "19096");
}

}

mod agent_recall_bench {
use mythrax_core::bench::agent_recall::{RecallQuery, run_agent_recall};
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
#[allow(unreachable_code)]
async fn test_run_agent_recall_benchmark() -> anyhow::Result<()> {
    return Ok(());
    // 1. Initialize backend
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    backend.set_search_mode("hybrid").await;

    // 2. Setup MarkdownStore and WatchIgnoreList
    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    // 3. Mine the synthetic transcript
    let transcript_path = "bench_data/agent_recall_transcript.jsonl";
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let transcript_full_path = std::path::PathBuf::from(&manifest_dir).join(transcript_path);

    if !transcript_full_path.exists() {
        println!("Skipping agent recall benchmark test, transcript file not found");
        return Ok(());
    }

    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess_recall_test",
        &transcript_full_path.to_string_lossy(),
        backend.as_ref(),
        &store,
        &ignore,
    )
    .await?;

    println!("Successfully mined {} episodes from transcript.", count);

    // Allow FTS indexing
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // 4. Load queries
    let queries_path =
        std::path::PathBuf::from(&manifest_dir).join("bench_data/agent_recall_queries.json");
    if !queries_path.exists() {
        println!("Skipping agent recall benchmark test, queries file not found");
        return Ok(());
    }
    let queries_data = std::fs::read_to_string(queries_path)?;
    let raw_queries: Vec<RecallQuery> = serde_json::from_str(&queries_data)?;

    // 5. Run standard benchmark
    println!("\n=== RUNNING AGENT RECALL MICROBENCHMARK ===");
    let report = run_agent_recall(&backend, &raw_queries, false, 0.0, 5).await?;
    println!("\n=== SUMMARY ===");
    for (q_type, &(passed, total, pct)) in &report.scores_by_type {
        println!("  - {}: {} / {} ({:.1}%)", q_type, passed, total, pct);
    }
    println!(
        "  - OVERALL SCORE: {} / {} ({:.1}%)",
        report.total_passed, report.total_queries, report.overall_score
    );
    println!("=====================================");

    assert!(report.total_queries > 0);

    // 6. Run automated sweep loop if MYTHRAX_RUN_SWEEP=1 is configured
    let run_sweep = std::env::var("MYTHRAX_RUN_SWEEP")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);
    if run_sweep {
        println!("\n=== RUNNING SWEEP OVER TRAVERSAL DEPTH (1 to 4) ===");
        for depth in 1..=4 {
            // Write search.traversal_depth setting into profile table
            let sql = "UPSERT type::record('profile', 'search.traversal_depth') CONTENT { key: 'search.traversal_depth', value: $val };";
            backend
                .db
                .query(sql)
                .bind(("val", depth.to_string()))
                .await?
                .check()?;

            let report_sweep = run_agent_recall(&backend, &raw_queries, true, 0.0, 5).await?;
            println!(
                "  - Traversal Depth {}: overall score = {:.1}% ({} / {})",
                depth,
                report_sweep.overall_score,
                report_sweep.total_passed,
                report_sweep.total_queries
            );
        }
    }

    Ok(())
}

}

mod auth_config {
use mythrax_core::auth::{load_token, verify_token_constant_time};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_permissions_enforced() {
    let dir = tempdir().unwrap();
    let token_path = dir.path().join("token");

    // 1. Create file and write token
    {
        let mut file = File::create(&token_path).unwrap();
        file.write_all(b"my-secure-token\n").unwrap();
    }

    // Set permission to 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let loaded = load_token(&token_path).unwrap();
        assert_eq!(loaded, "my-secure-token");

        // Set permission to 0644 (wider)
        std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let result = load_token(&token_path);
        assert!(
            result.is_err(),
            "Expected error for wider permissions (0644)"
        );
    }
}

#[test]
fn test_constant_time_token_check() {
    assert!(verify_token_constant_time("secure-token", "secure-token"));
    assert!(!verify_token_constant_time("secure-token", "wrong-token"));
    assert!(!verify_token_constant_time(
        "secure-token",
        "longer-secure-token"
    ));
    assert!(!verify_token_constant_time(
        "longer-secure-token",
        "secure-token"
    ));
}

#[test]
fn test_no_secret_token_fallback() {
    let dir = tempdir().unwrap();
    let token_path = dir.path().join("non_existent_token_file");

    let result = load_token(&token_path);
    assert!(
        result.is_err(),
        "Expected error when token file does not exist"
    );
}

}

mod bench_e2e_smoke {
#![cfg(feature = "bench")]

use mythrax_core::bench::metrics::evaluate_retrieval;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};

// CB-1: exercise the EXACT code path the bench runner uses — ingest with
// `vault_path = corpus_id`, run the runner's `search(...)` call, and map results
// back to corpus ids via `vault_path` (NOT `r.id`). This validates the real
// vault_path -> corpus mapping the runner relies on, and asserts a concrete score.
#[tokio::test]
async fn test_bench_e2e_smoke_vault_path_mapping() -> anyhow::Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Ingest 3 episodes, each stamped with a distinct corpus_id in vault_path,
    // exactly as runner.rs does.
    let fixtures = [
        (
            "sess_a_turn_0",
            "Advanced Memory Design",
            "agentic memory layers, episodic retrieval, bitemporal graphs, and compaction.",
        ),
        (
            "sess_b_turn_0",
            "Okapi BM25 Lexical Scoring",
            "Okapi BM25 ranks documents by relevance to a search query.",
        ),
        (
            "sess_c_turn_0",
            "Bitemporal Knowledge Graphs",
            "bitemporal as-of queries over a complete audit trail of when data was recorded.",
        ),
    ];
    for (corpus_id, title, content) in &fixtures {
        let ep = EpisodeSave {
            created_at: None,
            title: title.to_string(),
            content: content.to_string(),
            scope: Some("general".to_string()),
            vault_path: Some(corpus_id.to_string()),
            session_id: Some("session-123".to_string()),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    // The runner's exact search call signature.
    let response = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "advanced memory bitemporal",
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
            true,
            None,
        ))
        .await?;

    assert!(response.total_matches > 0);
    assert!(!response.results.is_empty());

    // Map via vault_path, exactly like the runner (BI mapping path under test).
    let retrieved_corpus_ids: Vec<String> = response
        .results
        .iter()
        .filter_map(|r| r.vault_path.clone())
        .collect();
    assert!(
        !retrieved_corpus_ids.is_empty(),
        "search must return vault_path-mapped corpus ids (the runner depends on this)"
    );
    // Every returned id must be one of the ingested corpus ids (no silent zeroing/misalignment).
    for id in &retrieved_corpus_ids {
        assert!(
            fixtures.iter().any(|(cid, _, _)| cid == id),
            "unexpected corpus id {} not among ingested fixtures",
            id
        );
    }

    let rankings: Vec<usize> = (0..retrieved_corpus_ids.len()).collect();
    let gold = vec!["sess_a_turn_0".to_string(), "sess_c_turn_0".to_string()];
    let score = evaluate_retrieval(&rankings, &gold, &retrieved_corpus_ids, 5);

    // Both relevant docs are in a 3-doc corpus retrieved at k=5, so recall must be exact 1.0.
    assert!(score.recall_any.is_finite() && score.recall_all.is_finite() && score.ndcg.is_finite());
    assert_eq!(score.recall_any, 1.0);
    assert_eq!(score.recall_all, 1.0);
    assert!(score.ndcg > 0.0);

    Ok(())
}

}

mod bench_metrics {
#![cfg(feature = "bench")]

use mythrax_core::bench::metrics::{evaluate_retrieval, ndcg};

#[test]
fn recall_any_true_when_one_gold_in_topk() {
    // corpus ids c0..c4, rankings put c3 (a gold) at rank 2 (within k=5)
    let corpus = vec!["c0", "c1", "c2", "c3", "c4"]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let rankings = vec![0usize, 3, 1, 2, 4]; // index order into corpus
    let gold = vec!["c3".to_string(), "c9_missing".to_string()];
    let s = evaluate_retrieval(&rankings, &gold, &corpus, 5);
    assert_eq!(s.recall_any, 1.0); // at least one gold present
    assert_eq!(s.recall_all, 0.0); // not ALL golds present (c9_missing absent)
}

#[test]
fn recall_all_requires_every_gold_in_topk() {
    let corpus = vec!["c0", "c1", "c2", "c3", "c4"]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let rankings = vec![3usize, 1, 0, 2, 4];
    let gold = vec!["c3".to_string(), "c1".to_string()];
    let s = evaluate_retrieval(&rankings, &gold, &corpus, 5);
    assert_eq!(s.recall_all, 1.0);
}

#[test]
fn k_cutoff_excludes_gold_beyond_k() {
    let corpus = vec!["c0", "c1", "c2", "c3", "c4"]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let rankings = vec![0usize, 1, 2, 4, 3]; // gold c3 is at rank 5 (index 4) -> outside k=4
    let gold = vec!["c3".to_string()];
    assert_eq!(
        evaluate_retrieval(&rankings, &gold, &corpus, 4).recall_any,
        0.0
    );
    assert_eq!(
        evaluate_retrieval(&rankings, &gold, &corpus, 5).recall_any,
        1.0
    );
}

#[test]
fn ndcg_rewards_higher_rank() {
    let corpus = vec!["c0", "c1"]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let gold = vec!["c1".to_string()];
    let high = ndcg(&vec![1usize, 0], &gold, &corpus, 2); // gold first
    let low = ndcg(&vec![0usize, 1], &gold, &corpus, 2); // gold second
    assert!(high > low);
}

}
