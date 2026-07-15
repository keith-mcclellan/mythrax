use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, anyhow};
use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use crate::db::StorageBackend;
use crate::contracts::{HypothesisNode, WisdomRule, Tier};

pub trait ArborLlmClient: Send + Sync + Clone + 'static {
    fn propose_hypotheses(
        &self,
        db: &dyn StorageBackend,
        parent_id: &str,
        parent_hypothesis: &str,
        target_files: &[(String, String)],
        constraints: &[String],
    ) -> impl std::future::Future<Output = Result<String>> + Send;
    fn evaluate_run(&self, db: &dyn StorageBackend, run_logs: &str) -> impl std::future::Future<Output = Result<String>> + Send;
    fn abstract_insights(&self, db: &dyn StorageBackend, parent_insight: Option<&str>, child_insight: &str) -> impl std::future::Future<Output = Result<String>> + Send;
}

pub trait HeldOutEvaluator: Send + Sync {
    fn evaluate(&self, branch_name: &str, temp_worktree_path: &Path) -> Result<f32>;
}

pub struct TestCommandEvaluator {
    pub test_command: String,
}

impl HeldOutEvaluator for TestCommandEvaluator {
    fn evaluate(&self, _branch_name: &str, temp_worktree_path: &Path) -> Result<f32> {
        let has_shell_operators = self.test_command.contains('&') || self.test_command.contains('|') || self.test_command.contains('>') || self.test_command.contains('<') || self.test_command.contains(';');
        let mut cmd = if has_shell_operators {
            let mut c = Command::new("sh");
            c.arg("-c").arg(&self.test_command);
            c
        } else {
            let mut args = Vec::new();
            let mut current_arg = String::new();
            let mut in_quotes = false;
            let mut quote_char = '\0';
            for c in self.test_command.chars() {
                match c {
                    '"' | '\'' if !in_quotes => {
                        in_quotes = true;
                        quote_char = c;
                    }
                    '"' | '\'' if in_quotes && c == quote_char => {
                        in_quotes = false;
                        quote_char = '\0';
                    }
                    ' ' | '\t' if !in_quotes => {
                        if !current_arg.is_empty() {
                            args.push(std::mem::take(&mut current_arg));
                        }
                    }
                    _ => {
                        current_arg.push(c);
                    }
                }
            }
            if !current_arg.is_empty() {
                args.push(current_arg);
            }
            if args.is_empty() {
                return Err(anyhow!("Empty test command"));
            }
            let mut c = Command::new(&args[0]);
            if args.len() > 1 {
                c.args(&args[1..]);
            }
            c
        };
        cmd.current_dir(temp_worktree_path);
        let status = cmd.status()?;
        if status.success() {
            Ok(100.0)
        } else {
            Ok(0.0)
        }
    }
}

pub struct LlmCriticEvaluator<L: ArborLlmClient> {
    pub llm_client: L,
    pub backend: crate::db::SurrealBackend,
}

impl<L: ArborLlmClient> HeldOutEvaluator for LlmCriticEvaluator<L> {
    fn evaluate(&self, branch_name: &str, temp_worktree_path: &Path) -> Result<f32> {
        let output = Command::new("git")
            .args(["diff", "HEAD", branch_name])
            .current_dir(temp_worktree_path)
            .output()?;
        let diff_str = String::from_utf8_lossy(&output.stdout).to_string();

        let backend = self.backend.clone();
        let llm_client = self.llm_client.clone();
        let diff_str_clone = diff_str.clone();

        let score = if let Ok(_handle) = tokio::runtime::Handle::try_current() {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                rt.block_on(async {
                    let critic = crate::cognitive::critic::ArborCritic::new();
                    if let Ok(res) = critic.evaluate(&backend, &llm_client, &diff_str_clone).await {
                        Ok::<f32, anyhow::Error>(res.score)
                    } else {
                        Ok::<f32, anyhow::Error>(50.0f32)
                    }
                })
            }).join().map_err(|_| anyhow::anyhow!("Thread panicked"))??
        } else {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let critic = crate::cognitive::critic::ArborCritic::new();
                if let Ok(res) = critic.evaluate(&backend, &llm_client, &diff_str).await {
                    Ok::<f32, anyhow::Error>(res.score)
                } else {
                    Ok::<f32, anyhow::Error>(50.0f32)
                }
            })?
        };
        Ok(score)
    }
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
            constraints: vec![],
            visits: 0,
        };

        let _: Option<HypothesisNode> = self.db.create(("hypothesis_node", root_id.as_str()))
            .content(root_node.clone())
            .await?;

        let md = format_node_markdown(&root_node);
        let path = self.get_vault_path(&root_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, md)?;

        Ok(())
    }

    pub async fn trigger_ideation(&self, parent_id: &str) -> Result<()> {
        let mut parent: HypothesisNode = self.db.select(("hypothesis_node", parent_id))
            .await?
            .ok_or_else(|| anyhow!("Parent node not found"))?;

        let files_context = self.get_current_files_context(&parent);
        let response = self.llm_client.propose_hypotheses(&self.backend, &parent.node_id, &parent.hypothesis, &files_context, &parent.constraints).await?;
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
                constraints: parent.constraints.clone(),
                visits: 0,
            };

            let _: Option<HypothesisNode> = self.db.create(("hypothesis_node", node_id.as_str()))
                .content(child_node.clone())
                .await?;

            let child_md = format_node_markdown(&child_node);
            let child_path = self.get_vault_path(&node_id);
            if let Some(p_dir) = child_path.parent() {
                fs::create_dir_all(p_dir)?;
            }
            fs::write(child_path, child_md)?;

            children_ids.push(node_id);
        }

        parent.children_ids.extend(children_ids);
        
        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", parent.node_id.as_str()))
            .content(parent.clone())
            .await?;

        let parent_md = format_node_markdown(&parent);
        fs::write(self.get_vault_path(&parent.node_id), parent_md)?;

        Ok(())
    }

    pub async fn select_next_batch(&self, limit: usize) -> Result<Vec<String>> {
        let mut res = self.db.query("SELECT * FROM hypothesis_node WHERE status = 'pending' AND scope = $target_scope ORDER BY score DESC LIMIT $limit")
            .bind(("target_scope", self.scope.as_str()))
            .bind(("limit", limit))
            .await?;
        let nodes: Vec<HypothesisNode> = res.take(0)?;
        Ok(nodes.into_iter().map(|n| n.node_id).collect())
    }

    pub async fn execute_node(&self, node_id: &str) -> Result<()> {
        let mut node: HypothesisNode = self.db.select(("hypothesis_node", node_id))
            .await?
            .ok_or_else(|| anyhow!("Node not found"))?;

        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.repo_path)
            .output()?;
        let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let executor = crate::cognitive::executor::ArborExecutor::new(self.repo_path.clone());
        let (mut success, mut logs) = executor.execute(&node.node_id, &commit_sha, &self.test_command, &node.code_changes, &self.backend).await?;

        let max_attempts = crate::store::get_config_val_int("htr", "tdd_max_attempts", 5) as usize;
        let mut attempt = 1;
        
        while !success && attempt < max_attempts {
            tracing::warn!("HTR TDD Loop: Attempt {}/{} failed. Attempting self-healing...", attempt, max_attempts);
            
            let prompt = format!(
                "The previous code changes failed to compile or pass tests.\n\
                 Hypothesis: {}\n\
                 Failure logs:\n{}\n\n\
                 Based on this error, please provide the corrected files.\n\
                 Return a JSON object containing the field 'code_changes' which maps file paths to their complete corrected content.\n\n\
                 JSON Response:",
                node.hypothesis, logs
            );
            
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
        
        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", node.node_id.as_str()))
            .content(node.clone())
            .await?;

        let md = format_node_markdown(&node);
        fs::write(self.get_vault_path(&node.node_id), md)?;

        Ok(())
    }

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

        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", leaf.node_id.as_str()))
            .content(leaf.clone())
            .await?;

        let leaf_md = format_node_markdown(&leaf);
        fs::write(self.get_vault_path(&leaf.node_id), leaf_md)?;

        let mut current_parent_id = leaf.parent_id.clone();
        while let Some(parent_id) = current_parent_id {
            let mut parent: HypothesisNode = self.db.select(("hypothesis_node", parent_id.as_str()))
                .await?
                .ok_or_else(|| anyhow!("Parent node not found"))?;

            let parent_insight = parent.insight.as_deref();
            let child_insight = critic_output.insight.as_str();

            let new_insight = self.llm_client.abstract_insights(&self.backend, parent_insight, child_insight).await?;
            parent.insight = Some(new_insight);

            let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", parent.node_id.as_str()))
                .content(parent.clone())
                .await?;

            let parent_md = format_node_markdown(&parent);
            fs::write(self.get_vault_path(&parent.node_id), parent_md)?;

            current_parent_id = parent.parent_id.clone();
        }

        Ok(())
    }

    pub async fn decide_admission(&self, node_id: &str) -> Result<()> {
        let mut node: HypothesisNode = self.db.select(("hypothesis_node", node_id))
            .await?
            .ok_or_else(|| anyhow!("Node not found"))?;

        let branch_name = format!("htr_branch_{}", node_id);
        let temp_dir = format!("/tmp/admission-gate-{}", node_id);
        let temp_path = Path::new(&temp_dir);

        if temp_path.exists() {
            let _ = std::fs::remove_dir_all(temp_path);
        }
        let status = Command::new("git")
            .args(["worktree", "add", &temp_dir, &branch_name])
            .current_dir(&self.repo_path)
            .status()?;
        if !status.success() {
            return Err(anyhow!("Failed to add worktree for admission check"));
        }

        let test_eval = TestCommandEvaluator { test_command: self.test_command.clone() };
        let critic_eval = LlmCriticEvaluator {
            llm_client: self.llm_client.clone(),
            backend: self.backend.clone(),
        };

        let test_score = test_eval.evaluate(&branch_name, temp_path).unwrap_or(0.0);
        let critic_score = critic_eval.evaluate(&branch_name, temp_path).unwrap_or(50.0);
        let blended_score = (test_score + critic_score) / 2.0;

        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", &temp_dir])
            .current_dir(&self.repo_path)
            .status();
        if temp_path.exists() {
            let _ = std::fs::remove_dir_all(temp_path);
        }

        node.score = Some(blended_score);

        if blended_score >= 70.0 {
            let merge_status = Command::new("git")
                .args(["merge", &branch_name])
                .current_dir(&self.repo_path)
                .status()?;
            if merge_status.success() {
                node.status = "merged".to_string();
            } else {
                node.status = "failed_merge".to_string();
            }
        } else {
            node.status = "rejected".to_string();
        }

        let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", node.node_id.as_str()))
            .content(node.clone())
            .await?;

        let md = format_node_markdown(&node);
        fs::write(self.get_vault_path(&node.node_id), md)?;

        Ok(())
    }

    pub async fn dispatch_batch(&self, node_ids: &[String]) -> Result<()> {
        use futures_util::stream::{StreamExt, TryStreamExt};
        
        futures_util::stream::iter(node_ids)
            .map(|id| {
                let id_clone = id.clone();
                async move {
                    self.execute_node(&id_clone).await
                }
            })
            .buffer_unordered(2)
            .try_collect::<Vec<()>>()
            .await?;

        Ok(())
    }

    pub async fn select_node_uct(&self, parent_id: &str) -> Result<Option<String>> {
        let parent: HypothesisNode = self.db.select(("hypothesis_node", parent_id))
            .await?
            .ok_or_else(|| anyhow!("Parent node not found"))?;

        if parent.children_ids.is_empty() {
            return Ok(None);
        }

        let mut best_node_id = None;
        let mut best_val = -1.0f32;

        let parent_visits = parent.visits as f32;
        let ln_np = if parent_visits > 0.0 { parent_visits.ln() } else { 0.0 };

        for child_id in &parent.children_ids {
            if let Some(child) = self.db.select::<Option<HypothesisNode>>(("hypothesis_node", child_id.as_str())).await? {
                let node_score = child.score.unwrap_or(50.0);
                let node_visits = child.visits as f32;

                let uct_val = if node_visits == 0.0 {
                    f32::INFINITY
                } else {
                    node_score / 100.0 + 1.414 * (ln_np / node_visits).sqrt()
                };

                if uct_val > best_val {
                    best_val = uct_val;
                    best_node_id = Some(child.node_id.clone());
                }
            }
        }

        Ok(best_node_id)
    }

    pub async fn increment_visits_upward(&self, node_id: &str) -> Result<()> {
        let mut current_id = Some(node_id.to_string());
        while let Some(id) = current_id {
            if let Some(mut node) = self.db.select::<Option<HypothesisNode>>(("hypothesis_node", id.as_str())).await? {
                node.visits += 1;
                let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", node.node_id.as_str()))
                    .content(node.clone())
                    .await?;
                
                let md = format_node_markdown(&node);
                let _ = fs::write(self.get_vault_path(&node.node_id), md);

                current_id = node.parent_id.clone();
            } else {
                break;
            }
        }
        Ok(())
    }

    pub async fn run_full_loop(&self) -> Result<()> {
        let mut pending_ids = self.select_next_batch(1).await?;
        if pending_ids.is_empty() {
            if let Some(selected_id) = self.select_node_uct("ROOT").await? {
                pending_ids.push(selected_id);
            }
        }

        if let Some(node_id) = pending_ids.first() {
            self.increment_visits_upward(node_id).await?;
            self.execute_node(node_id).await?;
            self.backpropagate_insights(node_id).await?;
            self.decide_admission(node_id).await?;
        }

        Ok(())
    }

    pub async fn prune_failed_hypotheses(&self) -> Result<()> {
        let sql = "SELECT * FROM hypothesis_node WHERE status = 'done' AND score < 70.0 AND scope = $target_scope;";
        let mut response = self.db.query(sql).bind(("target_scope", self.scope.as_str())).await?.check()?;
        let failed_nodes: Vec<HypothesisNode> = response.take(0)?;

        for node in failed_nodes {
            let branch_name = format!("htr_branch_{}", node.node_id);
            let _ = Command::new("git")
                .args(["branch", "-D", &branch_name])
                .current_dir(&self.repo_path)
                .status();

            if let Some(ref insight_str) = node.insight {
                let uuid = uuid::Uuid::new_v4().to_string();
                let rule = WisdomRule {
                    id: Some(format!("wisdom:{}", uuid)),
                    target_pattern: format!("Failed path: {}", node.hypothesis),
                    action_to_avoid: format!("Avoid implementing hypothesis: {}", node.hypothesis),
                    causal_explanation: format!("This approach failed tests. Logs/Insight: {}", insight_str),
                    prescribed_remedy: "Try an alternative refactoring approach or adjust target files.".to_string(),
                    tier: Tier::Project,
                    scope: self.scope.clone(),
                    vault_path: None,
                    embedding: None,
                    source_episodes: vec![],
                    generator_name: "HtrPruneAction".to_string(),
                    similarity: Some(1.0),
                    utility: Some(1.0),
                    status: Some("active".to_string()),
                    superseded_at: None,
                    superseded_by: None,
                    rule_type: Some("pruned_hypothesis".to_string()),
                    severity: Some("warning".to_string()),
                    blocking: Some(true),
                    importance: Some(8.0),
                };
                
                self.backend.save_wisdom_rule(&rule).await?;
            }

            let mut updated_node = node;
            updated_node.status = "pruned".to_string();
            let _: Option<HypothesisNode> = self.db.update(("hypothesis_node", updated_node.node_id.as_str()))
                .content(updated_node.clone())
                .await?;
            
            let md = format_node_markdown(&updated_node);
            let _ = fs::write(self.get_vault_path(&updated_node.node_id), md);
        }

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

    let constraints_str = serde_json::to_string(&node.constraints).unwrap_or_else(|_| "[]".to_string());

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
         constraints: {}\n\
         visits: {}\n\
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
        constraints_str,
        node.visits,
        node.node_id,
        node.hypothesis,
        navigation_section
    )
}
