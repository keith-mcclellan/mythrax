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
use crate::vault::markdown::{parse_frontmatter, extract_plain_text};
use crate::vault::organization::organize_file;

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
    dream_tx: Option<tokio::sync::mpsc::Sender<()>>,
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
    let dream_tx_clone = dream_tx.clone();
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
                            } else {
                                if let Some(ref tx) = dream_tx_clone {
                                    let _ = tx.send(()).await;
                                }
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

    let (yaml_opt, body) = parse_frontmatter(&content);
    
    // Extract plain text for database indexing/embeddings
    let plain_body = extract_plain_text(&body);

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
            content: plain_body,
            entities: frontmatter.entities.unwrap_or_default(),
            scope: frontmatter.scope,
            vault_path: Some(rel_path),
            source_episode: None,
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
                utility: None,
            };

            backend.save_wisdom_rule(&rule).await?;
        }
    }

    Ok(())
}

/// Helper to slugify a title for filenames
pub fn slugify(text: &str) -> String {
    let mut slug = text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    
    while slug.contains("__") {
        slug = slug.replace("__", "_");
    }
    slug = slug.trim_matches('_').to_string();
    
    if slug.is_empty() {
        slug = "note".to_string();
    }
    slug
}

/// Save an episode both to the database and back to the vault, with loop prevention
pub async fn save_episode_bidirectional(
    episode: &EpisodeSave,
    backend: &Arc<dyn StorageBackend>,
    store: &Arc<MarkdownStore>,
    ignore_list: &WatchIgnoreList,
) -> Result<String> {
    // 1. Determine relative path
    let rel_path = match episode.vault_path {
        Some(ref vp) if !vp.is_empty() => vp.clone(),
        _ => {
            let slug = slugify(&episode.title);
            let filename = format!("{}.md", slug);
            
            // Format markdown to check for collisions/duplicates
            let markdown = format_episode_markdown(episode);
            
            let resolved_abs = organize_file(&store.vault_root, "episodes", &filename, &markdown)?;
            resolved_abs.strip_prefix(&store.vault_root)
                .unwrap_or(&resolved_abs)
                .to_string_lossy()
                .to_string()
        }
    };

    // 2. Prepare EpisodeSave with resolved vault path
    let mut episode_to_save = episode.clone();
    episode_to_save.vault_path = Some(rel_path.clone());

    // 3. Save to database
    let db_id = backend.save_episode(&episode_to_save).await?;

    // 4. Save to vault
    let markdown = format_episode_markdown(&episode_to_save);
    let abs_path = store.vault_root.join(&rel_path);

    // Loop prevention: ignore the watcher event for this file
    ignore_list.ignore(abs_path.clone());

    // Write file using store (which does atomic write and secret scrubbing)
    store.write_file(&rel_path, &markdown)?;

    Ok(db_id)
}

/// Save a wisdom rule both to the database and back to the vault, with loop prevention
#[allow(dead_code)]
pub async fn save_wisdom_rule_bidirectional(
    rule: &WisdomRule,
    backend: &Arc<dyn StorageBackend>,
    store: &Arc<MarkdownStore>,
    ignore_list: &WatchIgnoreList,
) -> Result<String> {
    // 1. Determine relative path
    let rel_path = match rule.vault_path {
        Some(ref vp) if !vp.is_empty() => vp.clone(),
        _ => {
            let slug = slugify(&rule.target_pattern);
            let filename = format!("{}.md", slug);
            
            let markdown = format_wisdom_markdown(rule);
            let tier_subfolder = format!("wisdom/{}", rule.tier);
            
            let resolved_abs = organize_file(&store.vault_root, &tier_subfolder, &filename, &markdown)?;
            resolved_abs.strip_prefix(&store.vault_root)
                .unwrap_or(&resolved_abs)
                .to_string_lossy()
                .to_string()
        }
    };

    // 2. Prepare WisdomRule with resolved vault path
    let mut rule_to_save = rule.clone();
    rule_to_save.vault_path = Some(rel_path.clone());

    // 3. Save to database
    let db_id = backend.save_wisdom_rule(&rule_to_save).await?;

    // 4. Save to vault
    let markdown = format_wisdom_markdown(&rule_to_save);
    let abs_path = store.vault_root.join(&rel_path);

    // Loop prevention: ignore the watcher event for this file
    ignore_list.ignore(abs_path.clone());

    // Write file using store
    store.write_file(&rel_path, &markdown)?;

    Ok(db_id)
}

pub fn format_episode_markdown(episode: &EpisodeSave) -> String {
    let mut yaml_val = serde_json::Map::new();
    yaml_val.insert("title".to_string(), serde_json::json!(episode.title));
    if let Some(ref s) = episode.scope {
        yaml_val.insert("scope".to_string(), serde_json::json!(s));
    }
    if !episode.entities.is_empty() {
        yaml_val.insert("entities".to_string(), serde_json::json!(episode.entities));
    }

    let yaml_str = serde_yaml::to_string(&yaml_val).unwrap_or_default();
    format!("---\n{}---\n{}", yaml_str.trim(), episode.content)
}

#[allow(dead_code)]
pub fn format_wisdom_markdown(rule: &WisdomRule) -> String {
    let mut yaml_val = serde_json::Map::new();
    yaml_val.insert("target_pattern".to_string(), serde_json::json!(rule.target_pattern));
    yaml_val.insert("action_to_avoid".to_string(), serde_json::json!(rule.action_to_avoid));
    yaml_val.insert("causal_explanation".to_string(), serde_json::json!(rule.causal_explanation));
    yaml_val.insert("prescribed_remedy".to_string(), serde_json::json!(rule.prescribed_remedy));
    yaml_val.insert("tier".to_string(), serde_json::json!(rule.tier));
    yaml_val.insert("scope".to_string(), serde_json::json!(rule.scope));
    yaml_val.insert("source_episodes".to_string(), serde_json::json!(rule.source_episodes));
    yaml_val.insert("generator_name".to_string(), serde_json::json!(rule.generator_name));

    let yaml_str = serde_yaml::to_string(&yaml_val).unwrap_or_default();
    format!("---\n{}---\n", yaml_str.trim())
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
        
        let results = backend.search("Body content", Some("watcher-testing"), false, 1, 0).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Watcher Test");
    }

    #[tokio::test]
    async fn test_bidirectional_sync_and_loop_prevention() {
        let temp = tempdir().unwrap();
        let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await.unwrap());
        backend.init().await.unwrap();
        
        let store = Arc::new(MarkdownStore::new(temp.path()).unwrap());
        let ignore_list = Arc::new(WatchIgnoreList::new());
        
        // 1. Save an episode via save_episode_bidirectional
        let episode = EpisodeSave {
            title: "Bidirectional Test".to_string(),
            content: "This is some bidirectional sync body content.".to_string(),
            entities: vec![],
            scope: Some("bi-testing".to_string()),
            vault_path: None,
            source_episode: None,
        };
        
        let ep_id = save_episode_bidirectional(&episode, &backend, &store, &ignore_list).await.unwrap();
        assert!(ep_id.contains("episode:"));
        
        // Check that file was written to vault
        let expected_rel_path = "episodes/bidirectional_test.md";
        let abs_path = temp.path().join(expected_rel_path);
        assert!(abs_path.exists());
        
        // 2. Loop Prevention Check: Check that the written file path is in the ignore list
        assert!(ignore_list.is_ignored(&abs_path));
        
        // Verify content in DB
        let results = backend.search("bidirectional sync", Some("bi-testing"), false, 1, 0).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Bidirectional Test");
        
        // 3. Watcher side: modify file directly in vault and sync to DB
        // Wait 2.1 seconds so the ignore list entry expires
        tokio::time::sleep(tokio::time::Duration::from_millis(2100)).await;
        assert!(!ignore_list.is_ignored(&abs_path));
        
        // Write new content directly to vault
        let new_content = "---\ntitle: \"Watcher Test Updated\"\nscope: \"bi-testing\"\n---\nNew updated body content.";
        std::fs::write(&abs_path, new_content).unwrap();
        
        // Trigger manual sync
        sync_file_to_db(&abs_path, &backend, &store).await.unwrap();
        
        // Verify DB got updated
        let results2 = backend.search("updated body", Some("bi-testing"), false, 1, 0).await.unwrap();
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].title, "Watcher Test Updated");
    }
}
