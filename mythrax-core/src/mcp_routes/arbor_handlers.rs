use crate::api::ApiState;
use crate::contracts::HypothesisNode;
use crate::db::SurrealBackend;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};

pub async fn handle_manage_arbor(state: &ApiState, args: Value) -> Result<Value> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .context("Missing required field 'action'")?;

    match action {
        "tree_add_node" => {
            let parent_id = args.get("parent_id").and_then(|v| v.as_str());
            let hypothesis = args
                .get("hypothesis")
                .and_then(|v| v.as_str())
                .context("Missing hypothesis")?;
            let node_id = args
                .get("node_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("node_{}", uuid::Uuid::new_v4().simple()));

            let node = HypothesisNode {
                node_id: node_id.clone(),
                parent_id: parent_id.map(|s| s.to_string()),
                hypothesis: hypothesis.to_string(),
                result: None,
                score: None,
                insight: None,
                status: "pending".to_string(),
                code_changes: None,
                ..Default::default()
            };

            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required for arbor tree actions")?;

            let _: Option<HypothesisNode> = surreal_backend
                .db
                .create(("hypothesis_node", node.node_id.as_str()))
                .content(node.clone())
                .await?;

            let md = crate::cognitive::arbor::format_node_markdown(&node);
            let rel_path = format!("arbor/nodes/{}.md", node.node_id);
            state.store.write_file(&rel_path, &md)?;

            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Tree node '{}' added successfully. Parent: {:?}", node_id, parent_id)
                }]
            }))
        }
        "tree_update_node" => {
            let node_id = args
                .get("node_id")
                .and_then(|v| v.as_str())
                .context("Missing node_id")?;

            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            let mut node: HypothesisNode = surreal_backend
                .db
                .select(("hypothesis_node", node_id))
                .await?
                .ok_or_else(|| anyhow!("Node not found"))?;

            if let Some(h) = args.get("hypothesis").and_then(|v| v.as_str()) {
                node.hypothesis = h.to_string();
            }
            if let Some(r) = args.get("result").and_then(|v| v.as_str()) {
                node.result = Some(r.to_string());
            }
            if let Some(i) = args.get("insight").and_then(|v| v.as_str()) {
                node.insight = Some(i.to_string());
            }
            if let Some(s) = args.get("status").and_then(|v| v.as_str()) {
                node.status = s.to_string();
            }

            let _: Option<HypothesisNode> = surreal_backend
                .db
                .update(("hypothesis_node", node_id))
                .content(node.clone())
                .await?;

            let md = crate::cognitive::arbor::format_node_markdown(&node);
            let rel_path = format!("arbor/nodes/{}.md", node_id);
            state.store.write_file(&rel_path, &md)?;

            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Tree node '{}' updated successfully.", node_id)
                }]
            }))
        }
        "tree_prune" => {
            let node_id = args
                .get("node_id")
                .and_then(|v| v.as_str())
                .context("Missing node_id")?;

            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            let update_sql =
                "UPDATE type::record('hypothesis_node', $id) MERGE { status: 'pruned' };";
            let _ = surreal_backend
                .db
                .query(update_sql)
                .bind(("id", node_id))
                .await?;

            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Tree node '{}' pruned successfully.", node_id)
                }]
            }))
        }
        "tree_view" => {
            let format = args
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("hierarchy");

            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            let mut resp = surreal_backend
                .db
                .query("SELECT * FROM hypothesis_node;")
                .await?;
            let nodes: Vec<HypothesisNode> = resp.take(0)?;

            let output = match format {
                "constraints" => {
                    let mut rules = Vec::new();
                    let target_scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general");
                    // 1. Permanent Wisdom
                    if let Ok(mut resp) = surreal_backend
                        .db
                        .query("SELECT * FROM wisdom WHERE tier = 'permanent';")
                        .await
                    {
                        if let Ok(w_rules) = resp.take::<Vec<crate::contracts::WisdomRule>>(0) {
                            for r in w_rules {
                                rules.push(format!(
                                    "> [!CAUTION]\n> **Rule on {}**:\n> - **Avoid**: {}\n> - **Remedy**: {}",
                                    r.target_pattern, r.action_to_avoid, r.prescribed_remedy
                                ));
                            }
                        }
                    }
                    // 2. Pruned Hypotheses
                    let sql_pruned = "SELECT * FROM wisdom WHERE rule_type = 'pruned_hypothesis' AND status = 'active' AND (scope = $scope OR scope = 'general') LIMIT 5;";
                    if let Ok(mut resp) = surreal_backend
                        .db
                        .query(sql_pruned)
                        .bind(("scope", target_scope))
                        .await
                    {
                        if let Ok(p_rules) = resp.take::<Vec<serde_json::Value>>(0) {
                            for val in p_rules {
                                if let (Some(pat), Some(avoid), Some(remedy)) = (
                                    val.get("target_pattern").and_then(|v| v.as_str()),
                                    val.get("action_to_avoid").and_then(|v| v.as_str()),
                                    val.get("prescribed_remedy").and_then(|v| v.as_str()),
                                ) {
                                    rules.push(format!(
                                        "> [!CAUTION]\n> **Pruned Path: {}**\n> - **Avoid**: {}\n> - **Remedy**: {}",
                                        pat, avoid, remedy
                                    ));
                                }
                            }
                        }
                    }
                    // 3. Conflict Nodes
                    if let Ok(mut resp) = surreal_backend
                        .db
                        .query("SELECT * FROM episode WHERE node_type = 'conflict' AND (scope = $scope OR scope = 'general');")
                        .bind(("scope", target_scope))
                        .await
                    {
                        if let Ok(episodes) = resp.take::<Vec<serde_json::Value>>(0) {
                            for val in episodes {
                                if let (Some(title), Some(content)) = (
                                    val.get("title").and_then(|v| v.as_str()),
                                    val.get("content").and_then(|v| v.as_str()),
                                ) {
                                    rules.push(format!(
                                        "> [!CAUTION]\n> **Knowledge Conflict: {}**\n> {}",
                                        title, content
                                    ));
                                }
                            }
                        }
                    }

                    if rules.is_empty() {
                        "### Negative Constraints & Guardrails\n(No active negative constraints found)".to_string()
                    } else {
                        format!("### Negative Constraints & Guardrails\n{}\n\n", rules.join("\n"))
                    }
                }
                "hierarchy" => {
                    let mut out = String::from("### Arbor Tree Hierarchy\n");
                    let mut children_map: std::collections::HashMap<Option<String>, Vec<&HypothesisNode>> = std::collections::HashMap::new();
                    for node in &nodes {
                        children_map.entry(node.parent_id.clone()).or_default().push(node);
                    }
                    fn render_level(
                        parent_id: Option<String>,
                        map: &std::collections::HashMap<Option<String>, Vec<&HypothesisNode>>,
                        depth: usize,
                        out: &mut String,
                    ) {
                        if let Some(children) = map.get(&parent_id) {
                            for child in children {
                                let indent = "  ".repeat(depth);
                                out.push_str(&format!(
                                    "{}- [{}] `{}`: {}\n",
                                    indent, child.status, child.node_id, child.hypothesis
                                ));
                                if let Some(ref i) = child.insight {
                                    out.push_str(&format!("{}  - Insight: {}\n", indent, i));
                                }
                                render_level(Some(child.node_id.clone()), map, depth + 1, out);
                            }
                        }
                    }
                    render_level(None, &children_map, 0, &mut out);
                    out
                }
                "insights" => {
                    let mut out = String::from("### Abstract Insights\n");
                    for node in &nodes {
                        if let Some(ref i) = node.insight {
                            out.push_str(&format!("- `{}`: {}\n", node.node_id, i));
                        }
                    }
                    out
                }
                "scores" => {
                    let mut out = String::from("### Node Evaluation Scores\n");
                    for node in &nodes {
                        out.push_str(&format!(
                            "- `{}` (Status: {}): Score: {:?}\n",
                            node.node_id, node.status, node.score
                        ));
                    }
                    out
                }
                _ => {
                    format!("### Tree Overview\nTotal Nodes: {}", nodes.len())
                }
            };

            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": output
                }]
            }))
        }
        "git_merge_branch" => {
            let node_id = args
                .get("node_id")
                .and_then(|v| v.as_str())
                .context("Missing node_id")?;
            let test_cmd = args
                .get("test_command")
                .and_then(|v| v.as_str())
                .unwrap_or("cargo check");
            let target_branch = args
                .get("branch")
                .and_then(|v| v.as_str())
                .unwrap_or("main");

            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            let mut node: HypothesisNode = surreal_backend
                .db
                .select(("hypothesis_node", node_id))
                .await?
                .ok_or_else(|| anyhow!("Node '{}' not found for merge gate", node_id))?;

            let branch_name = format!("htr_branch_{}", node_id);
            let evaluator = crate::cognitive::arbor::TestCommandEvaluator {
                test_command: test_cmd.to_string(),
            };

            let repo_path =
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

            // Evaluate test command on the hypothesis branch directly!
            let test_score = crate::cognitive::arbor::HeldOutEvaluator::evaluate(
                &evaluator,
                &branch_name,
                &repo_path,
            )
            .unwrap_or(0.0);
            node.score = Some(test_score);

            let merge_passed = test_score >= 70.0;
            if merge_passed {
                let _ = std::process::Command::new("git")
                    .args(["checkout", target_branch])
                    .current_dir(&repo_path)
                    .status();

                let merge_status = std::process::Command::new("git")
                    .args(["merge", &branch_name])
                    .current_dir(&repo_path)
                    .status();
                if let Ok(st) = merge_status {
                    if st.success() {
                        node.status = "merged".to_string();
                    } else {
                        node.status = "failed_merge".to_string();
                    }
                } else {
                    node.status = "failed_merge".to_string();
                }
            } else {
                node.status = "rejected".to_string();
            }

            let _: Option<HypothesisNode> = surreal_backend
                .db
                .update(("hypothesis_node", node_id))
                .content(node.clone())
                .await?;

            let md = crate::cognitive::arbor::format_node_markdown(&node);
            let rel_path = format!("arbor/nodes/{}.md", node_id);
            state.store.write_file(&rel_path, &md)?;

            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Git merge branch evaluated for node '{}' (Etest Score: {}, Status: '{}').", node_id, test_score, node.status)
                }]
            }))
        }
        _ => Err(anyhow!("Unknown arbor action: {}", action)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiState;
    use crate::db::StorageBackend;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_arbor_tree_actions_and_views() {
        let temp = tempdir().unwrap();
        let backend: Arc<dyn StorageBackend> =
            Arc::new(SurrealBackend::new_in_memory().await.unwrap());
        backend.init().await.unwrap();

        let store = Arc::new(crate::store::MarkdownStore::new(temp.path()).unwrap());
        let state = ApiState {
            backend,
            auth_token: "secret-token".to_string(),
            store,
            ignore_list: Arc::new(crate::vault::watcher::WatchIgnoreList::new()),
            dream_tx: None,
            shutdown_tx: None,
        };

        // 1. TreeAddNode
        let res = handle_manage_arbor(
            &state,
            json!({
                "action": "tree_add_node",
                "node_id": "test_root",
                "hypothesis": "Root hypothesis"
            }),
        )
        .await
        .unwrap();
        assert!(res["content"][0]["text"].as_str().unwrap().contains("test_root"));

        // 2. TreeUpdateNode
        let res_up = handle_manage_arbor(
            &state,
            json!({
                "action": "tree_update_node",
                "node_id": "test_root",
                "insight": "Causal insight rule"
            }),
        )
        .await
        .unwrap();
        assert!(res_up["content"][0]["text"].as_str().unwrap().contains("updated"));

        // 3. TreeView hierarchy
        let res_h = handle_manage_arbor(
            &state,
            json!({
                "action": "tree_view",
                "format": "hierarchy"
            }),
        )
        .await
        .unwrap();
        assert!(res_h["content"][0]["text"].as_str().unwrap().contains("test_root"));

        // 4. TreeView constraints
        let res_c = handle_manage_arbor(
            &state,
            json!({
                "action": "tree_view",
                "format": "constraints"
            }),
        )
        .await
        .unwrap();
        assert!(res_c["content"][0]["text"].as_str().unwrap().contains("Negative Constraints"));

        // 5. TreePrune
        let res_p = handle_manage_arbor(
            &state,
            json!({
                "action": "tree_prune",
                "node_id": "test_root"
            }),
        )
        .await
        .unwrap();
        assert!(res_p["content"][0]["text"].as_str().unwrap().contains("pruned"));

        // 6. GitMergeBranch (Etest)
        let res_m = handle_manage_arbor(
            &state,
            json!({
                "action": "git_merge_branch",
                "node_id": "test_root",
                "test_command": "echo 'ok'"
            }),
        )
        .await
        .unwrap();
        assert!(res_m["content"][0]["text"].as_str().unwrap().contains("Git merge branch evaluated"));
    }
}
