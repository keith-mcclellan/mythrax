use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use anyhow::Result;
use surrealdb::engine::local::{Db, Mem};
use surrealdb::Surreal;
use tempfile::TempDir;

use mythrax_core::cognitive::arbor::{ArborCoordinator, ArborLlmClient};

#[derive(Clone)]
pub struct StmMockLlmClient {
    pub received_anchors: Arc<Mutex<Vec<String>>>,
}

impl StmMockLlmClient {
    pub fn new() -> Self {
        Self {
            received_anchors: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl ArborLlmClient for StmMockLlmClient {
    async fn propose_hypotheses(
        &self,
        _db: &dyn mythrax_core::db::StorageBackend,
        _parent_id: &str,
        _parent_hypothesis: &str,
        _target_files: &[(String, String)],
        _constraints: &[String],
        stm_anchors: &[String],
    ) -> Result<String> {
        let mut guard = self.received_anchors.lock().unwrap();
        *guard = stm_anchors.to_vec();

        Ok(r#"[
            {
                "node_id": "1",
                "hypothesis": "Optimize check range",
                "score": 90.0,
                "code_changes": {
                    "prime_calc.py": "def is_prime(n): return True"
                }
            }
        ]"#.to_string())
    }

    async fn evaluate_run(&self, _db: &dyn mythrax_core::db::StorageBackend, _run_logs: &str) -> Result<String> {
        Ok(r#"{"success": true, "score": 99.0, "insight": "success"}"#.to_string())
    }

    async fn abstract_insights(&self, _db: &dyn mythrax_core::db::StorageBackend, _parent_insight: Option<&str>, _child_insight: &str) -> Result<String> {
        Ok("insight".to_string())
    }
}

fn setup_mock_git_repo(repo_dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("init")
        .current_dir(repo_dir)
        .status()?;
    assert!(status.success());

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

    fs::write(repo_dir.join("prime_calc.py"), "def is_prime(n): return True")?;

    let status = Command::new("git")
        .args(["add", "prime_calc.py"])
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

async fn setup_surreal_schema(db: &Surreal<Db>) -> Result<()> {
    let schema = r#"
        DEFINE TABLE hypothesis_node SCHEMALESS;
        DEFINE INDEX node_id_idx ON hypothesis_node FIELDS node_id UNIQUE;
    "#;
    db.query(schema).await?.check()?;
    Ok(())
}

#[tokio::test]
async fn test_arbor_stm_grounded_ideation() -> Result<()> {
    let vault_temp = TempDir::new()?;
    let repo_temp = TempDir::new()?;

    setup_mock_git_repo(repo_temp.path())?;

    // Create the .handoffs directory inside vault_temp
    let handoffs_dir = vault_temp.path().join(".handoffs");
    fs::create_dir_all(&handoffs_dir)?;

    // Write a mock active STM anchors JSON file
    let stm_content = r#"{
        "_active_anchors": ["anchor_1", "anchor_2", "anchor_3"]
    }"#;
    fs::write(handoffs_dir.join("stm_123.json"), stm_content)?;

    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("mythrax").use_db("test").await?;
    setup_surreal_schema(&db).await?;

    let llm_client = StmMockLlmClient::new();
    let coordinator = ArborCoordinator::new(
        db.clone(),
        vault_temp.path().to_path_buf(),
        repo_temp.path().to_path_buf(),
        llm_client.clone(),
        "stm-testing".to_string(),
        "python3 prime_calc.py".to_string(),
        vec!["prime_calc.py".to_string()],
    ).await;

    coordinator.init_root("Base hypothesis".to_string(), None).await?;
    coordinator.trigger_ideation("ROOT").await?;

    // Verify that the mock client received the STM anchors from the JSON file
    let anchors = llm_client.received_anchors.lock().unwrap().clone();
    assert_eq!(anchors, vec!["anchor_1", "anchor_2", "anchor_3"]);

    Ok(())
}
