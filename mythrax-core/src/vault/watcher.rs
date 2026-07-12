use notify::{Watcher, RecursiveMode, Event, RecommendedWatcher};
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use tokio::sync::mpsc;
use anyhow::{Result, Context};
use crate::store::MarkdownStore;
use crate::db::StorageBackend;
use crate::contracts::{EpisodeSave, WisdomRule, Entity, WikiNode};
use surrealdb_types::SurrealValue;
use crate::vault::markdown::{parse_frontmatter, extract_plain_text};
use crate::vault::organization::organize_file;

pub struct WatchIgnoreList {
    ignored: Mutex<HashMap<PathBuf, Instant>>,
    ignored_hashes: Mutex<HashSet<u64>>,
    pub write_tx: Mutex<Option<mpsc::UnboundedSender<(PathBuf, String, Arc<MarkdownStore>)>>>,
}

impl Default for WatchIgnoreList {
    fn default() -> Self {
        Self::new()
    }
}

impl WatchIgnoreList {
    pub fn new() -> Self {
        Self {
            ignored: Mutex::new(HashMap::new()),
            ignored_hashes: Mutex::new(HashSet::new()),
            write_tx: Mutex::new(None),
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

    pub fn ignore_hash(&self, hash: u64) {
        let mut hashes = self.ignored_hashes.lock().unwrap();
        hashes.insert(hash);
    }

    pub fn is_hash_ignored(&self, hash: &u64) -> bool {
        let mut hashes = self.ignored_hashes.lock().unwrap();
        hashes.remove(hash)
    }

    pub fn queue_write(&self, path: PathBuf, content: &str, store: Arc<MarkdownStore>) {
        let tx = self.write_tx.lock().unwrap();
        if let Some(sender) = tx.as_ref() {
            let _ = sender.send((path, content.to_string(), store));
        } else {
            // Fallback: Write immediately
            let mut hasher = DefaultHasher::new();
            content.hash(&mut hasher);
            let hash = hasher.finish();
            
            self.ignore_hash(hash);
            self.ignore(path.clone());
            
            let rel_path = path.strip_prefix(&store.vault_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let _ = store.write_file(&rel_path, content);
        }
    }
}

fn count_files_recursive(path: &Path, depth: usize) -> usize {
    if depth > 10 {
        return 0;
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name == ".trash" || name == "target" || name == ".git" || name == ".mythrax" {
            return 0;
        }
    }
    if path.is_dir() {
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                count += count_files_recursive(&entry.path(), depth + 1);
            }
        }
        count
    } else {
        1
    }
}

fn get_vault_paths() -> (PathBuf, PathBuf) {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/Users/keith".to_string());
    let home_path = PathBuf::from(home);
    let vault_root = home_path.join("mythrax-vault");
    let global_dir = vault_root.join("global");
    (vault_root, global_dir)
}

pub fn start_watching(
    vault_root: PathBuf,
    ignore_list: Arc<WatchIgnoreList>,
    backend: Arc<dyn StorageBackend>,
    store: Arc<MarkdownStore>,
    dream_tx: Option<tokio::sync::mpsc::Sender<()>>,
) -> Result<RecommendedWatcher> {
    let vault_root = std::fs::canonicalize(&vault_root)
        .unwrap_or(vault_root);
    // Hard monitor limits: max recursion depth 10, max watch limit 50,000 files
    let mut total_files = count_files_recursive(&vault_root, 0);
    let (_, global_dir) = get_vault_paths();
    if global_dir.exists() && global_dir != vault_root.join("global") && !global_dir.starts_with(&vault_root) {
        total_files += count_files_recursive(&global_dir, 0);
    }
    if total_files > 50000 {
        return Err(anyhow::anyhow!("Vault exceeds hard limit of 50,000 files (found {})", total_files));
    }

    // 1. Setup Raw Event Channel
    let (tx, mut rx) = mpsc::channel::<notify::Result<Event>>(100);

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.blocking_send(res);
        },
        notify::Config::default(),
    )?;

    // Watch the main vault
    watcher.watch(&vault_root, RecursiveMode::Recursive)?;

    // Watch global directory if it exists and is outside the vault
    let (_, global_dir) = get_vault_paths();
    if global_dir.exists() && global_dir != vault_root.join("global") && !global_dir.starts_with(&vault_root) {
        watcher.watch(&global_dir, RecursiveMode::Recursive)?;
    }

    // 2. Setup Write-Behind Coalescing Queue (500ms delay)
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<(PathBuf, String, Arc<MarkdownStore>)>();
    {
        let mut guard = ignore_list.write_tx.lock().unwrap();
        *guard = Some(write_tx);
    }

    // Fixed 500ms delay for coalescing writes
    let delay = Duration::from_millis(500);

    let ignore_list_clone = ignore_list.clone();
    tokio::spawn(async move {
        let mut pending: HashMap<PathBuf, (String, Arc<MarkdownStore>, Instant)> = HashMap::new();
        loop {
            let sleep_dur = if let Some(earliest) = pending.values().map(|(_, _, t)| *t).min() {
                let now = Instant::now();
                if earliest > now {
                    earliest.duration_since(now)
                } else {
                    Duration::from_millis(0)
                }
            } else {
                Duration::from_millis(100)
            };

            tokio::select! {
                res = write_rx.recv() => {
                    if let Some((path, content, store_ref)) = res {
                        let flush_time = Instant::now() + delay;
                        pending.insert(path, (content, store_ref, flush_time));
                    } else {
                        break;
                    }
                }
                _ = tokio::time::sleep(sleep_dur) => {}
            }

            let now = Instant::now();
            let mut expired = Vec::new();
            for (path, (_, _, flush_time)) in &pending {
                if now >= *flush_time {
                    expired.push(path.clone());
                }
            }

            for path in expired {
                if let Some((content, store_ref, _)) = pending.remove(&path) {
                    let mut s = DefaultHasher::new();
                    content.hash(&mut s);
                    let hash = s.finish();

                    ignore_list_clone.ignore_hash(hash);
                    ignore_list_clone.ignore(path.clone());

                    let rel_path = path.strip_prefix(&store_ref.vault_root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();

                    if let Err(e) = store_ref.write_file(&rel_path, &content) {
                        tracing::error!("Coalescing queue failed to write file {:?}: {:?}", path, e);
                    }
                }
            }
        }
    });

    // 3. Setup Inbound Debouncing Queue (500ms delay)
    let (debounce_tx, mut debounce_rx) = mpsc::unbounded_channel::<(PathBuf, bool, Instant)>();

    // 4. Setup Worker Pool with Dynamic Concurrency Limit
    let (worker_tx, mut worker_rx) = mpsc::channel::<(PathBuf, bool)>(100);

    // Worker Pool Handler
    let backend_worker = backend.clone();
    let store_worker = store.clone();
    let dream_tx_worker = dream_tx.clone();

    tokio::spawn(async move {
        let max_concurrent_tasks = backend_worker.get_max_concurrent_tasks().await;
        let sem = Arc::new(tokio::sync::Semaphore::new(max_concurrent_tasks));

        while let Some((path, is_remove)) = worker_rx.recv().await {
            let sem_c = sem.clone();
            let b_c = backend_worker.clone();
            let s_c = store_worker.clone();
            let d_c = dream_tx_worker.clone();

            tokio::spawn(async move {
                let _permit = sem_c.acquire().await.unwrap();

                if is_remove {
                    let canonical_root = std::fs::canonicalize(&s_c.vault_root).unwrap_or_else(|_| s_c.vault_root.clone());
                    let rel_path_str = if let Ok(rel_path) = path.strip_prefix(&canonical_root) {
                        rel_path.to_string_lossy().to_string()
                    } else if let Ok(rel_path) = path.strip_prefix(&get_vault_paths().0) {
                        rel_path.to_string_lossy().to_string()
                    } else {
                        path.to_string_lossy().to_string()
                    };
                    tracing::info!("File removed from vault, deleting from DB: {}", rel_path_str);
                    if let Err(e) = b_c.delete_by_vault_path(&rel_path_str).await {
                        tracing::error!("Failed to delete file {} from DB: {:?}", rel_path_str, e);
                    }
                } else {
                    if let Err(e) = sync_file_to_db(&path, &b_c, &s_c).await {
                        tracing::error!("Failed to sync file {:?} to DB: {:?}", path, e);
                    } else {
                        if let Some(ref tx) = d_c {
                            let _ = tx.send(()).await;
                        }
                    }
                }
            });
        }
    });

    // 5. Main Event Loop with Filtering and Debouncing
    let vault_root_clone = vault_root.clone();
    let ignore_list_evt = ignore_list.clone();

    tokio::spawn(async move {
        while let Some(res) = rx.recv().await {
            match res {
                Ok(mut event) => {
                    // Filter events inside the callback
                    event.paths.retain(|path| {
                        let (_, global_path) = get_vault_paths();
                        // Discard symbolic links
                        let is_sym = std::fs::symlink_metadata(path)
                            .map(|m| m.file_type().is_symlink())
                            .unwrap_or(false);
                        if is_sym {
                            return false;
                        }

                        // Check ignored directories
                        let has_ignored_comp = path.components().any(|comp| {
                            let comp_str = comp.as_os_str().to_string_lossy();
                            comp_str == ".trash"
                                || comp_str == "target"
                                || comp_str == ".git"
                                || comp_str == ".mythrax"
                        });
                        if has_ignored_comp {
                            return false;
                        }

                        // Check depth <= 10 relative to root
                        let root_to_use = if path.starts_with(&vault_root_clone) {
                            Some(vault_root_clone.as_path())
                        } else {
                            if path.starts_with(&global_path) {
                                Some(global_path.as_path())
                            } else {
                                None
                            }
                        };

                        if let Some(r) = root_to_use {
                            if let Ok(rel) = path.strip_prefix(r) {
                                rel.components().count() <= 10
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    });

                    if event.paths.is_empty() {
                        continue;
                    }

                    // Only process modify/create/remove events for .md files
                    if event.kind.is_modify() || event.kind.is_create() {
                        for path in event.paths {
                            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                                continue;
                            }

                            if ignore_list_evt.is_ignored(&path) {
                                tracing::debug!("Watcher ignoring path: {:?}", path);
                                continue;
                            }

                            // Check hash suppression
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let mut s = DefaultHasher::new();
                                content.hash(&mut s);
                                let hash = s.finish();
                                if ignore_list_evt.is_hash_ignored(&hash) {
                                    tracing::debug!("Watcher ignoring path due to hash match: {:?}", path);
                                    continue;
                                }
                            }

                            let flush_time = Instant::now() + Duration::from_millis(500);
                            let _ = debounce_tx.send((path, false, flush_time));
                        }
                    } else if event.kind.is_remove() {
                        for path in event.paths {
                            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                                continue;
                            }
                            let flush_time = Instant::now() + Duration::from_millis(500);
                            let _ = debounce_tx.send((path, true, flush_time));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Watcher error: {:?}", e);
                }
            }
        }
    });

    // 6. Debounce Handler: Collects events, waits for timeout, then sends to Worker Pool
    tokio::spawn(async move {
        let mut pending: HashMap<PathBuf, (bool, Instant)> = HashMap::new();
        loop {
            let sleep_dur = if let Some((_, earliest)) = pending.values().min_by_key(|(_, t)| t) {
                let now = Instant::now();
                if earliest > &now {
                    earliest.duration_since(now)
                } else {
                    Duration::from_millis(0)
                }
            } else {
                Duration::from_millis(100)
            };

            tokio::select! {
                res = debounce_rx.recv() => {
                    if let Some((path, is_remove, flush_time)) = res {
                        pending.insert(path, (is_remove, flush_time));
                    } else {
                        break;
                    }
                }
                _ = tokio::time::sleep(sleep_dur) => {}
            }

            let now = Instant::now();
            let mut expired = Vec::new();
            for (path, (_, flush_time)) in &pending {
                if now >= *flush_time {
                    expired.push(path.clone());
                }
            }

            for path in expired {
                if let Some((is_remove, _)) = pending.remove(&path) {
                    let _ = worker_tx.send((path, is_remove)).await;
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

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct FrontmatterEdge {
    pub target: String,
    pub relation: Option<String>,
    pub strength: Option<f32>,
}

#[derive(serde::Deserialize)]
struct WikiFrontmatter {
    name: Option<String>,
    scope: Option<String>,
    edges: Option<Vec<FrontmatterEdge>>,
}

async fn resolve_target_to_id(
    target: &str,
    surreal_backend: &crate::db::SurrealBackend,
) -> Option<surrealdb::types::RecordId> {
    let cleaned = target.trim_start_matches("[[").trim_end_matches("]]").trim();
    if cleaned.contains(':') {
        if let Ok(rec_id) = crate::db::parse_record_id(cleaned) {
            return Some(rec_id);
        }
    }
    
    // Query wiki_node
    let q = "SELECT VALUE id FROM wiki_node WHERE name = $target LIMIT 1;";
    if let Ok(mut resp) = surreal_backend.db.query(q).bind(("target", cleaned)).await {
        if let Ok(Some(id)) = resp.take::<Option<surrealdb::types::RecordId>>(0) {
            return Some(id);
        }
    }
    
    // Fallback to episode
    let q = "SELECT VALUE id FROM episode WHERE title = $target LIMIT 1;";
    if let Ok(mut resp) = surreal_backend.db.query(q).bind(("target", cleaned)).await {
        if let Ok(Some(id)) = resp.take::<Option<surrealdb::types::RecordId>>(0) {
            return Some(id);
        }
    }
    
    // Fallback to wisdom
    let q = "SELECT VALUE id FROM wisdom WHERE target_pattern = $target LIMIT 1;";
    if let Ok(mut resp) = surreal_backend.db.query(q).bind(("target", cleaned)).await {
        if let Ok(Some(id)) = resp.take::<Option<surrealdb::types::RecordId>>(0) {
            return Some(id);
        }
    }
    
    None
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
    utility: Option<f32>,
    status: Option<String>,
    superseded_at: Option<String>,
    superseded_by: Option<String>,
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
    let canonical_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let canonical_root = std::fs::canonicalize(&store.vault_root).unwrap_or_else(|_| store.vault_root.clone());
    let rel_path = if let Ok(rel) = canonical_path.strip_prefix(&canonical_root) {
        rel.to_string_lossy().to_string()
    } else if let Ok(rel) = canonical_path.strip_prefix(&get_vault_paths().0) {
        rel.to_string_lossy().to_string()
    } else {
        canonical_path.to_string_lossy().to_string()
    };

    if rel_path.contains("episodes/") {
        let frontmatter: EpisodeFrontmatter = yaml_opt
            .and_then(|y| serde_json::to_value(y).ok())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or(EpisodeFrontmatter { title: None, scope: None, entities: None });

        let title = frontmatter.title.unwrap_or_else(|| {
            path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string()
        });

        let episode = EpisodeSave::builder(title, plain_body)
            .entities(frontmatter.entities.unwrap_or_default())
            .scope(frontmatter.scope)
            .vault_path(Some(rel_path))
            .build();

        backend.save_episode(&episode).await?;
    } else if rel_path.contains("wisdom/") || rel_path.starts_with("global/") {
        if let Some(yaml_val) = yaml_opt {
            let frontmatter: WisdomFrontmatter = serde_json::from_value(
                serde_json::to_value(yaml_val).unwrap_or_default()
            ).context("Failed to parse Wisdom frontmatter")?;

            let is_global = rel_path.starts_with("global/") || rel_path.contains("/global/");
            let final_tier = if is_global {
                "permanent".to_string()
            } else {
                frontmatter.tier.unwrap_or_else(|| {
                    if rel_path.contains("wisdom/skills/") {
                        "skills".to_string()
                    } else {
                        "dynamic".to_string()
                    }
                })
            };

            let final_scope = if is_global {
                "general".to_string()
            } else {
                frontmatter.scope.unwrap_or_else(|| "general".to_string())
            };

            let rule = WisdomRule {
                id: None,
                target_pattern: frontmatter.target_pattern,
                action_to_avoid: frontmatter.action_to_avoid,
                causal_explanation: frontmatter.causal_explanation,
                prescribed_remedy: frontmatter.prescribed_remedy,
                tier: final_tier,
                scope: final_scope,
                vault_path: Some(rel_path),
                embedding: None,
                source_episodes: frontmatter.source_episodes.unwrap_or_default(),
                generator_name: frontmatter.generator_name.unwrap_or_else(|| "manual".to_string()),
                similarity: None,
                utility: frontmatter.utility,
                status: frontmatter.status,
                superseded_at: frontmatter.superseded_at,
                superseded_by: frontmatter.superseded_by,
                rule_type: None,
            };

            backend.save_wisdom_rule(&rule).await?;
        }
    } else {
        let frontmatter: WikiFrontmatter = yaml_opt
            .and_then(|y| serde_json::to_value(y).ok())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or(WikiFrontmatter { name: None, scope: None, edges: None });

        let name = frontmatter.name.unwrap_or_else(|| {
            path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string()
        });

        let node = WikiNode {
            id: None,
            name: name.clone(),
            content: plain_body.clone(),
            scope: frontmatter.scope.unwrap_or_else(|| "general".to_string()),
            vault_path: Some(rel_path),
            embedding: None,
        };

        let db_id = backend.save_wiki_node(&node).await?;

        if let Some(surreal_backend) = backend.as_any().downcast_ref::<crate::db::SurrealBackend>() {
            let from_id = crate::db::parse_record_id(&db_id)?;
            
            // Parse body wikilinks using zero-regex robust scanner
            let body_links = crate::parser::extract_wiki_links(&body);

            // Resolve desired relations
            let mut desired: Vec<(surrealdb::types::RecordId, String, Option<f32>)> = Vec::new();
            if let Some(ref edges) = frontmatter.edges {
                for edge in edges {
                    if let Some(target_id) = resolve_target_to_id(&edge.target, surreal_backend).await {
                        let relation = edge.relation.clone().unwrap_or_else(|| "related".to_string());
                        desired.push((target_id, relation, edge.strength));
                    }
                }
            }
            
            for link in body_links {
                if let Some(target_id) = resolve_target_to_id(&link, surreal_backend).await {
                    if !desired.iter().any(|(tid, rel, _)| tid == &target_id && rel == "related") {
                        desired.push((target_id, "related".to_string(), None));
                    }
                }
            }

            // Query existing relations
            let mut existing_resp = surreal_backend.db.query("SELECT id, relation, out FROM relates_to WHERE in = $from;")
                .bind(("from", from_id.clone()))
                .await?;
            
            #[derive(serde::Deserialize, surrealdb_types::SurrealValue)]
            struct RelatesToRaw {
                id: surrealdb::types::RecordId,
                relation: String,
                out: surrealdb::types::RecordId,
            }
            
            let existing: Vec<RelatesToRaw> = existing_resp.take(0)?;

            // Delete removed relations
            for ext in &existing {
                let is_still_desired = desired.iter().any(|(tid, rel, _)| {
                    tid == &ext.out && rel == &ext.relation
                });
                if !is_still_desired {
                    let delete_q = "DELETE FROM relates_to WHERE id = $rel_id;";
                    let _ = surreal_backend.db.query(delete_q)
                        .bind(("rel_id", ext.id.clone()))
                        .await;
                }
            }

            // Create new relations
            for (tid, rel, strength_opt) in desired {
                let already_exists = existing.iter().any(|ext| {
                    &ext.out == &tid && &ext.relation == &rel
                });
                if !already_exists {
                    let relate_q = "RELATE $from->relates_to->$to CONTENT { relation: $relation, strength: $strength };";
                    let _ = surreal_backend.db.query(relate_q)
                        .bind(("from", from_id.clone()))
                        .bind(("to", tid))
                        .bind(("relation", rel))
                        .bind(("strength", strength_opt))
                        .await;
                }
            }
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
    backend: &dyn StorageBackend,
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

    // Queue write operation to coalescing write-behind queue
    ignore_list.queue_write(abs_path, &markdown, store.clone());

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

    // Queue write operation to coalescing write-behind queue
    ignore_list.queue_write(abs_path, &markdown, store.clone());

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
    if let Some(utility) = rule.utility {
        yaml_val.insert("utility".to_string(), serde_json::json!(utility));
    }
    if let Some(status) = &rule.status {
        yaml_val.insert("status".to_string(), serde_json::json!(status));
    }
    if let Some(superseded_at) = &rule.superseded_at {
        yaml_val.insert("superseded_at".to_string(), serde_json::json!(superseded_at));
    }
    if let Some(superseded_by) = &rule.superseded_by {
        yaml_val.insert("superseded_by".to_string(), serde_json::json!(superseded_by));
    }

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
        
        let results = backend.search(
        "Body content",
        Some("watcher-testing"),
        false,
        1,
        0,
        0.55,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    ).await.unwrap();
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].title, "Watcher Test");
    }

    #[tokio::test]
    async fn test_bidirectional_sync_and_loop_prevention() {
        let temp = tempdir().unwrap();
        let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await.unwrap());
        backend.init().await.unwrap();
        
        let store = Arc::new(MarkdownStore::new(temp.path()).unwrap());
        let ignore_list = Arc::new(WatchIgnoreList::new());
        
        // 1. Save an episode via save_episode_bidirectional
        let episode = EpisodeSave::builder(
            "Bidirectional Test".to_string(),
            "This is some bidirectional sync body content.".to_string(),
        )
        .scope(Some("bi-testing".to_string()))
        .build();
        
        let ep_id = save_episode_bidirectional(&episode, backend.as_ref(), &store, &ignore_list).await.unwrap();
        assert!(ep_id.contains("episode:"));
        
        // Yield to allow the background write-behind queue to flush and update the ignore list
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        // Check that file was written to vault
        let expected_rel_path = "episodes/bidirectional_test.md";
        let abs_path = temp.path().join(expected_rel_path);
        assert!(abs_path.exists());
        
        // 2. Loop Prevention Check: Check that the written file path is in the ignore list
        assert!(ignore_list.is_ignored(&abs_path));
        
        // Verify content in DB
        let results = backend.search(
        "bidirectional sync",
        Some("bi-testing"),
        false,
        1,
        0,
        0.55,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    ).await.unwrap();
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].title, "Bidirectional Test");
        
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
        let results2 = backend.search(
        "updated body",
        Some("bi-testing"),
        false,
        1,
        0,
        0.55,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    ).await.unwrap();
        assert_eq!(results2.results.len(), 1);
        assert_eq!(results2.results[0].title, "Watcher Test Updated");
    }

    #[tokio::test]
    async fn test_watcher_sync_skills_tier() {
        let temp = tempdir().unwrap();
        let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await.unwrap());
        backend.init().await.unwrap();
        
        let store = Arc::new(MarkdownStore::new(temp.path()).unwrap());
        
        let relative_path = "wisdom/skills/test_skill_rule.md";
        let rule_content = "---\ntarget_pattern: \"test-pattern\"\naction_to_avoid: \"test-action\"\ncausal_explanation: \"test-explanation\"\nprescribed_remedy: \"test-remedy\"\n---\n";
        store.write_file(relative_path, rule_content).unwrap();
        
        sync_file_to_db(&temp.path().join(relative_path), &backend, &store).await.unwrap();
        
        // Retrieve wisdom rule using get_wisdom with tier "skills"
        let results = backend.get_wisdom("test-pattern", Some("skills"), 10, 0, 0.0).await.unwrap();
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].tier, "skills");
        assert_eq!(results.results[0].target_pattern, "test-pattern");
    }
}
