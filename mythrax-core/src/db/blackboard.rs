use std::sync::Arc;
use anyhow::Result;
use crate::contracts::WikiNode;
use crate::db::SurrealBackend;

#[derive(Debug, Clone)]
pub enum WikiNodeEvent {
    Insert(WikiNode),
    Update(WikiNode),
    Delete { name: String, scope: String },
    Shutdown,
}

pub struct EventMessage {
    pub event: WikiNodeEvent,
    pub respond_to: tokio::sync::oneshot::Sender<Result<String>>,
}

pub struct MaterializerActor {
    backend: Arc<SurrealBackend>,
    receiver: tokio::sync::mpsc::Receiver<EventMessage>,
}

impl MaterializerActor {
    pub fn new(backend: Arc<SurrealBackend>, receiver: tokio::sync::mpsc::Receiver<EventMessage>) -> Self {
        Self { backend, receiver }
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            let is_shutdown = matches!(msg.event, WikiNodeEvent::Shutdown);
            let backend = self.backend.clone();
            let res = tokio::time::timeout(std::time::Duration::from_secs(5), async move {
                match msg.event {
                    WikiNodeEvent::Insert(ref node) | WikiNodeEvent::Update(ref node) => {
                        backend.save_wiki_node_db(node).await
                    }
                    WikiNodeEvent::Delete { ref name, ref scope } => {
                        backend.delete_wiki_node_db(name, scope).await?;
                        Ok(format!("Deleted wiki_node with name {} in scope {}", name, scope))
                    }
                    WikiNodeEvent::Shutdown => {
                        Ok("Shutdown".to_string())
                    }
                }
            }).await;

            let result = match res {
                Ok(inner_res) => inner_res,
                Err(_) => Err(anyhow::anyhow!("Database operation timed out after 5 seconds")),
            };

            let _ = msg.respond_to.send(result);
            if is_shutdown {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::StorageBackend;
    use tokio::sync::mpsc;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn test_blackboard_backpressure() {
        // Create a bounded channel with capacity 2.
        let (tx, _rx) = mpsc::channel::<EventMessage>(2);

        // Send 2 messages successfully.
        let node1 = WikiNode {
            id: None,
            name: "test1".to_string(),
            content: "content1".to_string(),
            scope: "scope1".to_string(),
            vault_path: None,
            embedding: None,
        };
        let (r1, _) = oneshot::channel();
        tx.try_send(EventMessage {
            event: WikiNodeEvent::Insert(node1.clone()),
            respond_to: r1,
        }).expect("First send should succeed");

        let (r2, _) = oneshot::channel();
        tx.try_send(EventMessage {
            event: WikiNodeEvent::Insert(node1.clone()),
            respond_to: r2,
        }).expect("Second send should succeed");

        // The third try_send should fail because capacity is 2 and actor is not reading.
        let (r3, _) = oneshot::channel();
        let send_res = tx.try_send(EventMessage {
            event: WikiNodeEvent::Insert(node1.clone()),
            respond_to: r3,
        });

        assert!(send_res.is_err());
        assert!(matches!(send_res.unwrap_err(), mpsc::error::TrySendError::Full(_)));
    }

    #[tokio::test]
    async fn test_sequential_serialization_and_ordering() {
        let backend = Arc::new(SurrealBackend::new_in_memory().await.unwrap());
        backend.init().await.unwrap();

        let (tx, rx) = mpsc::channel(100);
        let actor = MaterializerActor::new(backend.clone(), rx);
        let actor_handle = tokio::spawn(actor.run());

        // Spawn multiple tasks sending write and update requests.
        let mut handles = vec![];
        for i in 0..10 {
            let tx_clone = tx.clone();
            let handle = tokio::spawn(async move {
                let node = WikiNode {
                    id: None,
                    name: format!("node_{}", i),
                    content: format!("initial_content_{}", i),
                    scope: "test_scope".to_string(),
                    vault_path: None,
                    embedding: None,
                };
                let (respond_to, rx_resp) = oneshot::channel();
                tx_clone.send(EventMessage {
                    event: WikiNodeEvent::Insert(node),
                    respond_to,
                }).await.unwrap();
                let res = rx_resp.await.unwrap();
                assert!(res.is_ok());

                // Perform update
                let updated_node = WikiNode {
                    id: None,
                    name: format!("node_{}", i),
                    content: format!("updated_content_{}", i),
                    scope: "test_scope".to_string(),
                    vault_path: None,
                    embedding: None,
                };
                let (respond_to2, rx_resp2) = oneshot::channel();
                tx_clone.send(EventMessage {
                    event: WikiNodeEvent::Update(updated_node),
                    respond_to: respond_to2,
                }).await.unwrap();
                let res2 = rx_resp2.await.unwrap();
                assert!(res2.is_ok());
            });
            handles.push(handle);
        }

        // Wait for all requests to finish.
        for h in handles {
            h.await.unwrap();
        }

        // Drop the channel sender so actor stops loop.
        drop(tx);
        actor_handle.await.unwrap();

        // Check if database state matches expectation.
        let all_nodes = backend.get_all_wiki_nodes().await.unwrap();
        assert_eq!(all_nodes.len(), 10);
        for node in all_nodes {
            assert!(node.name.starts_with("node_"));
            let idx: usize = node.name["node_".len()..].parse().unwrap();
            assert_eq!(node.content, format!("updated_content_{}", idx));
        }
    }
}
