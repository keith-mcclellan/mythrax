use serde_json::{json, Value};
use anyhow::{Result, Context};
use crate::api::ApiState;
use crate::db::SurrealBackend;
use crate::contracts::HypothesisNode;
use crate::cognitive::ArborCoordinator;

pub async fn handle_manage_htr(state: &ApiState, args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?;
    let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("general").to_string();

    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>()
        .context("SurrealBackend required for HTR")?;

    match action {
        "init" => {
            let hypothesis = args.get("hypothesis").and_then(|v| v.as_str()).context("Missing hypothesis")?.to_string();
            let files_val = args.get("files").and_then(|v| v.as_array()).context("Missing files")?;
            let files: Vec<String> = files_val.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect();

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                "".to_string(),
                files,
            ).await;
            coordinator.init_root(hypothesis, None).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "HTR root node initialized successfully."
                    }
                ]
            }))
        }
        "ideate" => {
            let node = args.get("node_id").or_else(|| args.get("node")).and_then(|v| v.as_str()).context("Missing node")?.to_string();

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                "".to_string(),
                vec![],
            ).await;
            coordinator.trigger_ideation(&node).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("HTR ideation complete for node: {}", node)
                    }
                ]
            }))
        }
        "execute" => {
            let node = args.get("node_id").or_else(|| args.get("node")).and_then(|v| v.as_str()).context("Missing node")?.to_string();
            let test_command = args.get("test_command").and_then(|v| v.as_str()).context("Missing test_command")?.to_string();

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                test_command,
                vec![],
            ).await;
            coordinator.execute_node(&node).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("HTR execution complete for node: {}", node)
                    }
                ]
            }))
        }
        "backprop" => {
            let node = args.get("node_id").or_else(|| args.get("node")).and_then(|v| v.as_str()).context("Missing node")?.to_string();

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                "".to_string(),
                vec![],
            ).await;
            coordinator.backpropagate_insights(&node).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("HTR backpropagation complete for node: {}", node)
                    }
                ]
            }))
        }
        "merge" => {
            let node = args.get("node_id").or_else(|| args.get("node")).and_then(|v| v.as_str()).context("Missing node")?.to_string();

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                "".to_string(),
                vec![],
            ).await;
            coordinator.decide_admission(&node).await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("HTR merge complete for node: {}", node)
                    }
                ]
            }))
        }
        "run" => {
            let hypothesis = args.get("hypothesis").and_then(|v| v.as_str()).context("Missing hypothesis")?.to_string();
            let files_val = args.get("files").and_then(|v| v.as_array()).context("Missing files")?;
            let files: Vec<String> = files_val.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect();
            let test_command = args.get("test_command").and_then(|v| v.as_str()).context("Missing test_command")?.to_string();
            let max_steps = args.get("max_steps").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir.clone(),
                llm,
                scope.clone(),
                test_command,
                files,
            ).await;
            
            coordinator.init_root(hypothesis, None).await?;
            
            let mut step = 0;
            let mut current_node = "ROOT".to_string();
            let mut status_msg = "HTR run loop completed without finding a candidate score >= 95.0.".to_string();
            
            loop {
                if step >= max_steps {
                    break;
                }
                coordinator.trigger_ideation(&current_node).await?;
                
                let next_batch = coordinator.select_next_batch(1).await?;
                if next_batch.is_empty() {
                    break;
                }
                
                let selected_node = &next_batch[0];
                coordinator.execute_node(selected_node).await?;
                coordinator.backpropagate_insights(selected_node).await?;
                
                let node_val: Option<HypothesisNode> = surreal_backend.db.select(("hypothesis_node", selected_node.as_str())).await?;
                if let Some(node_node) = node_val {
                    if let Some(score) = node_node.score {
                        if score >= 95.0 {
                            coordinator.decide_admission(selected_node).await?;
                            status_msg = format!("HTR run loop completed successfully. Node {} merged with Score: {}.", selected_node, score);
                            break;
                        }
                    }
                }
                
                current_node = selected_node.clone();
                step += 1;
            }

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": status_msg
                    }
                ]
            }))
        }
        "prune" => {
            let llm = crate::llm::LLMClient::new();
            let current_dir = std::env::current_dir()?;
            let coordinator = ArborCoordinator::new(
                surreal_backend.db.clone(),
                state.store.vault_root.clone(),
                current_dir,
                llm,
                scope,
                "".to_string(),
                vec![],
            ).await;
            coordinator.prune_failed_hypotheses().await?;

            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": "HTR prune complete. Deleted failed hypothesis branches and extracted preventive insights."
                    }
                ]
            }))
        }
        _ => anyhow::bail!("Invalid action for manage_htr: {}", action),
    }
}
