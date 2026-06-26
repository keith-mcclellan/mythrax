use std::fs;
use std::path::PathBuf;
use std::process::Command;
use anyhow::{Result, anyhow};
use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use crate::db::StorageBackend;
use crate::contracts::HypothesisNode;

pub trait ArborLlmClient: Send + Sync {
    async fn propose_hypotheses(
        &self,
        db: &dyn StorageBackend,
        parent_id: &str,
        parent_hypothesis: &str,
        target_files: &[(String, String)],
    ) -> Result<String>;
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
    test_command: String,
    target_files: Vec<String>,
}

impl<L: ArborLlmClient> ArborCoordinator<L> {
    pub async fn new(
        db: Surreal<Db>,
        vault_root: PathBuf,
        repo_path: PathBuf,
        llm_client: L,
        scope: String,
        test_command: String,
        target_files: Vec<String>,
    ) -> Self {
        let backend = crate::db::SurrealBackend::new_with_db(db.clone());
        Self {
            db,
            backend,
            vault_root,
            repo_path,
            llm_client,
            scope,
            test_command,
            target_files,
        }
    }

    fn get_vault_path(&self, node_id: &str) -> PathBuf {
        self.vault_root.join(format!("wiki/{}/hypothesis_tree/{}.md", self.scope, node_id))
    }

    fn get_current_files_context(&self, parent: &HypothesisNode) -> Vec<(String, String)> {
        let mut result = Vec::new();
        for rel_path in &self.target_files {
            let mut content = String::new();
            if let Some(ref changes) = parent.code_changes
                && let Some(c) = changes.get(rel_path) {
                    content = c.clone();
                }
            if content.is_empty() {
                let full_path = self.repo_path.join(rel_path);
                if full_path.exists()
                    && let Ok(c) = fs::read_to_string(full_path) {
                        content = c;
                    }
            }
            result.push((rel_path.clone(), content));
        }
        result
    }

    /// Step A: Initialize the root node and base assessment
    pub async fn init_root(
        &self,
        hypothesis: String,
        code_changes: Option<std::collections::HashMap<String, String>>,
    ) -> Result<()> {
        let root_id = "ROOT".to_string();
        
        let root_node = HypothesisNode {
            node_id: root_id.clone(),
            parent_id: None,
            children_ids: vec![],
            depth: 0,
            hypothesis,
            status: "pending".to_string(),
            score: None,
            result: None,
            insight: None,
            code_ref: None,
            code_changes,
            scope: Some(self.scope.clone()),
            vault_path: Some(format!("wiki/{}/hypothesis_tree/{}.md", self.scope, root_id)),
        };

        // Write to SurrealDB
        let _: Option<HypothesisNode> = self.db.create(("hypothesis_node", root_id.as_str()))
            .content(root_node.clone())
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

        let files_context = self.get_current_files_context(&parent);
        let response = self.llm_client.propose_hypotheses(&self.backend, &parent.node_id, &parent.hypothesis, &files_context).await?;
        let proposals: Vec<serde_json::Value> = serde_json::from_str(&response)?;
        
        let mut children_ids = vec![];
        for prop in proposals {
            let node_id = prop["node_id"].as_str().ok_or_else(|| anyhow!("Missing node_id"))?.to_string();
            let hypothesis = prop["hypothesis"].as_str().ok_or_else(|| anyhow!("Missing hypothesis"))?.to_string();
            let score = prop["score"].as_f64().map(|s| s as f32);

            let code_changes: Option<std::collections::HashMap<String, String>> = prop["code_changes"]
                .as_object()
                .map(|obj| {
                    obj.iter()
                       .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                       .collect()
                });

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
                code_changes,
                scope: Some(self.scope.clone()),
                vault_path: Some(format!("wiki/{}/hypothesis_tree/{}.md", self.scope, node_id)),
            };

            // Write to SurrealDB
            let _: Option<HypothesisNode> = self.db.create(("hypothesis_node", node_id.as_str()))
                .content(child_node.clone())
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
        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", parent.node_id.as_str()))
            .content(parent.clone())
            .await?;

        // Rewrite parent markdown
        let parent_md = format_node_markdown(&parent);
        fs::write(self.get_vault_path(&parent.node_id), parent_md)?;

        Ok(())
    }

    /// Step C: Select next batch of hypotheses
    pub async fn select_next_batch(&self, limit: usize) -> Result<Vec<String>> {
        let mut res = self.db.query("SELECT * FROM hypothesis_node WHERE status = 'pending' AND scope = $target_scope ORDER BY score DESC LIMIT $limit")
            .bind(("target_scope", self.scope.as_str()))
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
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.repo_path)
            .output()?;
        let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let executor = crate::cognitive::executor::ArborExecutor::new(self.repo_path.clone());
        let (mut success, mut logs) = executor.execute(&node.node_id, &commit_sha, &self.test_command, &node.code_changes, &self.backend).await?;

        // T6: Stateful TDD Loop
        let max_attempts = crate::store::get_config_val_int("htr", "tdd_max_attempts", 5) as usize;
        let mut attempt = 1;
        
        while !success && attempt < max_attempts {
            tracing::warn!("HTR TDD Loop: Attempt {}/{} failed. Attempting self-healing...", attempt, max_attempts);
            
            // Construct debugging prompt
            let prompt = format!(
                "The previous code changes failed to compile or pass tests.\n\
                 Hypothesis: {}\n\
                 Failure logs:\n{}\n\n\
                 Based on this error, please provide the corrected files.\n\
                 Return a JSON object containing the field 'code_changes' which maps file paths to their complete corrected content.\n\n\
                 JSON Response:",
                node.hypothesis, logs
            );
            
            // Query the LLM
            match self.llm_client.evaluate_run(&self.backend, &prompt).await {
                Ok(resp) => {
                    let cleaned = crate::llm::strip_code_fences(&resp);
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&cleaned) {
                        if let Some(changes_obj) = val.get("code_changes").and_then(|c| c.as_object()) {
                            let mut new_changes = std::collections::HashMap::new();
                            for (k, v) in changes_obj {
                                if let Some(content_str) = v.as_str() {
                                    new_changes.insert(k.clone(), content_str.to_string());
                                }
                            }
                            node.code_changes = Some(new_changes);
                            
                            // Re-execute tests
                            let (new_success, new_logs) = executor.execute(&node.node_id, &commit_sha, &self.test_command, &node.code_changes, &self.backend).await?;
                            success = new_success;
                            logs = new_logs;
                        } else {
                            tracing::warn!("LLM did not return code_changes in the expected JSON format");
                            break;
                        }
                    } else {
                        tracing::warn!("Failed to parse LLM response as JSON: {}", cleaned);
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to query LLM for TDD fix: {:?}", e);
                    break;
                }
            }
            attempt += 1;
        }

        node.result = Some(logs);
        
        // Update in SurrealDB
        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", node.node_id.as_str()))
            .content(node.clone())
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
        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", leaf.node_id.as_str()))
            .content(leaf.clone())
            .await?;

        // Rewrite leaf markdown
        let leaf_md = format_node_markdown(&leaf);
        fs::write(self.get_vault_path(&leaf.node_id), leaf_md)?;

        // Propagate up to parent ancestors
        let mut current_parent_id = leaf.parent_id.clone();
        while let Some(parent_id) = current_parent_id {
            let mut parent: HypothesisNode = self.db.select(("hypothesis_node", parent_id.as_str()))
                .await?
                .ok_or_else(|| anyhow!("Parent node not found"))?;

            let parent_insight = parent.insight.as_deref();
            let child_insight = critic_output.insight.as_str();

            let new_insight = self.llm_client.abstract_insights(&self.backend, parent_insight, child_insight).await?;
            parent.insight = Some(new_insight);

            // Update parent in SurrealDB
            let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", parent.node_id.as_str()))
                .content(parent.clone())
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

        // Apply dynamic code changes to the repo path
        if let Some(ref changes) = node.code_changes {
            for (rel_path, content) in changes {
                let full_path = self.repo_path.join(rel_path);
                if let Some(parent) = full_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&full_path, content)?;

                // Run git add
                let _ = Command::new("git")
                    .args(["add", rel_path])
                    .current_dir(&self.repo_path)
                    .status();
            }
        }

        // Commit changes to main branch
        let commit_msg = format!("Apply HTR refinement: {} (Score: {})", node.hypothesis, node.score.unwrap_or(0.0));
        let _ = Command::new("git")
            .args(["commit", "-m", &commit_msg])
            .current_dir(&self.repo_path)
            .status();

        node.status = "merged".to_string();

        // Update in SurrealDB
        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", node.node_id.as_str()))
            .content(node.clone())
            .await?;

        // Rewrite vault markdown
        let md = format_node_markdown(&node);
        fs::write(self.get_vault_path(&node.node_id), md)?;

        Ok(())
    }
}

fn format_node_markdown(node: &HypothesisNode) -> String {
    let scope = node.scope.as_deref().unwrap_or("default");

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

    let code_changes_str = match &node.code_changes {
        Some(changes) => serde_json::to_string(changes).unwrap_or_else(|_| "null".to_string()),
        None => "null".to_string(),
    };

    // Format Navigation body section
    let nav_parent = match &node.parent_id {
        Some(p) => format!("[[wiki/{}/hypothesis_tree/{}|{}]]", scope, p, p),
        None => "None".to_string(),
    };

    let nav_children = if node.children_ids.is_empty() {
        "None".to_string()
    } else {
        let child_bullets: Vec<String> = node.children_ids.iter()
            .map(|c| format!("  - [[wiki/{}/hypothesis_tree/{}|{}]]", scope, c, c))
            .collect();
        format!("\n{}", child_bullets.join("\n"))
    };

    let navigation_section = format!(
        "\n\n## Navigation\n- **Parent**: {}\n- **Children**: {}",
        nav_parent,
        nav_children
    );

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
         code_changes: {}\n\
         ---\n\n\
         # Hypothesis Tree Node: {}\n\n\
         {}{}\n",
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
        code_changes_str,
        node.node_id,
        node.hypothesis,
        navigation_section
    )
}
