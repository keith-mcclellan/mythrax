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
                    let wisdom_res = state
                        .backend
                        .get_wisdom("*", None, 50, 0, 0.0)
                        .await
                        .unwrap_or_default();
                    for r in wisdom_res.results {
                        rules.push(format!(
                            "- Avoid: `{}` | Reason: `{}`",
                            r.action_to_avoid, r.causal_explanation
                        ));
                    }
                    if rules.is_empty() {
                        "### Negative Constraints & Guardrails\n(No active negative constraints found)".to_string()
                    } else {
                        format!("### Negative Constraints & Guardrails\n{}", rules.join("\n"))
                    }
                }
                "hierarchy" => {
                    let mut out = String::from("### Arbor Tree Hierarchy\n");
                    for node in &nodes {
                        let indent = if node.parent_id.is_some() { "  " } else { "" };
                        out.push_str(&format!(
                            "{}- [{}] `{}`: {}\n",
                            indent,
                            node.status,
                            node.node_id,
                            node.hypothesis
                        ));
                        if let Some(ref i) = node.insight {
                            out.push_str(&format!("{}  - Insight: {}\n", indent, i));
                        }
                    }
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

            let surreal_backend = state
                .backend
                .as_any()
                .downcast_ref::<SurrealBackend>()
                .context("SurrealBackend required")?;

            let node: Option<HypothesisNode> = surreal_backend
                .db
                .select(("hypothesis_node", node_id))
                .await?;

            if let Some(n) = node {
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Git merge branch evaluated for node '{}' (Score: {:?}, Status: {}).", node_id, n.score, n.status)
                    }]
                }))
            } else {
                Err(anyhow!("Node '{}' not found for merge gate", node_id))
            }
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
    }
}
