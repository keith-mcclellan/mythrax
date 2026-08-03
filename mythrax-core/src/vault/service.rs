use crate::db::StorageBackend;
use crate::store::MarkdownStore;
use anyhow::Result;
use std::sync::Arc;

pub struct VaultService {
    pub backend: Arc<dyn StorageBackend>,
    pub store: Arc<MarkdownStore>,
}

impl VaultService {
    pub fn new(backend: Arc<dyn StorageBackend>, store: Arc<MarkdownStore>) -> Self {
        Self { backend, store }
    }

    pub async fn sync(&self) -> Result<usize> {
        crate::vault::operations::sync_vault_to_db(&self.backend, &self.store).await
    }

    pub async fn verify_and_repair(&self, fix: bool) -> Result<(usize, usize)> {
        let synced = self.sync().await?;
        let mut missing = 0;
        let mut offset = 0;
        let limit: usize = 500;

        loop {
            let page = self.backend.get_episodes_paginated(limit as u32, offset).await?;
            if page.is_empty() {
                break;
            }

            for ep in &page {
                if let Some(ref vp) = ep.vault_path {
                    if vp.starts_with("episodes/") {
                        continue;
                    }
                    let path = self.store.vault_root.join(vp);
                    if !path.exists() {
                        missing += 1;
                        if fix {
                            let save = crate::contracts::EpisodeSave::builder(ep.title.clone(), ep.content.clone())
                                .scope(ep.scope.clone())
                                .vault_path(Some(vp.clone()))
                                .build();
                            let _ = self.backend.save_episode(&save).await;
                        }
                    }
                }
            }

            if page.len() < limit {
                break;
            }
            offset += limit as u32;
        }

        Ok((synced, missing))
    }
}
