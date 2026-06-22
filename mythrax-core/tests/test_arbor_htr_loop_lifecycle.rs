use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use surrealdb::engine::local::{Db, Mem};
use surrealdb::Surreal;
use tempfile::TempDir;

use mythrax_core::contracts::HypothesisNode;
use mythrax_core::cognitive::arbor::{ArborCoordinator, ArborLlmClient};

pub struct MockLLMClient;

impl MockLLMClient {
    pub fn new() -> Self {
        Self
    }
}

impl ArborLlmClient for MockLLMClient {
    async fn propose_hypotheses(&self, _db: &dyn mythrax_core::db::StorageBackend, _parent_id: &str, _parent_hypothesis: &str) -> Result<String> {
        Ok(r#"[
            { "node_id": "1", "hypothesis": "Optimize check range", "score": 90.0 },
            { "node_id": "2", "hypothesis": "Sieve of Eratosthenes", "score": 98.0 }
        ]"#.to_string())
    }

    async fn evaluate_run(&self, _db: &dyn mythrax_core::db::StorageBackend, _run_logs: &str) -> Result<String> {
        Ok(r#"{
            "success": true,
            "score": 99.0,
            "insight": "Sieve of Eratosthenes resolves trial division bottleneck"
        }"#.to_string())
    }

    async fn abstract_insights(&self, _db: &dyn mythrax_core::db::StorageBackend, _parent_insight: Option<&str>, _child_insight: &str) -> Result<String> {
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
        DEFINE TABLE hypothesis_node SCHEMAFULL;
        DEFINE FIELD node_id ON hypothesis_node TYPE string;
        DEFINE FIELD parent_id ON hypothesis_node TYPE option<string>;
        DEFINE FIELD children_ids ON hypothesis_node TYPE array<string>;
        DEFINE FIELD depth ON hypothesis_node TYPE int;
        DEFINE FIELD hypothesis ON hypothesis_node TYPE string;
        DEFINE FIELD status ON hypothesis_node TYPE string DEFAULT 'pending';
        DEFINE FIELD score ON hypothesis_node TYPE option<float>;
        DEFINE FIELD result ON hypothesis_node TYPE option<string>;
        DEFINE FIELD insight ON hypothesis_node TYPE option<string>;
        DEFINE FIELD code_ref ON hypothesis_node TYPE option<string>;
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
    ).await;

    // ----- Step A: Initialization & Base Assessment -----
    coordinator.init_root().await?;

    // Assertion 1: ROOT node exists in SurrealDB
    let root_node: Option<HypothesisNode> = db
        .select(("hypothesis_node", "ROOT"))
        .await?;
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
    let node_1: Option<HypothesisNode> = db
        .select(("hypothesis_node", "1"))
        .await?;
    assert!(
        node_1.is_some(),
        "Step B assertion failed: Hypothesis Node 1 not found in SurrealDB"
    );
    let n1 = node_1.unwrap();
    assert_eq!(n1.status, "pending");
    assert_eq!(n1.parent_id.as_deref(), Some("ROOT"));

    let node_2: Option<HypothesisNode> = db
        .select(("hypothesis_node", "2"))
        .await?;
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
    coordinator.backpropagate_insights("2").await?;

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
        insight_text.contains("Sieve of Eratosthenes resolves trial division bottleneck") ||
        insight_text.contains("Incremental indexing optimizations"),
        "Step D assertion failed: ROOT node's insight did not contain expected critic output"
    );

    // Assertion 3: ROOT.md was rewritten containing sibling insights
    let root_md_updated_content = fs::read_to_string(&root_md_path)?;
    assert!(
        root_md_updated_content.contains("Sieve of Eratosthenes"),
        "Step D assertion failed: ROOT.md was not updated with the child insight"
    );

    // ----- Step E: Deciding & Detached Merge Gate -----
    coordinator.decide_admission("2").await?;

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
        prime_calc_final.contains("sieve") || prime_calc_final.contains("range(2, int("),
        "Step E assertion failed: prime_calc.py on the main branch was not updated with the selected optimization"
    );

    Ok(())
}
