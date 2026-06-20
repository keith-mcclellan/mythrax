use notify::{Watcher, RecursiveMode, Event, RecommendedWatcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use tokio::sync::mpsc;
use anyhow::{Result, Context};
use crate::store::MarkdownStore;
use crate::db::StorageBackend;
use crate::contracts::{EpisodeSave, WisdomRule, Entity};

pub struct WatchIgnoreList {
    ignored: Mutex<HashMap<PathBuf, Instant>>,
}

impl WatchIgnoreList {
    pub fn new() -> Self {
        Self {
            ignored: Mutex::new(HashMap::new()),
        }
    }

    pub fn ignore(&self, path: PathBuf) {
        let mut map = self.ignored.lock().unwrap();
        map.insert(path, Instant::now());
    }

    pub fn is_ignored(&self, path: &Path) -> bool {
        let mut map = self.ignored.lock().unwrap();
        let now = Instant::now();
        map.retain(|_, &mut time| now.duration_since(time) < Duration::from_secs(2));
        map.contains_key(path)
    }
}

pub fn start_watching(
    vault_root: PathBuf,
    ignore_list: Arc<WatchIgnoreList>,
    backend: Arc<dyn StorageBackend>,
    store: Arc<MarkdownStore>,
) -> Result<RecommendedWatcher> {
    let (tx, mut rx) = mpsc::channel::<notify::Result<Event>>(100);

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.blocking_send(res);
        },
        notify::Config::default(),
    )?;

    watcher.watch(&vault_root, RecursiveMode::Recursive)?;

    // Spawn a tokio task to handle events
    let backend_clone = backend.clone();
    let store_clone = store.clone();
    tokio::spawn(async move {
        while let Some(res) = rx.recv().await {
            match res {
                Ok(event) => {
                    if event.kind.is_modify() || event.kind.is_create() {
                        for path in event.paths {
                            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                                continue;
                            }
                            if ignore_list.is_ignored(&path) {
                                tracing::debug!("Watcher ignoring path: {:?}", path);
                                continue;
                            }
                            if let Err(e) = sync_file_to_db(&path, &backend_clone, &store_clone).await {
                                tracing::error!("Failed to sync file {:?} to DB: {:?}", path, e);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Watcher error: {:?}", e);
                }
            }
        }
    });

    Ok(watcher)
}

pub fn parse_markdown_file(content: &str) -> (Option<serde_yaml::Value>, String) {
    if !content.starts_with("---") {
        return (None, content.to_string());
    }
    
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return (None, content.to_string());
    }
    
    let yaml_str = parts[1];
    let body = parts[2].trim().to_string();
    
    let yaml_val = serde_yaml::from_str(yaml_str).ok();
    (yaml_val, body)
}

#[derive(serde::Deserialize)]
struct EpisodeFrontmatter {
    title: Option<String>,
    scope: Option<String>,
    entities: Option<Vec<Entity>>,
}

#[derive(serde::Deserialize)]
struct WisdomFrontmatter {
    target_pattern: String,
    action_to_avoid: String,
    causal_explanation: String,
    prescribed_remedy: String,
    tier: Option<String>,
    scope: Option<String>,
    source_episodes: Option<Vec<String>>,
    generator_name: Option<String>,
}

pub async fn sync_file_to_db(
    path: &Path,
    backend: &Arc<dyn StorageBackend>,
    store: &Arc<MarkdownStore>,
) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .context("Failed to read file for sync")?;

    let (yaml_opt, body) = parse_markdown_file(&content);

    // Compute relative path
    let rel_path = path.strip_prefix(&store.vault_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    if rel_path.contains("episodes/") {
        let frontmatter: EpisodeFrontmatter = yaml_opt
            .and_then(|y| serde_json::to_value(y).ok())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or(EpisodeFrontmatter { title: None, scope: None, entities: None });

        let title = frontmatter.title.unwrap_or_else(|| {
            path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string()
        });

        let episode = EpisodeSave {
            title,
            content: body,
            entities: frontmatter.entities.unwrap_or_default(),
            scope: frontmatter.scope,
            vault_path: Some(rel_path),
        };

        backend.save_episode(&episode).await?;
    } else if rel_path.contains("wisdom/") {
        if let Some(yaml_val) = yaml_opt {
            let frontmatter: WisdomFrontmatter = serde_json::from_value(
                serde_json::to_value(yaml_val).unwrap_or_default()
            ).context("Failed to parse Wisdom frontmatter")?;

            let rule = WisdomRule {
                id: None,
                target_pattern: frontmatter.target_pattern,
                action_to_avoid: frontmatter.action_to_avoid,
                causal_explanation: frontmatter.causal_explanation,
                prescribed_remedy: frontmatter.prescribed_remedy,
                tier: frontmatter.tier.unwrap_or_else(|| "dynamic".to_string()),
                scope: frontmatter.scope.unwrap_or_else(|| "general".to_string()),
                vault_path: Some(rel_path),
                embedding: None,
                source_episodes: frontmatter.source_episodes.unwrap_or_default(),
                generator_name: frontmatter.generator_name.unwrap_or_else(|| "manual".to_string()),
                similarity: None,
            };

            backend.save_wisdom_rule(&rule).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::db::SurrealBackend;

    #[tokio::test]
    async fn test_watcher_sync() {
        let temp = tempdir().unwrap();
        let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await.unwrap());
        backend.init().await.unwrap();
        
        let store = Arc::new(MarkdownStore::new(temp.path()).unwrap());
        
        let relative_path = "episodes/test_note.md";
        let note_content = "---\ntitle: \"Watcher Test\"\nscope: \"watcher-testing\"\n---\nBody content here.";
        store.write_file(relative_path, note_content).unwrap();
        
        sync_file_to_db(&temp.path().join(relative_path), &backend, &store).await.unwrap();
        
        let results = backend.search("Body content", Some("watcher-testing"), 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Watcher Test");
    }
}
