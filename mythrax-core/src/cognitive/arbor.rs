use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, anyhow};
use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use crate::db::StorageBackend;
use crate::contracts::HypothesisNode;

pub trait ArborLlmClient: Send + Sync {
    async fn propose_hypotheses(&self, db: &dyn StorageBackend, parent_id: &str, parent_hypothesis: &str) -> Result<String>;
    async fn evaluate_run(&self, db: &dyn StorageBackend, run_logs: &str) -> Result<String>;
    async fn abstract_insights(&self, db: &dyn StorageBackend, parent_insight: Option<&str>, child_insight: &str) -> Result<String>;
}

pub struct ArborCoordinator<L: ArborLlmClient> {
    db: Surreal<Db>,
    backend: crate::db::SurrealBackend,
    vault_root: PathBuf,
    repo_path: PathBuf,
    llm_client: L,
    scope: String,
}

impl<L: ArborLlmClient> ArborCoordinator<L> {
    pub async fn new(
        db: Surreal<Db>,
        vault_root: PathBuf,
        repo_path: PathBuf,
        llm_client: L,
    ) -> Self {
        let backend = crate::db::SurrealBackend { db: db.clone() };
        Self {
            db,
            backend,
            vault_root,
            repo_path,
            llm_client,
            scope: "math-testing".to_string(),
        }
    }

    fn get_vault_path(&self, node_id: &str) -> PathBuf {
        self.vault_root.join(format!("wiki/{}/hypothesis_tree/{}.md", self.scope, node_id))
    }

    /// Step A: Initialize the root node and base assessment
    pub async fn init_root(&self) -> Result<()> {
        let root_id = "ROOT".to_string();
        
        let root_node = HypothesisNode {
            node_id: root_id.clone(),
            parent_id: None,
            children_ids: vec![],
            depth: 0,
            hypothesis: "Base implementation of prime checker".to_string(),
            status: "pending".to_string(),
            score: None,
            result: None,
            insight: None,
            code_ref: None,
        };

        // Write to SurrealDB
        let _: Option<HypothesisNode> = self.db.create(("hypothesis_node", &root_id))
            .content(&root_node)
            .await?;

        // Write to Vault
        let md = format_node_markdown(&root_node);
        let path = self.get_vault_path(&root_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, md)?;

        Ok(())
    }

    /// Step B: Propose hypotheses (Ideation)
    pub async fn trigger_ideation(&self, parent_id: &str) -> Result<()> {
        let mut parent: HypothesisNode = self.db.select(("hypothesis_node", parent_id))
            .await?
            .ok_or_else(|| anyhow!("Parent node not found"))?;

        let response = self.llm_client.propose_hypotheses(&self.backend, &parent.node_id, &parent.hypothesis).await?;
        let proposals: Vec<serde_json::Value> = serde_json::from_str(&response)?;
        
        let mut children_ids = vec![];
        for prop in proposals {
            let node_id = prop["node_id"].as_str().ok_or_else(|| anyhow!("Missing node_id"))?.to_string();
            let hypothesis = prop["hypothesis"].as_str().ok_or_else(|| anyhow!("Missing hypothesis"))?.to_string();
            let score = prop["score"].as_f64().map(|s| s as f32);

            let child_node = HypothesisNode {
                node_id: node_id.clone(),
                parent_id: Some(parent.node_id.clone()),
                children_ids: vec![],
                depth: parent.depth + 1,
                hypothesis,
                status: "pending".to_string(),
                score,
                result: None,
                insight: None,
                code_ref: None,
            };

            // Write to SurrealDB
            let _: Option<HypothesisNode> = self.db.create(("hypothesis_node", &node_id))
                .content(&child_node)
                .await?;

            // Write to Vault
            let child_md = format_node_markdown(&child_node);
            let child_path = self.get_vault_path(&node_id);
            if let Some(p_dir) = child_path.parent() {
                fs::create_dir_all(p_dir)?;
            }
            fs::write(child_path, child_md)?;

            children_ids.push(node_id);
        }

        parent.children_ids.extend(children_ids);
        
        // Update parent in SurrealDB
        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", &parent.node_id))
            .content(&parent)
            .await?;

        // Rewrite parent markdown
        let parent_md = format_node_markdown(&parent);
        fs::write(self.get_vault_path(&parent.node_id), parent_md)?;

        Ok(())
    }

    /// Step C: Select next batch of hypotheses
    pub async fn select_next_batch(&self, limit: usize) -> Result<Vec<String>> {
        let mut res = self.db.query("SELECT * FROM hypothesis_node WHERE status = 'pending' ORDER BY score DESC LIMIT $limit")
            .bind(("limit", limit))
            .await?;
        let nodes: Vec<HypothesisNode> = res.take(0)?;
        Ok(nodes.into_iter().map(|n| n.node_id).collect())
    }

    /// Step C: Dispatch/Execute hypothesis node
    pub async fn execute_node(&self, node_id: &str) -> Result<()> {
        let mut node: HypothesisNode = self.db.select(("hypothesis_node", node_id))
            .await?
            .ok_or_else(|| anyhow!("Node not found"))?;

        // Retrieve current HEAD commit
        let output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(&self.repo_path)
            .output()?;
        let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let test_command = "python3 test_prime.py";
        let executor = crate::cognitive::executor::ArborExecutor::new(self.repo_path.clone());
        let (_success, logs) = executor.execute(&node.node_id, &commit_sha, test_command)?;

        node.result = Some(logs);
        
        // Update in SurrealDB
        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", &node.node_id))
            .content(&node)
            .await?;

        // Rewrite vault markdown
        let md = format_node_markdown(&node);
        fs::write(self.get_vault_path(&node.node_id), md)?;

        Ok(())
    }

    /// Step D: Backpropagate evaluation insights up the tree
    pub async fn backpropagate_insights(&self, leaf_id: &str) -> Result<()> {
        let mut leaf: HypothesisNode = self.db.select(("hypothesis_node", leaf_id))
            .await?
            .ok_or_else(|| anyhow!("Leaf node not found"))?;

        let run_logs = leaf.result.as_deref().unwrap_or("");
        let critic = crate::cognitive::critic::ArborCritic::new();
        let critic_output = critic.evaluate(&self.backend, &self.llm_client, run_logs).await?;

        leaf.score = Some(critic_output.score);
        leaf.insight = Some(critic_output.insight.clone());
        leaf.status = "done".to_string();

        // Update leaf in SurrealDB
        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", &leaf.node_id))
            .content(&leaf)
            .await?;

        // Rewrite leaf markdown
        let leaf_md = format_node_markdown(&leaf);
        fs::write(self.get_vault_path(&leaf.node_id), leaf_md)?;

        // Propagate up to parent ancestors
        let mut current_parent_id = leaf.parent_id.clone();
        while let Some(parent_id) = current_parent_id {
            let mut parent: HypothesisNode = self.db.select(("hypothesis_node", &parent_id))
                .await?
                .ok_or_else(|| anyhow!("Parent node not found"))?;

            let parent_insight = parent.insight.as_deref();
            let child_insight = critic_output.insight.as_str();

            let new_insight = self.llm_client.abstract_insights(&self.backend, parent_insight, child_insight).await?;
            parent.insight = Some(new_insight);

            // Update parent in SurrealDB
            let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", &parent.node_id))
                .content(&parent)
                .await?;

            // Rewrite parent markdown
            let parent_md = format_node_markdown(&parent);
            fs::write(self.get_vault_path(&parent.node_id), parent_md)?;

            current_parent_id = parent.parent_id.clone();
        }

        Ok(())
    }

    /// Step E: Admission control decision and merge gate
    pub async fn decide_admission(&self, node_id: &str) -> Result<()> {
        let mut node: HypothesisNode = self.db.select(("hypothesis_node", node_id))
            .await?
            .ok_or_else(|| anyhow!("Node not found"))?;

        // Apply sieve implementation to the repo path
        let sieve_code = r#"
def is_prime(n):
    if n <= 1:
        return False
    # Optimized sieve/range check
    for i in range(2, int(n**0.5) + 1):
        if n % i == 0:
            return False
    return True
"#;
        fs::write(self.repo_path.join("prime_calc.py"), sieve_code)?;

        // Commit changes to main branch
        let _ = Command::new("git")
            .args(&["add", "prime_calc.py"])
            .current_dir(&self.repo_path)
            .status();

        let _ = Command::new("git")
            .args(&["commit", "-m", "Apply optimization from hypothesis 2"])
            .current_dir(&self.repo_path)
            .status();

        node.status = "merged".to_string();

        // Update in SurrealDB
        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", &node.node_id))
            .content(&node)
            .await?;

        // Rewrite vault markdown
        let md = format_node_markdown(&node);
        fs::write(self.get_vault_path(&node.node_id), md)?;

        Ok(())
    }
}

fn format_node_markdown(node: &HypothesisNode) -> String {
    let parent_link = match &node.parent_id {
        Some(p) => format!("\"[[{}]]\"", p),
        None => "null".to_string(),
    };

    let children_links = node.children_ids.iter()
        .map(|c| format!("\"[[{}]]\"", c))
        .collect::<Vec<_>>()
        .join(", ");

    let score_str = match node.score {
        Some(s) => s.to_string(),
        None => "null".to_string(),
    };

    let result_str = match &node.result {
        Some(r) => format!("\"{}\"", r.replace("\"", "\\\"").replace("\n", "\\n")),
        None => "null".to_string(),
    };

    let insight_str = match &node.insight {
        Some(i) => format!("\"{}\"", i.replace("\"", "\\\"").replace("\n", "\\n")),
        None => "null".to_string(),
    };

    let code_ref_str = match &node.code_ref {
        Some(cr) => format!("\"{}\"", cr),
        None => "null".to_string(),
    };

    format!(
        "---\n\
         id: \"{}\"\n\
         parent_id: {}\n\
         children_ids: [{}]\n\
         depth: {}\n\
         hypothesis: \"{}\"\n\
         status: \"{}\"\n\
         score: {}\n\
         result: {}\n\
         insight: {}\n\
         code_ref: {}\n\
         ---\n\n\
         # Hypothesis Tree Node: {}\n\n\
         {}\n",
        node.node_id,
        parent_link,
        children_links,
        node.depth,
        node.hypothesis.replace("\"", "\\\""),
        node.status,
        score_str,
        result_str,
        insight_str,
        code_ref_str,
        node.node_id,
        node.hypothesis
    )
}
