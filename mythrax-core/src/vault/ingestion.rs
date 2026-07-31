use crate::db::StorageBackend;
use crate::store::MarkdownStore;
use anyhow::Result;
use std::path::{Path, PathBuf};
use surrealdb_types::SurrealValue;
use uuid::Uuid;

pub static IS_INGESTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub struct IngestionGuard;
impl IngestionGuard {
    pub fn new() -> Self {
        IS_INGESTING.store(true, std::sync::atomic::Ordering::SeqCst);
        Self
    }
}
impl Drop for IngestionGuard {
    fn drop(&mut self) {
        IS_INGESTING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

fn ingest_cursor(path: &Path) -> Result<String> {
    let conn = rusqlite::Connection::open(path)?;
    let mut stmt = conn.prepare(
        "SELECT key, value FROM ItemTable WHERE key LIKE 'composer:%' OR key LIKE 'chat:%';",
    )?;
    let mut rows = stmt.query([])?;
    let mut result = String::new();
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        result.push_str(&format!("### Key: {}\n", key));
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&value) {
            result.push_str(&format!(
                "```json\n{}\n```\n\n",
                serde_json::to_string_pretty(&json_val)?
            ));
        } else {
            result.push_str(&format!("{}\n\n", value));
        }
    }
    if result.is_empty() {
        anyhow::bail!("No composer or chat entries found in ItemTable");
    }
    Ok(result)
}

fn ingest_hermes(path: &Path) -> Result<String> {
    let conn = rusqlite::Connection::open(path)?;
    let mut table_name = "";
    let check_sql = "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('messages', 'chat_history');";
    let mut stmt = conn.prepare(check_sql)?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        let name: String = row.get(0)?;
        if name == "messages" {
            table_name = "messages";
        } else if name == "chat_history" {
            table_name = "chat_history";
        }
    }

    if table_name.is_empty() {
        anyhow::bail!("Neither 'messages' nor 'chat_history' tables found in sqlite");
    }

    let query_sql = format!("SELECT role, content FROM {};", table_name);
    let mut stmt = conn.prepare(&query_sql)?;
    let mut rows = stmt.query([])?;
    let mut result = String::new();
    while let Some(row) = rows.next()? {
        let role: String = row.get(0)?;
        let content: String = row.get(1)?;
        result.push_str(&format!("**{}**: {}\n\n", role, content));
    }
    if result.is_empty() {
        anyhow::bail!("No messages found in table {}", table_name);
    }
    Ok(result)
}

async fn parse_antigravity_log(
    path: &Path,
    db: &dyn StorageBackend,
    session_id: &str,
) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    use std::io::BufRead;
    let mut markdown = String::new();
    let mut user_turn_count = 0;
    for line_res in reader.lines() {
        let line = line_res?;
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) {
            let step_type = obj["type"].as_str().unwrap_or("");
            if step_type == "USER_INPUT" {
                if let Some(content) = obj["content"].as_str() {
                    markdown.push_str(&format!("## User Request\n{}\n\n", content));

                    user_turn_count += 1;
                    if user_turn_count > 1 {
                        let lower = content.to_lowercase();
                        let is_correction = lower.contains("wrong")
                            || lower.contains("forgot")
                            || lower.contains("incorrect")
                            || lower.contains("mistake")
                            || lower.contains("should have")
                            || lower.contains("actually")
                            || lower.contains("not right")
                            || lower.contains("that was a mistake")
                            || lower.contains("that's wrong");

                        if is_correction {
                            if let Some(surreal) =
                                db.as_any().downcast_ref::<crate::db::SurrealBackend>()
                            {
                                let task = crate::db::cognitive_tasks::CognitiveTask {
                                    id: format!("cognitive_task:{}", uuid::Uuid::new_v4()),
                                    task_type: "Extraction".to_string(),
                                    prompt: content.to_string(),
                                    system_instruction: "Analyze this user correction and extract a WisdomRule if applicable.".to_string(),
                                    expected_format: "Json".to_string(),
                                    priority: "Normal".to_string(),
                                    created_at: chrono::Utc::now(),
                                    status: "Pending".to_string(),
                                    result: None,
                                    ttl_minutes: 30,
                                    injected_at: None,
                                    session_id: Some(session_id.to_string()),
                                };
                                if let Err(e) = surreal.create_cognitive_task(&task).await {
                                    tracing::error!(
                                        "Failed to create cognitive task during bulk ingestion: {:?}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                }
            } else if step_type == "PLANNER_RESPONSE"
                && let Some(content) = obj["content"].as_str()
            {
                markdown.push_str(&format!("## Planner Response\n{}\n\n", content));
            }
        }
    }
    if markdown.is_empty() {
        anyhow::bail!("No user inputs or planner responses found in log");
    }
    Ok(markdown)
}

fn get_transcript_created_at(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    use std::io::BufRead;
    for line_res in reader.lines() {
        if let Ok(line) = line_res {
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(created_at) = obj["created_at"].as_str() {
                    return Some(created_at.to_string());
                }
            }
        }
    }
    None
}

fn get_folder_created_at_fallback(path: &Path) -> String {
    let metadata = std::fs::metadata(path);
    let mtime = metadata
        .and_then(|m| m.modified())
        .unwrap_or_else(|_| std::time::SystemTime::now());
    let dt: chrono::DateTime<chrono::Utc> = mtime.into();
    dt.to_rfc3339()
}

fn parse_claude_log(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    use std::io::BufRead;
    let mut markdown = String::new();
    for line_res in reader.lines() {
        let line = line_res?;
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(messages) = obj["messages"].as_array() {
                for msg in messages {
                    let role = msg["role"].as_str().unwrap_or("unknown");
                    let content = msg["content"].as_str().unwrap_or("");
                    markdown.push_str(&format!("**{}**: {}\n\n", role, content));
                }
            } else {
                let role = obj["role"].as_str().unwrap_or("unknown");
                let content = obj["content"].as_str().unwrap_or("");
                markdown.push_str(&format!("**{}**: {}\n\n", role, content));
            }
        }
    }
    if markdown.is_empty() {
        anyhow::bail!("No messages found in Claude JSONL");
    }
    Ok(markdown)
}

fn parse_opencode_log(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    let json_val: serde_json::Value = serde_json::from_str(&content)?;
    let mut markdown = String::new();
    if let Some(arr) = json_val.as_array() {
        for msg in arr {
            let role = msg["role"].as_str().unwrap_or("unknown");
            let content = msg["content"].as_str().unwrap_or("");
            markdown.push_str(&format!("**{}**: {}\n\n", role, content));
        }
    } else if let Some(arr) = json_val["messages"].as_array() {
        for msg in arr {
            let role = msg["role"].as_str().unwrap_or("unknown");
            let content = msg["content"].as_str().unwrap_or("");
            markdown.push_str(&format!("**{}**: {}\n\n", role, content));
        }
    }
    if markdown.is_empty() {
        anyhow::bail!("No messages found in OpenCode JSON");
    }
    Ok(markdown)
}

fn parse_openclaw_log(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&content) {
        let mut markdown = String::new();
        if let Some(arr) = json_val.as_array() {
            for msg in arr {
                let role = msg["role"].as_str().unwrap_or("unknown");
                let content = msg["content"].as_str().unwrap_or("");
                markdown.push_str(&format!("**{}**: {}\n\n", role, content));
            }
        } else if let Some(arr) = json_val["messages"].as_array() {
            for msg in arr {
                let role = msg["role"].as_str().unwrap_or("unknown");
                let content = msg["content"].as_str().unwrap_or("");
                markdown.push_str(&format!("**{}**: {}\n\n", role, content));
            }
        }
        if !markdown.is_empty() {
            return Ok(markdown);
        }
    }

    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    use std::io::BufRead;
    let mut markdown = String::new();
    for line_res in reader.lines() {
        let line = line_res?;
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) {
            let role = obj["role"].as_str().unwrap_or("unknown");
            let content = obj["content"].as_str().unwrap_or("");
            markdown.push_str(&format!("**{}**: {}\n\n", role, content));
        }
    }
    if markdown.is_empty() {
        anyhow::bail!("No messages found in OpenClaw content");
    }
    Ok(markdown)
}

fn parse_generic_jsonl(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    use std::io::BufRead;
    let mut markdown = String::new();
    for line_res in reader.lines() {
        let line = line_res?;
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) {
            let role = obj["role"]
                .as_str()
                .or_else(|| obj["speaker"].as_str())
                .unwrap_or("unknown");
            let content = obj["content"]
                .as_str()
                .or_else(|| obj["text"].as_str())
                .or_else(|| obj["message"].as_str())
                .unwrap_or("");
            markdown.push_str(&format!("**{}**: {}\n\n", role, content));
        }
    }
    if markdown.is_empty() {
        anyhow::bail!("No messages found in generic JSONL");
    }
    Ok(markdown)
}

fn parse_generic_markdown(path: &Path, scope: &str) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    if content.starts_with("---") {
        Ok(content)
    } else {
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("note");
        Ok(format!(
            "---\ntitle: \"{}\"\nscope: \"{}\"\n---\n\n{}\n\n## Synthesized Into\n- [[wiki/{}/MOC|Scope Map of Content]]\n\n## Temporal Navigation\n- **Sequence**: Episode captured in timeline\n",
            file_stem, scope, content, scope
        ))
    }
}

fn parse_codex_log(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&content) {
        let mut markdown = String::new();
        if let Some(arr) = json_val.as_array() {
            for msg in arr {
                let role = msg["role"].as_str().unwrap_or("unknown");
                let content = msg["content"].as_str().unwrap_or("");
                markdown.push_str(&format!("**{}**: {}\n\n", role, content));
            }
        } else if let Some(arr) = json_val["messages"].as_array() {
            for msg in arr {
                let role = msg["role"].as_str().unwrap_or("unknown");
                let content = msg["content"].as_str().unwrap_or("");
                markdown.push_str(&format!("**{}**: {}\n\n", role, content));
            }
        }
        if !markdown.is_empty() {
            return Ok(markdown);
        }
    }

    let mut markdown = String::new();
    let mut current_role = String::new();
    let mut current_content = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("role =") || trimmed.starts_with("role=") {
            if let Some(idx) = trimmed.find('"')
                && let Some(end_idx) = trimmed[idx + 1..].find('"')
            {
                current_role = trimmed[idx + 1..idx + 1 + end_idx].to_string();
            }
        } else if (trimmed.starts_with("content =") || trimmed.starts_with("content="))
            && let Some(idx) = trimmed.find('"')
            && let Some(end_idx) = trimmed[idx + 1..].find('"')
        {
            current_content = trimmed[idx + 1..idx + 1 + end_idx].to_string();
        }
        if !current_role.is_empty() && !current_content.is_empty() {
            markdown.push_str(&format!("**{}**: {}\n\n", current_role, current_content));
            current_role.clear();
            current_content.clear();
        }
    }

    if markdown.is_empty() {
        if content.trim().is_empty() {
            anyhow::bail!("Codex log file is empty");
        }
        Ok(content)
    } else {
        Ok(markdown)
    }
}

fn quarantine_file(file_path: &Path, source_dir: &Path, error_msg: &str) -> String {
    let quarantine_dir = source_dir.join("quarantine");
    let _ = std::fs::create_dir_all(&quarantine_dir);
    let filename = file_path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("unknown_file"));
    let dest_path = quarantine_dir.join(filename);
    let move_res = std::fs::rename(file_path, &dest_path);
    if move_res.is_err() && std::fs::copy(file_path, &dest_path).is_ok() {
        let _ = std::fs::remove_file(file_path);
    }
    format!("Failed to parse {}: {}", file_path.display(), error_msg)
}

pub fn resolve_scope_from_path(path: &Path) -> Option<String> {
    let components: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Common generic directory names to skip
    let skip_names = [
        "brain",
        "antigravity",
        ".gemini",
        "episodes",
        "wiki",
        "wisdom",
        "general",
        "archive",
        "users",
        "keith",
        "documents",
        "repos",
        "workspace",
        "workspaces",
        "projects",
        ".system_generated",
        "logs",
        "messages",
        "quarantine",
        "tempmediastorage",
        "target",
        "src",
        "release",
        "debug",
        "git",
        "refs",
        "ref",
        "github",
        "lib",
        "bin",
        "tests",
        "test",
        "deps",
        "build",
        "dist",
        "node_modules",
        "vendor",
    ];

    // Check from right to left (deepest directory first)
    for comp_str in components.iter().rev() {
        // Skip dotfiles/directories starting with '.'
        if comp_str.starts_with('.') {
            continue;
        }

        // Skip UUIDs
        if Uuid::parse_str(comp_str).is_ok() {
            continue;
        }

        let lower = comp_str.to_lowercase();
        // Skip generic names, source, or anything containing "session"
        if skip_names
            .iter()
            .any(|&s| s == *comp_str || s == lower.as_str())
            || lower.contains("session")
            || lower == "source"
        {
            continue;
        }

        // Filter to keep only alphanumeric, '-', '_', '.'
        let normalized: String = comp_str
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
            .map(|c| c.to_ascii_lowercase())
            .collect::<String>()
            .trim_matches('.')
            .to_string();

        if !normalized.is_empty() {
            return Some(normalized);
        }
    }

    None
}

pub fn extract_scope_from_log(log_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(log_path).ok()?;

    // 1. Try parsing active workspaces from <user_information>
    if let Some(info_start) = content.find("<user_information>") {
        if let Some(info_offset) = content[info_start..].find("</user_information>") {
            let info_block = &content[info_start..info_start + info_offset];
            for line in info_block.lines() {
                if let Some(arrow_idx) = line.find(" -> ") {
                    let path_part = line[..arrow_idx].trim();
                    let path = Path::new(path_part);
                    if let Some(scope) = resolve_scope_from_path(path) {
                        return Some(scope);
                    }
                }
            }
        }
    }

    // Fallback: Existing generic scanner
    let mut scopes: Vec<String> = Vec::new();
    let folder_prefixes = [
        "/Documents/",
        "/repos/",
        "/workspace/",
        "/workspaces/",
        "/projects/",
    ];
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/keith".to_string());

    for prefix in &folder_prefixes {
        let mut start = 0;
        while let Some(idx) = content[start..].find(prefix) {
            let absolute_start = start + idx + prefix.len();
            let suffix = &content[absolute_start..];
            let len = suffix
                .chars()
                .take_while(|c| {
                    *c != '/'
                        && !c.is_whitespace()
                        && *c != '"'
                        && *c != '\''
                        && *c != ','
                        && *c != '\\'
                })
                .map(|c| c.len_utf8())
                .sum();
            if len > 0 {
                let scope = &suffix[..len];
                let normalized: String = scope
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
                    .map(|c| c.to_ascii_lowercase())
                    .collect::<String>()
                    .trim_matches('.')
                    .to_string();
                if !normalized.is_empty() {
                    let skip_names = [
                        "brain",
                        "antigravity",
                        ".gemini",
                        "episodes",
                        "wiki",
                        "wisdom",
                        "general",
                        "archive",
                        "users",
                        "keith",
                        "documents",
                        "repos",
                        "workspace",
                        "workspaces",
                        "projects",
                        ".system_generated",
                        "logs",
                        "messages",
                        "quarantine",
                        "tempmediastorage",
                        "target",
                        "src",
                        "release",
                        "debug",
                        "git",
                        "refs",
                        "ref",
                        "github",
                        "lib",
                        "bin",
                        "tests",
                        "test",
                        "deps",
                        "build",
                        "dist",
                        "node_modules",
                        "vendor",
                    ];
                    if !skip_names.iter().any(|&s| s == normalized) {
                        let clean_prefix = prefix.trim_matches('/');
                        let full_path = Path::new(&home).join(clean_prefix).join(&normalized);
                        if full_path.is_dir() {
                            scopes.push(normalized);
                        }
                    }
                }
            }
            start = absolute_start + len;
        }
    }

    if scopes.is_empty() {
        return None;
    }

    // If "mythrax" is the only match, return it.
    // Otherwise, prefer non-"mythrax" scopes.
    let non_mythrax: Vec<&String> = scopes.iter().filter(|s| **s != "mythrax").collect();

    if non_mythrax.is_empty() {
        scopes.first().map(|s| (*s).clone())
    } else {
        non_mythrax.first().map(|s| (*s).clone())
    }
}

async fn episode_title_exists(db: &dyn StorageBackend, title: &str) -> bool {
    if let Some(surreal) = db.as_any().downcast_ref::<crate::db::SurrealBackend>() {
        let sql = "SELECT VALUE id FROM episode WHERE title = $title LIMIT 1;";
        if let Ok(mut resp) = surreal.db.query(sql).bind(("title", title)).await {
            if let Ok(Some(_id)) = resp.take::<Option<surrealdb::types::RecordId>>(0) {
                return true;
            }
        }
    }
    false
}

pub async fn bulk_ingest_vault(
    vault_root: &Path,
    source_dir: &Path,
    harness_type: &str,
    scope: &str,
    db: &dyn StorageBackend,
    offset: Option<usize>,
    limit: Option<usize>,
    _skip_llm: bool,
) -> Result<(usize, Vec<String>, bool)> {
    let _ingestion_guard = IngestionGuard::new();
    crate::daemon::update_last_activity();
    let mut success_count = 0;
    let mut errors = Vec::new();
    let mut has_more = false;

    let store = MarkdownStore::new(vault_root)?;

    let mut existing_titles: std::collections::HashSet<String> = std::collections::HashSet::new();

    let find_files = |exts: &[&str]| -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(source_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    let path = entry.path();
                    if let Some(ext) = path.extension().and_then(|s| s.to_str())
                        && exts.contains(&ext.to_lowercase().as_str())
                        && !path.components().any(|c| c.as_os_str() == "quarantine")
                    {
                        files.push(path);
                    }
                }
            }
        }
        files
    };

    match harness_type {
        "antigravity" => {
            let mut dirs_with_time = Vec::new();
            if let Ok(entries) = std::fs::read_dir(source_dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        let path = entry.path();
                        let dir_name = path.file_name().unwrap_or_default().to_string_lossy();

                        // Skip if directory starts with '.'
                        if dir_name.starts_with('.') {
                            continue;
                        }

                        // Skip case-insensitive matches for: quarantine, tempmediastorage, git, refs, ref
                        let lower_name = dir_name.to_lowercase();
                        if lower_name == "quarantine"
                            || lower_name == "tempmediastorage"
                            || lower_name == "git"
                            || lower_name == "refs"
                            || lower_name == "ref"
                        {
                            continue;
                        }

                        let logs_dir = path.join(".system_generated/logs");
                        let log_exists = logs_dir.join("transcript.jsonl").exists()
                            || logs_dir.join("transcript_full.jsonl").exists();

                        let has_md = if let Ok(sub_entries) = std::fs::read_dir(&path) {
                            sub_entries.flatten().any(|se| {
                                se.path()
                                    .extension()
                                    .and_then(|ext| ext.to_str())
                                    .map(|ext| ext.eq_ignore_ascii_case("md"))
                                    .unwrap_or(false)
                            })
                        } else {
                            false
                        };

                        if log_exists || has_md {
                            let mut timestamp_ns: u128 = 0;
                            if log_exists {
                                let transcript_file = path.join(".system_generated/logs/transcript.jsonl");
                                if let Ok(file) = std::fs::File::open(&transcript_file) {
                                    use std::io::BufRead;
                                    let reader = std::io::BufReader::new(file);
                                    if let Some(Ok(line)) = reader.lines().next() {
                                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                                            if let Some(ts_str) = v.get("timestamp").and_then(|t| t.as_str()) {
                                                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_str) {
                                                    timestamp_ns = dt.timestamp_nanos_opt().unwrap_or(0) as u128;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if timestamp_ns == 0 {
                                let mtime = std::fs::metadata(&path)
                                    .and_then(|m| m.modified())
                                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                                timestamp_ns = mtime.duration_since(std::time::SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
                            }
                            dirs_with_time.push((path, timestamp_ns));
                        }
                    }
                }
            }

            dirs_with_time.sort_by_key(|d| d.1);
            let total_dirs = dirs_with_time.len();
            let start = offset.unwrap_or(0);
            let count = limit.unwrap_or(total_dirs);
            let end = (start + count).min(total_dirs);
            has_more = end < total_dirs;

            let dirs: Vec<std::path::PathBuf> = dirs_with_time[start..end]
                .iter()
                .map(|d| d.0.clone())
                .collect();

            let mut last_episode_id: Option<String> = None;

            for dir_chunk in dirs.chunks(50) {
                for (local_idx, path) in dir_chunk.iter().enumerate() {
                    let current_index = start + local_idx;
                    let dir_name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();

                    let title = format!("antigravity_{}", dir_name);

                    let part1_title = format!("{}_part1", title);
                    if existing_titles.contains(&title)
                        || existing_titles.contains(&part1_title)
                        || episode_title_exists(db, &title).await
                        || episode_title_exists(db, &part1_title).await
                    {
                        tracing::info!(
                            "processing episode {} of {} complete (skipped - already exists)",
                            current_index + 1,
                            total_dirs
                        );
                        continue;
                    }
                    existing_titles.insert(title.clone());

                    // Dynamically resolve scope for each conversation folder
                    let relative_path = path.strip_prefix(source_dir).unwrap_or(&path);
                    let resolved_scope =
                        resolve_scope_from_path(relative_path).unwrap_or_else(|| {
                            let logs_dir = path.join(".system_generated/logs");
                            let mut log_path = logs_dir.join("transcript.jsonl");
                            if !log_path.exists() {
                                log_path = logs_dir.join("transcript_full.jsonl");
                            }
                            if log_path.exists() {
                                extract_scope_from_log(&log_path)
                                    .unwrap_or_else(|| scope.to_string())
                            } else {
                                scope.to_string()
                            }
                        });

                    // 1. Pre-scan markdown artifacts in the conversation folder
                    let mut pre_scanned_artifacts = Vec::new();
                    if let Ok(file_entries) = std::fs::read_dir(&path) {
                        for file_entry in file_entries.flatten() {
                            let fpath = file_entry.path();
                            if fpath.is_file() {
                                let is_md = fpath
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| e.eq_ignore_ascii_case("md"))
                                    .unwrap_or(false);
                                if is_md {
                                    let file_stem = fpath
                                        .file_stem()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();
                                    if let Ok(artifact_content) = std::fs::read_to_string(&fpath) {
                                        if !artifact_content.trim().is_empty() {
                                            pre_scanned_artifacts
                                                .push((file_stem, artifact_content));
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Resolve and chunk the artifacts to keep prompts and embeddings bounded
                    let mut resolved_artifacts = Vec::new();
                    for (file_stem, raw_artifact_content) in pre_scanned_artifacts {
                        let artifact_chunks = chunk_parsed_content(&raw_artifact_content, 20_000);
                        let total_art_chunks = artifact_chunks.len();
                        for (art_idx, chunk_text) in artifact_chunks.into_iter().enumerate() {
                            // Use resolved_scope for unique, readable node name and vault paths
                            let node_name = if total_art_chunks > 1 {
                                format!("{}/{}_part{}", resolved_scope, file_stem, art_idx + 1)
                            } else {
                                format!("{}/{}", resolved_scope, file_stem)
                            };
                            let wiki_rel = if total_art_chunks > 1 {
                                format!(
                                    "wiki/{}/raw/{}_part{}.md",
                                    resolved_scope,
                                    file_stem,
                                    art_idx + 1
                                )
                            } else {
                                format!("wiki/{}/raw/{}.md", resolved_scope, file_stem)
                            };
                            let wikilink = if total_art_chunks > 1 {
                                format!(
                                    "wiki/{}/raw/{}_part{}",
                                    resolved_scope,
                                    file_stem,
                                    art_idx + 1
                                )
                            } else {
                                format!("wiki/{}/raw/{}", resolved_scope, file_stem)
                            };
                            resolved_artifacts.push((node_name, wiki_rel, wikilink, chunk_text));
                        }
                    }

                    // 2. Parse the transcript log
                    let logs_dir = path.join(".system_generated/logs");
                    let mut log_path = logs_dir.join("transcript.jsonl");
                    if !log_path.exists() {
                        log_path = logs_dir.join("transcript_full.jsonl");
                    }

                    let parsed_content = if log_path.exists() {
                        match parse_antigravity_log(&log_path, db, &dir_name).await {
                            Ok(content) => content,
                            Err(e) => {
                                let err_msg =
                                    quarantine_file(&log_path, source_dir, &e.to_string());
                                errors.push(err_msg);
                                continue;
                            }
                        }
                    } else {
                        continue;
                    };

                    let created_at_opt = if log_path.exists() {
                        get_transcript_created_at(&log_path)
                    } else {
                        None
                    }
                    .unwrap_or_else(|| get_folder_created_at_fallback(&path));

                    let uuid_suffix = uuid::Uuid::new_v4().to_string()[..8].to_string();

                    // Chunk the parsed log to keep prompt sizes bounded
                    let chunks = chunk_parsed_content(&parsed_content, 20_000);
                    let total_chunks = chunks.len();
                    let mut generated_parts = Vec::new();

                    let slug_title = crate::cognitive::pipeline::slugify_title(&title);
                    let parent_relative_path =
                        format!("episodes/{}_{}_{}.md", slug_title, &dir_name[..dir_name.len().min(8)], uuid_suffix);
                    let parent_title = title.clone();
                    let mut parent_saved_id = String::new();

                    // If multi-part, write the parent index document first and save it
                    if total_chunks > 1 {
                        let mut parent_parts_list = String::new();
                        parent_parts_list.push_str("\n\n## Parts\n");
                        for chunk_idx in 0..total_chunks {
                            let part_path = format!(
                                "episodes/{}_part{}_{}_{}",
                                slug_title,
                                chunk_idx + 1,
                                &dir_name[..dir_name.len().min(8)],
                                uuid_suffix
                            );
                            parent_parts_list.push_str(&format!("- [[{}]]\n", part_path));
                        }
                        let parent_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"antigravity\"\n---\n\n# {}\n{}",
                            parent_title, resolved_scope, parent_title, parent_parts_list
                        );

                        let parent_ep_save = crate::contracts::EpisodeSave::builder(
                            parent_title.clone(),
                            parent_content,
                        )
                        .scope(Some(resolved_scope.clone()))
                        .vault_path(Some(parent_relative_path.clone()))
                        .session_id(Some(dir_name.clone()))
                        .created_at(Some(created_at_opt.clone()))
                        .temporal_range_start(
                            chrono::DateTime::parse_from_rfc3339(&created_at_opt)
                                .ok()
                                .map(|dt| dt.with_timezone(&chrono::Utc)),
                        )
                        .temporal_range_end(
                            chrono::DateTime::parse_from_rfc3339(&created_at_opt)
                                .ok()
                                .map(|dt| dt.with_timezone(&chrono::Utc)),
                        )
                        .build();
                        if let Ok(ep_id) = db.save_episode(&parent_ep_save).await {
                            success_count += 1;
                            parent_saved_id = ep_id;
                        }
                    }

                    for (chunk_idx, chunk_text) in chunks.iter().enumerate() {
                        let part_title = if total_chunks > 1 {
                            format!("{} Part {}", title, chunk_idx + 1)
                        } else {
                            title.clone()
                        };

                        let relative_path = if total_chunks > 1 {
                            format!(
                                "episodes/{}_part{}_{}_{}.md",
                                slug_title,
                                chunk_idx + 1,
                                &dir_name[..dir_name.len().min(8)],
                                uuid_suffix
                            )
                        } else {
                            format!("episodes/{}_{}_{}.md", slug_title, &dir_name[..dir_name.len().min(8)], uuid_suffix)
                        };

                        let mut linked_artifacts_section = String::new();
                        if !resolved_artifacts.is_empty() {
                            linked_artifacts_section.push_str("\n\n## Linked Artifacts\n");
                            for (_, _, wikilink, _) in &resolved_artifacts {
                                linked_artifacts_section.push_str(&format!("- [[{}]]\n", wikilink));
                            }
                        }

                        // Collapsible navigation callout at the bottom of the chunk
                        let mut nav_callout = String::new();
                        if total_chunks > 1 {
                            nav_callout.push_str("\n\n> [!INFO]- Navigation\n");
                            let parent_target = parent_relative_path
                                .strip_suffix(".md")
                                .unwrap_or(&parent_relative_path);
                            nav_callout.push_str(&format!("> Parent: [[{}]]\n", parent_target));

                            let prev_str = if chunk_idx > 0 {
                                let prev_path = format!(
                                    "episodes/{}_part{}_{}_{}",
                                    slug_title, chunk_idx, &dir_name[..dir_name.len().min(8)], uuid_suffix
                                );
                                format!("[[{}]]", prev_path)
                            } else {
                                "None".to_string()
                            };

                            let next_str = if chunk_idx + 1 < total_chunks {
                                let next_path = format!(
                                    "episodes/{}_part{}_{}_{}",
                                    slug_title,
                                    chunk_idx + 2,
                                    &dir_name[..dir_name.len().min(8)],
                                    uuid_suffix
                                );
                                format!("[[{}]]", next_path)
                            } else {
                                "None".to_string()
                            };

                            nav_callout
                                .push_str(&format!("> Prev: {} | Next: {}\n", prev_str, next_str));
                        }

                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"antigravity\"\n---\n\n{}{}{}",
                            part_title,
                            resolved_scope,
                            chunk_text,
                            linked_artifacts_section,
                            nav_callout
                        );

                        let ep_save = crate::contracts::EpisodeSave::builder(
                            part_title.clone(),
                            note_content,
                        )
                        .scope(Some(resolved_scope.clone()))
                        .vault_path(Some(relative_path.clone()))
                        .session_id(Some(dir_name.clone()))
                        .created_at(Some(created_at_opt.clone()))
                        .temporal_range_start(
                            chrono::DateTime::parse_from_rfc3339(&created_at_opt)
                                .ok()
                                .map(|dt| dt.with_timezone(&chrono::Utc)),
                        )
                        .temporal_range_end(
                            chrono::DateTime::parse_from_rfc3339(&created_at_opt)
                                .ok()
                                .map(|dt| dt.with_timezone(&chrono::Utc)),
                        )
                        .build();
                        if let Ok(episode_saved_id) = db.save_episode(&ep_save).await {
                            success_count += 1;
                            generated_parts.push((part_title, relative_path, episode_saved_id));
                        }
                    }

                    // Downcast and establish SurrealDB relationships
                    if let Some(surreal) = db.as_any().downcast_ref::<crate::db::SurrealBackend>() {
                        if total_chunks > 1 && !parent_saved_id.is_empty() {
                            if let Ok(parent_thing) = crate::db::parse_record_id(&parent_saved_id) {
                                for (_, _, part_saved_id) in &generated_parts {
                                    if let Ok(child_thing) =
                                        crate::db::parse_record_id(part_saved_id)
                                    {
                                        let query_parent = "RELATE $child_thing -> relates_to -> $parent_thing UNIQUE CONTENT { relation: 'parent', created_at: time::now() };";
                                        let _ = surreal
                                            .db
                                            .query(query_parent)
                                            .bind(("child_thing", child_thing))
                                            .bind(("parent_thing", parent_thing.clone()))
                                            .await;
                                    }
                                }
                            }
                        }

                        for i in 0..generated_parts.len().saturating_sub(1) {
                            if let (Ok(part_n), Ok(part_n_plus_1)) = (
                                crate::db::parse_record_id(&generated_parts[i].2),
                                crate::db::parse_record_id(&generated_parts[i + 1].2),
                            ) {
                                let query_next = "RELATE $part_n -> relates_to -> $part_n_plus_1 UNIQUE CONTENT { relation: 'next', created_at: time::now() };";
                                let _ = surreal
                                    .db
                                    .query(query_next)
                                    .bind(("part_n", part_n.clone()))
                                    .bind(("part_n_plus_1", part_n_plus_1.clone()))
                                    .await;

                                let query_prev = "RELATE $part_n_plus_1 -> relates_to -> $part_n UNIQUE CONTENT { relation: 'prev', created_at: time::now() };";
                                let _ = surreal
                                    .db
                                    .query(query_prev)
                                    .bind(("part_n_plus_1", part_n_plus_1))
                                    .bind(("part_n", part_n))
                                    .await;
                            }
                        }
                    }

                    if let Some(surreal) = db.as_any().downcast_ref::<crate::db::SurrealBackend>() {
                        let current_primary_id = if total_chunks > 1 && !parent_saved_id.is_empty()
                        {
                            Some(parent_saved_id.clone())
                        } else if !generated_parts.is_empty() {
                            Some(generated_parts[0].2.clone())
                        } else {
                            None
                        };

                        if let Some(ref curr_id) = current_primary_id {
                            if let Some(ref last_id) = last_episode_id {
                                if let (Ok(last_thing), Ok(curr_thing)) = (
                                    crate::db::parse_record_id(last_id),
                                    crate::db::parse_record_id(curr_id),
                                ) {
                                    let query_followed = "RELATE $last_thing -> followed_by -> $curr_thing UNIQUE CONTENT { created_at: time::now() };";
                                    let _ = surreal
                                        .db
                                        .query(query_followed)
                                        .bind(("last_thing", last_thing))
                                        .bind(("curr_thing", curr_thing))
                                        .await;
                                }
                            }
                            last_episode_id = Some(curr_id.clone());
                        }
                    }

                    // 3. Process and write the artifacts, creating bidirectional wikilinks & SurrealDB edges
                    for (node_name, wiki_rel, _, chunk_text) in resolved_artifacts {
                        let mut backlink_footer = String::new();
                        if !generated_parts.is_empty() {
                            backlink_footer.push_str("\n\n---\nSource Episodes: ");
                            let links: Vec<String> = generated_parts
                                .iter()
                                .map(|(part_title, rel_path, _)| {
                                    let link_target =
                                        rel_path.strip_suffix(".md").unwrap_or(rel_path);
                                    format!("[[{}|{}]]", link_target, part_title)
                                })
                                .collect();
                            backlink_footer.push_str(&links.join(" | "));
                            backlink_footer.push('\n');
                        }

                        let artifact_content = format!("{}{}", chunk_text, backlink_footer);
                        let _ = store.write_file(&wiki_rel, &artifact_content);

                        let node = crate::contracts::WikiNode {
                            id: None,
                            name: node_name,
                            content: artifact_content,
                            scope: resolved_scope.clone(),
                            vault_path: Some(wiki_rel),
                            embedding: None,
                            item_type: Some("episode_summary".to_string()),
                            node_type: Some("episode_summary".to_string()),
                            ..Default::default()
                        };

                        if let Ok(wiki_node_id) = db.save_wiki_node(&node).await {
                            success_count += 1;
                            for (_, _, ep_saved_id) in &generated_parts {
                                let _ = db
                                    .relate_nodes(ep_saved_id, &wiki_node_id, None, None, None)
                                    .await;
                            }
                        }
                    }

                    // Log a clean progress message at INFO level
                    tracing::info!(
                        "processing episode {} of {} complete",
                        current_index + 1,
                        total_dirs
                    );
                } // End inner loop
            } // End outer loop
        }
        "claude" => {
            let files = find_files(&["jsonl"]);
            for file in files {
                let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                let title = format!("claude_{}", file_stem);
                if existing_titles.contains(&title) || episode_title_exists(db, &title).await {
                    continue;
                }
                match parse_claude_log(&file) {
                    Ok(content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = format!("claude_{}", file_stem);
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path =
                            format!("episodes/claude_{}_{}.md", file_stem, &uuid[..8]);

                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"claude\"\n---\n\n{}",
                            title, scope, content
                        );
                        let ep_save =
                            crate::contracts::EpisodeSave::builder(title, note_content)
                                .scope(Some(scope.to_string()))
                                .vault_path(Some(relative_path))
                                .build();
                        if db.save_episode(&ep_save).await.is_ok() {
                            success_count += 1;
                        }
                    }
                    Err(e) => {
                        let err_msg = quarantine_file(&file, source_dir, &e.to_string());
                        errors.push(err_msg);
                    }
                }
            }
        }
        "cursor" => {
            let db_path = source_dir.join("state.vscdb");
            if db_path.exists() {
                let title = "cursor_chat".to_string();
                if existing_titles.contains(&title) || episode_title_exists(db, &title).await {
                    return Ok((0, vec![], false));
                }
                match ingest_cursor(&db_path) {
                    Ok(content) => {
                        let title = "cursor_chat".to_string();
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path = format!("episodes/cursor_chat_{}.md", &uuid[..8]);
                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"cursor\"\n---\n\n{}",
                            title, scope, content
                        );
                        let ep_save =
                            crate::contracts::EpisodeSave::builder(title, note_content)
                                .scope(Some(scope.to_string()))
                                .vault_path(Some(relative_path))
                                .build();
                        if db.save_episode(&ep_save).await.is_ok() {
                            success_count += 1;
                        }
                    }
                    Err(e) => {
                        let err_msg = quarantine_file(&db_path, source_dir, &e.to_string());
                        errors.push(err_msg);
                    }
                }
            } else {
                errors.push("state.vscdb not found in source directory".to_string());
            }
        }
        "codex" => {
            let files = find_files(&["json", "jsonl", "toml", "txt"]);
            for file in files {
                let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                let title = format!("codex_{}", file_stem);
                if existing_titles.contains(&title) || episode_title_exists(db, &title).await {
                    continue;
                }
                match parse_codex_log(&file) {
                    Ok(content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = format!("codex_{}", file_stem);
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path =
                            format!("episodes/codex_{}_{}.md", file_stem, &uuid[..8]);

                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"codex\"\n---\n\n{}",
                            title, scope, content
                        );
                        let ep_save =
                            crate::contracts::EpisodeSave::builder(title, note_content)
                                .scope(Some(scope.to_string()))
                                .vault_path(Some(relative_path))
                                .build();
                        if db.save_episode(&ep_save).await.is_ok() {
                            success_count += 1;
                        }
                    }
                    Err(e) => {
                        let err_msg = quarantine_file(&file, source_dir, &e.to_string());
                        errors.push(err_msg);
                    }
                }
            }
        }
        "opencode" => {
            let files = find_files(&["json"]);
            for file in files {
                let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                let title = format!("opencode_{}", file_stem);
                if existing_titles.contains(&title) || episode_title_exists(db, &title).await {
                    continue;
                }
                match parse_opencode_log(&file) {
                    Ok(content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = format!("opencode_{}", file_stem);
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path =
                            format!("episodes/opencode_{}_{}.md", file_stem, &uuid[..8]);

                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"opencode\"\n---\n\n{}",
                            title, scope, content
                        );
                        let ep_save =
                            crate::contracts::EpisodeSave::builder(title, note_content)
                                .scope(Some(scope.to_string()))
                                .vault_path(Some(relative_path))
                                .build();
                        if db.save_episode(&ep_save).await.is_ok() {
                            success_count += 1;
                        }
                    }
                    Err(e) => {
                        let err_msg = quarantine_file(&file, source_dir, &e.to_string());
                        errors.push(err_msg);
                    }
                }
            }
        }
        "openclaw" => {
            let files = find_files(&["json", "jsonl"]);
            for file in files {
                let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                let title = format!("openclaw_{}", file_stem);
                if existing_titles.contains(&title) || episode_title_exists(db, &title).await {
                    continue;
                }
                match parse_openclaw_log(&file) {
                    Ok(content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = format!("openclaw_{}", file_stem);
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path =
                            format!("episodes/openclaw_{}_{}.md", file_stem, &uuid[..8]);

                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"openclaw\"\n---\n\n{}",
                            title, scope, content
                        );
                        let ep_save =
                            crate::contracts::EpisodeSave::builder(title, note_content)
                                .scope(Some(scope.to_string()))
                                .vault_path(Some(relative_path))
                                .build();
                        if db.save_episode(&ep_save).await.is_ok() {
                            success_count += 1;
                        }
                    }
                    Err(e) => {
                        let err_msg = quarantine_file(&file, source_dir, &e.to_string());
                        errors.push(err_msg);
                    }
                }
            }
        }
        "hermes" => {
            let db_path = source_dir.join("state.db");
            if db_path.exists() {
                let title = "hermes_chat".to_string();
                if existing_titles.contains(&title) || episode_title_exists(db, &title).await {
                    return Ok((0, vec![], false));
                }
                match ingest_hermes(&db_path) {
                    Ok(content) => {
                        let title = "hermes_chat".to_string();
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path = format!("episodes/hermes_chat_{}.md", &uuid[..8]);
                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"hermes\"\n---\n\n{}",
                            title, scope, content
                        );
                        let ep_save =
                            crate::contracts::EpisodeSave::builder(title, note_content)
                                .scope(Some(scope.to_string()))
                                .vault_path(Some(relative_path))
                                .build();
                        if db.save_episode(&ep_save).await.is_ok() {
                            success_count += 1;
                        }
                    }
                    Err(e) => {
                        let err_msg = quarantine_file(&db_path, source_dir, &e.to_string());
                        errors.push(err_msg);
                    }
                }
            } else {
                errors.push("state.db not found in source directory".to_string());
            }
        }
        "generic_jsonl" => {
            let files = find_files(&["jsonl"]);
            for file in files {
                let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                let title = format!("generic_{}", file_stem);
                if existing_titles.contains(&title) || episode_title_exists(db, &title).await {
                    continue;
                }
                match parse_generic_jsonl(&file) {
                    Ok(content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = format!("generic_{}", file_stem);
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path =
                            format!("episodes/generic_{}_{}.md", file_stem, &uuid[..8]);

                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"generic_jsonl\"\n---\n\n{}",
                            title, scope, content
                        );
                        let ep_save =
                            crate::contracts::EpisodeSave::builder(title, note_content)
                                .scope(Some(scope.to_string()))
                                .vault_path(Some(relative_path))
                                .build();
                        if db.save_episode(&ep_save).await.is_ok() {
                            success_count += 1;
                        }
                    }
                    Err(e) => {
                        let err_msg = quarantine_file(&file, source_dir, &e.to_string());
                        errors.push(err_msg);
                    }
                }
            }
        }
        "generic_markdown" => {
            let files = find_files(&["md"]);
            for file in files {
                let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                let title = file_stem.to_string();
                if existing_titles.contains(&title) || episode_title_exists(db, &title).await {
                    continue;
                }
                match parse_generic_markdown(&file, scope) {
                    Ok(note_content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = file_stem.to_string();
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path = format!("episodes/{}_{}.md", file_stem, &uuid[..8]);

                        let ep_save =
                            crate::contracts::EpisodeSave::builder(title, note_content)
                                .scope(Some(scope.to_string()))
                                .vault_path(Some(relative_path))
                                .build();
                        if db.save_episode(&ep_save).await.is_ok() {
                            success_count += 1;
                        }
                    }
                    Err(e) => {
                        let err_msg = quarantine_file(&file, source_dir, &e.to_string());
                        errors.push(err_msg);
                    }
                }
            }
        }
        other => anyhow::bail!("Unsupported harness type: {}", other),
    }

    if success_count > 0 {
        if let Err(e) = post_ingestion_compaction_and_cleanup(db, &store, scope).await {
            tracing::warn!("Post-ingestion compaction and cleanup returned error: {:?}", e);
        }
    }

    Ok((success_count, errors, has_more))
}

async fn post_ingestion_compaction_and_cleanup(
    db: &dyn StorageBackend,
    store: &MarkdownStore,
    scope: &str,
) -> Result<()> {
    if let Some(surreal) = db.as_any().downcast_ref::<crate::db::SurrealBackend>() {
        let db_arc = std::sync::Arc::new(crate::db::SurrealBackend::new_with_db(surreal.db.clone()));
        if let Err(e) = crate::cognitive::pipeline::refine_hypotheses(&*db_arc, None, scope).await {
            tracing::warn!("Auto scope compaction post-ingestion returned: {:?}", e);
        }

        if let Ok(mut response) = surreal
            .db
            .query("SELECT * FROM episode WHERE scope = $scope AND archived = true;")
            .bind(("scope", scope))
            .await
        {
            if let Ok(raws) = response.take::<Vec<crate::db::EpisodeRaw>>(0) {
                let archived_episodes: Vec<crate::contracts::Episode> = raws.into_iter().map(|r| r.into()).collect();
                for ep in archived_episodes {
                    if let Some(ref path) = ep.vault_path {
                        let file_name = std::path::Path::new(path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        let archive_rel_path = format!("archive/{}/{}", scope, file_name);
                        let src_path = store.vault_root.join(path);
                        let dst_path = store.vault_root.join(&archive_rel_path);
                        if let Some(parent) = dst_path.parent() {
                            if let Err(e) = std::fs::create_dir_all(parent) {
                                tracing::warn!("Failed to create archive directory {:?}: {:?}", parent, e);
                            }
                        }
                        if src_path.exists() {
                            if let Err(e) = std::fs::rename(&src_path, &dst_path) {
                                tracing::warn!("Failed to move archived episode from {:?} to {:?}: {:?}", src_path, dst_path, e);
                            } else if let Some(ref ep_id) = ep.id {
                                let id_part = ep_id.split(':').nth(1).unwrap_or(ep_id);
                                let update_sql = "UPDATE type::record('episode', $id) SET vault_path = $vp;";
                                if let Err(e) = surreal.db.query(update_sql).bind(("id", id_part)).bind(("vp", archive_rel_path.clone())).await {
                                    tracing::warn!("Failed to update database vault_path for archived episode '{}': {:?}", ep_id, e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let wiki_nodes = db.get_all_wiki_nodes().await.unwrap_or_default();
    let mut moc_content = format!("# Map of Content: {}\n\n## Wiki Nodes\n", scope);
    for node in wiki_nodes {
        if node.scope == scope {
            moc_content.push_str(&format!("- [[wiki/{}/{}|{}]]\n", scope, node.name, node.name));
        }
    }
    let moc_path = format!("wiki/{}/MOC.md", scope);
    store.write_file(&moc_path, &moc_content)?;

    Ok(())
}

fn split_by_page_breaks(text: &str) -> Vec<String> {
    text.split("\n---\n")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn split_by_sections(text: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current_section = String::new();

    for line in text.lines() {
        if line.starts_with('#') {
            if !current_section.is_empty() {
                sections.push(current_section.trim_end().to_string());
            }
            current_section = line.to_string();
        } else {
            if !current_section.is_empty() {
                current_section.push('\n');
            }
            current_section.push_str(line);
        }
    }

    if !current_section.is_empty() {
        sections.push(current_section.trim_end().to_string());
    }

    sections
}

fn split_by_paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn split_by_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn split_by_words(text: &str) -> Vec<String> {
    text.split(' ')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn split_by_chars(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut count = 0;
    for c in text.chars() {
        if count >= max_chars {
            chunks.push(current.clone());
            current.clear();
            count = 0;
        }
        current.push(c);
        count += 1;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn group_sub_chunks(sub_chunks: Vec<String>, delimiter: &str, max_chars: usize) -> Vec<String> {
    let mut grouped = Vec::new();
    let mut current_group = String::new();
    let mut current_len = 0;

    for chunk in sub_chunks {
        if chunk.is_empty() {
            continue;
        }

        let chunk_len = chunk.chars().count();
        if chunk_len > max_chars {
            if !current_group.is_empty() {
                grouped.push(current_group.clone());
                current_group.clear();
                current_len = 0;
            }
            grouped.push(chunk);
            continue;
        }

        let needed_len = if current_group.is_empty() {
            chunk_len
        } else {
            current_len + delimiter.chars().count() + chunk_len
        };

        if needed_len <= max_chars {
            if !current_group.is_empty() {
                current_group.push_str(delimiter);
            }
            current_group.push_str(&chunk);
            current_len = needed_len;
        } else {
            if !current_group.is_empty() {
                grouped.push(current_group.clone());
            }
            current_group = chunk;
            current_len = chunk_len;
        }
    }

    if !current_group.is_empty() {
        grouped.push(current_group);
    }

    grouped
}

fn split_recursive(text: &str, level: usize, max_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    let (chunks, delimiter) = match level {
        0 => (split_by_page_breaks(text), "\n---\n"),
        1 => (split_by_sections(text), "\n"),
        2 => (split_by_paragraphs(text), "\n\n"),
        3 => (split_by_lines(text), "\n"),
        4 => (split_by_words(text), " "),
        _ => (split_by_chars(text, max_chars), ""),
    };

    if chunks.len() <= 1 {
        if level >= 5 {
            return chunks;
        }
        return split_recursive(text, level + 1, max_chars);
    }

    let mut processed_chunks = Vec::new();
    for chunk in chunks {
        if chunk.chars().count() > max_chars {
            processed_chunks.extend(split_recursive(&chunk, level + 1, max_chars));
        } else {
            processed_chunks.push(chunk);
        }
    }

    group_sub_chunks(processed_chunks, delimiter, max_chars)
}

fn extract_frontmatter(text: &str) -> (Option<String>, &str) {
    if !text.starts_with("---") {
        return (None, text);
    }

    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return (None, text);
    }

    let mut closing_line_idx = None;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            closing_line_idx = Some(idx);
            break;
        }
    }

    if let Some(idx) = closing_line_idx {
        let fm = lines[0..=idx].join("\n") + "\n";

        let mut byte_offset = 0;
        for i in 0..=idx {
            if let Some(line_pos) = text[byte_offset..].find(lines[i]) {
                byte_offset += line_pos + lines[i].len();
            }
        }
        let mut body = &text[byte_offset..];
        if body.starts_with('\n') {
            body = &body[1..];
        } else if body.starts_with("\r\n") {
            body = &body[2..];
        }

        (Some(fm), body)
    } else {
        (None, text)
    }
}

pub fn chunk_parsed_content(content: &str, limit: usize) -> Vec<String> {
    let normalized = content.replace("\r\n", "\n");
    let (frontmatter, remaining) = extract_frontmatter(&normalized);

    if remaining.trim().is_empty() {
        if let Some(fm) = frontmatter {
            return vec![fm];
        }
        return vec![];
    }

    let chunks = split_recursive(remaining, 0, limit);

    let mut final_chunks = Vec::new();
    for chunk in chunks {
        let mut final_chunk = String::new();
        if let Some(ref fm) = frontmatter {
            final_chunk.push_str(fm);
            if !fm.ends_with('\n') {
                final_chunk.push('\n');
            }
        }
        final_chunk.push_str(&chunk);
        final_chunks.push(final_chunk);
    }

    final_chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_harness_parsers() {
        let tmp = tempdir().unwrap();

        let claude_file = tmp.path().join("chat.jsonl");
        let claude_data = "{\"role\": \"user\", \"content\": \"hello\"}\n{\"role\": \"assistant\", \"content\": \"hi\"}";
        std::fs::write(&claude_file, claude_data).unwrap();
        let parsed_claude = parse_claude_log(&claude_file).unwrap();
        assert!(parsed_claude.contains("**user**: hello"));
        assert!(parsed_claude.contains("**assistant**: hi"));

        let generic_file = tmp.path().join("generic.jsonl");
        let generic_data = "{\"speaker\": \"developer\", \"text\": \"writing tests\"}";
        std::fs::write(&generic_file, generic_data).unwrap();
        let parsed_generic = parse_generic_jsonl(&generic_file).unwrap();
        assert!(parsed_generic.contains("**developer**: writing tests"));

        let md_file = tmp.path().join("note.md");
        let md_data = "Some markdown body without frontmatter";
        std::fs::write(&md_file, md_data).unwrap();
        let parsed_md = parse_generic_markdown(&md_file, "testing").unwrap();
        assert!(parsed_md.contains("scope: \"testing\""));
        assert!(parsed_md.contains("title: \"note\""));
        assert!(parsed_md.contains("Some markdown body"));
    }

    #[test]
    fn test_chunk_parsed_content_simple() {
        let content = "Hello world";
        let chunks = chunk_parsed_content(content, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world");
    }

    #[test]
    fn test_chunk_parsed_content_paragraph_boundary() {
        let content = "Paragraph one is here.\n\nParagraph two is there.";
        let chunks = chunk_parsed_content(content, 25);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "Paragraph one is here.");
        assert_eq!(chunks[1], "Paragraph two is there.");
    }

    #[test]
    fn test_chunk_parsed_content_line_fallback() {
        let content = "Line one here.\nLine two there.";
        let chunks = chunk_parsed_content(content, 18);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "Line one here.");
        assert_eq!(chunks[1], "Line two there.");
    }

    #[test]
    fn test_chunk_parsed_content_character_fallback() {
        let content = "VeryLongSingleLineTextExceedingLimit";
        let chunks = chunk_parsed_content(content, 10);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0], "VeryLongSi");
        assert_eq!(chunks[1], "ngleLineTe");
        assert_eq!(chunks[2], "xtExceedin");
        assert_eq!(chunks[3], "gLimit");
    }

    #[test]
    fn test_parse_batched_titles() {
        let resp = "1(:|-|:)Title A\n2(:|-|:)Title B\n3(:|-|:)Title C";
        let parsed = parse_batched_titles(resp, 3);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], "Title A");
        assert_eq!(parsed[1], "Title B");
        assert_eq!(parsed[2], "Title C");
    }

    #[test]
    fn test_parse_batched_titles_fallback() {
        let resp = "1(:|-|:)Title A";
        let parsed = parse_batched_titles(resp, 3);
        assert!(parsed.is_empty());
    }

    #[tokio::test]
    async fn test_sync_workspace_docs_boundary_and_sha256_diffing() {
        let ws_dir = tempdir().unwrap();
        let ws_path = ws_dir.path();

        let vault_dir = ws_path.join("mythrax-vault");
        std::fs::create_dir_all(&vault_dir).unwrap();

        // Create ignored subdirs inside workspace
        let target_dir = ws_path.join("target");
        let git_dir = ws_path.join(".git");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(target_dir.join("ignored.md"), "ignored build content").unwrap();
        std::fs::write(git_dir.join("git.md"), "ignored git content").unwrap();
        std::fs::write(vault_dir.join("vault_note.md"), "ignored inner vault content").unwrap();

        // Create valid workspace docs
        let specs_dir = ws_path.join("specs").join("foo");
        std::fs::create_dir_all(&specs_dir).unwrap();
        let doc1_path = specs_dir.join("bar.md");
        std::fs::write(&doc1_path, "# Specs Foo Bar\nContent for bar.").unwrap();

        let store = MarkdownStore::new(&vault_dir).unwrap();
        let backend = crate::db::SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        // Sync workspace docs
        sync_workspace_docs_to_vault(ws_path, &store, &backend).await.unwrap();

        let ref_doc = vault_dir.join("reference").join("specs").join("foo").join("bar.md");
        assert!(ref_doc.exists(), "Mirrored reference file should exist in vault/reference");

        let ignored_ref = vault_dir.join("reference").join("target").join("ignored.md");
        assert!(!ignored_ref.exists(), "Target directory files should be ignored");

        let vault_inner_ref = vault_dir.join("reference").join("mythrax-vault");
        assert!(!vault_inner_ref.exists(), "Inner vault directory should be ignored");

        // Verify WikiNode in DB
        let node = backend.find_wiki_node_by_hash_db("dummy", "workspace_ref").await.unwrap();
        assert!(node.is_none());

        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"# Specs Foo Bar\nContent for bar.");
        let expected_hash = format!("{:x}", hasher.finalize());

        let node_found = backend.find_wiki_node_by_hash_db(&expected_hash, "workspace_ref").await.unwrap();
        assert!(node_found.is_some());
        let found = node_found.unwrap();
        assert_eq!(found.name, "specs/foo/bar.md");
        assert_eq!(found.scope, "workspace_ref");

        // Re-sync without changes: SHA-256 diffing skips re-write
        sync_workspace_docs_to_vault(ws_path, &store, &backend).await.unwrap();
    }

    #[tokio::test]
    async fn test_sync_workspace_docs_deletion_pruning() {
        let ws_dir = tempdir().unwrap();
        let ws_path = ws_dir.path();
        let vault_dir = ws_path.join("mythrax-vault");
        std::fs::create_dir_all(&vault_dir).unwrap();

        let doc_path = ws_path.join("temp_doc.md");
        std::fs::write(&doc_path, "# Temporary Document\nWill be deleted.").unwrap();

        let store = MarkdownStore::new(&vault_dir).unwrap();
        let backend = crate::db::SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        sync_workspace_docs_to_vault(ws_path, &store, &backend).await.unwrap();

        let ref_doc = vault_dir.join("reference").join("temp_doc.md");
        assert!(ref_doc.exists());

        // Delete from workspace
        std::fs::remove_file(&doc_path).unwrap();

        // Re-sync to trigger deletion pruning
        sync_workspace_docs_to_vault(ws_path, &store, &backend).await.unwrap();

        assert!(!ref_doc.exists(), "Mirrored file should be pruned when deleted from workspace");
    }

    #[test]
    fn test_moc_rebuild_nested_wikilinks() {
        let vault_dir = tempdir().unwrap();
        let store = MarkdownStore::new(vault_dir.path()).unwrap();

        let ref_specs = vault_dir.path().join("reference").join("specs").join("foo");
        std::fs::create_dir_all(&ref_specs).unwrap();
        std::fs::write(ref_specs.join("bar.md"), "# Bar Spec").unwrap();
        std::fs::write(vault_dir.path().join("reference").join("architecture.md"), "# Architecture").unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let backend = crate::db::SurrealBackend::new_in_memory().await.unwrap();
            store.rebuild_reference_moc(&backend).unwrap();
        });

        let moc_content = std::fs::read_to_string(vault_dir.path().join("MOC.md")).unwrap();
        assert!(moc_content.contains("## Reference"));
        assert!(moc_content.contains("- [[reference/architecture|architecture]]"));
        assert!(moc_content.contains("- [[reference/specs/foo/bar|specs / foo / bar]]"));
    }

    #[tokio::test]
    async fn test_wiki_node_content_hash_backfill() {
        let backend = crate::db::SurrealBackend::new_in_memory().await.unwrap();
        backend.init().await.unwrap();

        let sql = "CREATE wiki_node CONTENT { name: 'test_node', content: 'hello world content', scope: 'general' };";
        backend.db.query(sql).await.unwrap();

        backend.backfill_wiki_node_content_hashes_db().await.unwrap();

        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"hello world content");
        let hash = format!("{:x}", hasher.finalize());

        let found = backend.find_wiki_node_by_hash_db(&hash, "general").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "test_node");
    }
}

pub fn parse_batched_titles(resp: &str, expected_count: usize) -> Vec<String> {
    let mut titles = Vec::new();
    for line in resp.lines() {
        let parts: Vec<&str> = line.split("(:|-|:)").collect();
        if parts.len() >= 2 {
            titles.push(parts[1..].join("(:|-|:)").trim().to_string());
        }
    }
    if titles.len() == expected_count {
        titles
    } else {
        tracing::warn!(
            "Parsed titles count ({}) does not match expected chunk size ({}). Falling back.",
            titles.len(),
            expected_count
        );
        Vec::new()
    }
}

pub struct WorkspaceDocFile {
    pub rel_path: String,
    pub content: String,
    pub hash: String,
}

pub async fn sync_workspace_docs_to_vault(
    workspace_root: &Path,
    store: &MarkdownStore,
    backend: &dyn StorageBackend,
) -> Result<()> {
    let _guard = match crate::daemon::SyncWorkspaceDocsGuard::new() {
        Some(g) => g,
        None => {
            tracing::info!("Workspace docs sync already in progress, skipping.");
            return Ok(());
        }
    };

    let ws_root = workspace_root.to_path_buf();
    let vault_root = store.vault_root.clone();

    let workspace_files = tokio::task::spawn_blocking(move || -> Result<Vec<WorkspaceDocFile>> {
        let mut results = Vec::new();
        let canonical_ws = ws_root.canonicalize().unwrap_or_else(|_| ws_root.clone());
        let canonical_vault = vault_root.canonicalize().unwrap_or_else(|_| vault_root.clone());

        // Safety Guard: Never scan user HOME directory or system root
        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(&home);
            if let Ok(canon_home) = home_path.canonicalize() {
                if canonical_ws == canon_home || canonical_ws == Path::new("/") {
                    tracing::warn!("sync_workspace_docs_to_vault: Refusing to scan entire HOME directory or system root ({:?})", canonical_ws);
                    return Ok(Vec::new());
                }
            }
        }

        fn collect_docs(
            dir: &Path,
            ws_root: &Path,
            canonical_vault: &Path,
            results: &mut Vec<WorkspaceDocFile>,
            depth: usize,
        ) -> Result<()> {
            if depth > 10 {
                return Ok(());
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
                        continue;
                    }
                    let path = entry.path();
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if path.is_dir() {
                        if file_name == "target"
                            || file_name == ".git"
                            || file_name == ".venv"
                            || file_name == ".cargo"
                            || file_name == ".trash"
                            || file_name == "node_modules"
                            || file_name == "Library"
                            || file_name == "Music"
                            || file_name == "Pictures"
                            || file_name == "Desktop"
                            || file_name == "Downloads"
                            || file_name == "Movies"
                            || file_name == "Applications"
                            || file_name == ".gemini"
                            || file_name == ".rustup"
                            || file_name == ".npm"
                        {
                            continue;
                        }
                        if let Ok(canon) = path.canonicalize() {
                            if canon == *canonical_vault || canon.starts_with(canonical_vault) {
                                continue;
                            }
                        }
                        collect_docs(&path, ws_root, canonical_vault, results, depth + 1)?;
                    } else if matches!(path.extension().and_then(|s| s.to_str()), Some("md") | Some("rs") | Some("py") | Some("ts") | Some("go")) {
                        if file_name.ends_with(".tmp") || file_name == "MOC.md" {
                            continue;
                        }
                        if let Ok(rel) = path.strip_prefix(ws_root) {
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            if let Ok(raw_content) = std::fs::read_to_string(&path) {
                                use sha2::Digest;
                                let mut hasher = sha2::Sha256::new();
                                hasher.update(raw_content.as_bytes());
                                let hash = format!("{:x}", hasher.finalize());
                                results.push(WorkspaceDocFile {
                                    rel_path: rel_str,
                                    content: raw_content,
                                    hash,
                                });
                            }
                        }
                    }
                }
            }
            Ok(())
        }

        collect_docs(&canonical_ws, &canonical_ws, &canonical_vault, &mut results, 0)?;
        Ok(results)
    })
    .await??;

    let mut active_vault_paths = std::collections::HashSet::new();

    for file in &workspace_files {
        let vault_rel_path = format!("reference/{}", file.rel_path);
        active_vault_paths.insert(vault_rel_path.clone());
        let dest_disk_path = store.vault_root.join(&vault_rel_path);

        let mut needs_write = true;
        if dest_disk_path.exists() {
            if let Ok(disk_content) = std::fs::read_to_string(&dest_disk_path) {
                use sha2::Digest;
                let mut hasher = sha2::Sha256::new();
                hasher.update(disk_content.as_bytes());
                let disk_hash = format!("{:x}", hasher.finalize());
                if disk_hash == file.hash {
                    if let Ok(Some(_)) = backend.find_wiki_node_by_hash(&file.hash, "workspace_ref").await {
                        needs_write = false;
                    }
                }
            }
        }

        let ws_scope = workspace_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace_ref");
        if needs_write {
            store.write_file(&vault_rel_path, &file.content)?;
            index_reference_doc(&file.rel_path, &vault_rel_path, &file.content, &file.hash, backend).await?;
            if file.rel_path.ends_with(".rs") || file.rel_path.ends_with(".py") || file.rel_path.ends_with(".ts") || file.rel_path.ends_with(".go") {
                let _ = crate::cognitive::pipeline::extract_from_code(backend, None, &file.content, &file.rel_path, ws_scope).await;
            }
        }
    }

    if let Some(surreal) = backend.as_any().downcast_ref::<crate::db::SurrealBackend>() {
        #[derive(serde::Deserialize, SurrealValue)]
        struct WikiNodePathRef {
            id: surrealdb::types::RecordId,
            vault_path: Option<String>,
        }

        let mut offset = 0;
        loop {
            let sql = "SELECT id, vault_path FROM wiki_node WHERE scope = 'workspace_ref' LIMIT 50 START $offset;";
            let mut response = surreal.db.query(sql).bind(("offset", offset)).await?;
            let refs: Vec<WikiNodePathRef> = response.take(0).unwrap_or_default();
            if refs.is_empty() {
                break;
            }

            let page_len = refs.len();
            let mut deleted_count = 0;

            for node in refs {
                let is_stale = match &node.vault_path {
                    Some(vp) => !active_vault_paths.contains(vp),
                    None => true,
                };

                if is_stale {
                    if let Some(ref vp) = node.vault_path {
                        let fpath = store.vault_root.join(vp);
                        if fpath.exists() {
                            let _ = std::fs::remove_file(fpath);
                        }
                    }
                    let _ = surreal
                        .db
                        .query("DELETE relates_to WHERE in = $id OR out = $id;")
                        .bind(("id", node.id.clone()))
                        .await;
                    let _ = surreal
                        .db
                        .query("DELETE followed_by WHERE in = $id OR out = $id;")
                        .bind(("id", node.id.clone()))
                        .await;
                    let _ = surreal
                        .db
                        .query("DELETE $id;")
                        .bind(("id", node.id.clone()))
                        .await;
                    deleted_count += 1;
                }
            }

            offset += page_len - deleted_count;
        }
    }

    store.rebuild_reference_moc(backend)?;
    Ok(())
}

async fn index_reference_doc(
    rel_path: &str,
    vault_rel_path: &str,
    content: &str,
    content_hash: &str,
    backend: &dyn StorageBackend,
) -> Result<()> {
    if let Some(surreal) = backend.as_any().downcast_ref::<crate::db::SurrealBackend>() {
        let sql = "SELECT * FROM wiki_node WHERE vault_path = $vault_path AND scope = 'workspace_ref';";
        let mut response = surreal
            .db
            .query(sql)
            .bind(("vault_path", vault_rel_path.to_string()))
            .await?;
        let raws: Vec<crate::db::WikiNodeRaw> = response.take(0).unwrap_or_default();
        let nodes: Vec<crate::contracts::WikiNode> = raws.into_iter().map(|r| r.into_wiki_node()).collect();
        for node in nodes {
            if let Some(ref id_str) = node.id {
                if let Ok(thing) = crate::db::parse_record_id(id_str) {
                    let _ = surreal.db.query("DELETE relates_to WHERE in = $id OR out = $id;").bind(("id", thing.clone())).await;
                    let _ = surreal.db.query("DELETE followed_by WHERE in = $id OR out = $id;").bind(("id", thing.clone())).await;
                    let _ = surreal.db.query("DELETE $id;").bind(("id", thing.clone())).await;
                }
            }
        }
    }

    let chunks = chunk_parsed_content(content, 2000);
    let total_chunks = chunks.len();
    for (idx, chunk) in chunks.into_iter().enumerate() {
        let node_name = if total_chunks > 1 {
            format!("{}#part-{}", rel_path, idx + 1)
        } else {
            rel_path.to_string()
        };

        let node = crate::contracts::WikiNode {
            id: None,
            name: node_name,
            content: chunk,
            scope: "workspace_ref".to_string(),
            vault_path: Some(vault_rel_path.to_string()),
            embedding: None,
            temporal_range_start: None,
            temporal_range_end: None,
            metacognitive_confidence: Some(100),
            node_type: Some("reference".to_string()),
            content_hash: Some(content_hash.to_string()),
            ..Default::default()
        };
        backend.save_wiki_node(&node).await?;
    }
    Ok(())
}

#[cfg(test)]
mod phase8_tests {
    use super::*;

    #[tokio::test]
    async fn test_post_ingestion_compaction_and_cleanup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = MarkdownStore::new(temp_dir.path()).unwrap();
        let db = crate::db::SurrealBackend::new_in_memory().await.unwrap();
        let _ = db.db.query(crate::db::schema::INIT_SCHEMA).await;

        // 1. WikiNode for MOC generation
        let node = crate::contracts::WikiNode {
            name: "test_node".to_string(),
            scope: "test_scope".to_string(),
            ..Default::default()
        };
        db.save_wiki_node(&node).await.unwrap();

        // 2. Archived Episode for file move and DB vault_path update
        let ep_rel_path = "episodes/test_scope/archived_ep.md";
        store.write_file(ep_rel_path, "# Archived Episode Content").unwrap();

        let ep = crate::contracts::Episode {
            id: None,
            title: "Archived Episode".to_string(),
            scope: Some("test_scope".to_string()),
            archived: Some(true),
            vault_path: Some(ep_rel_path.to_string()),
            ..Default::default()
        };
        let _: Option<crate::db::EpisodeRaw> = db
            .db
            .create(("episode", "archived_ep"))
            .content(ep)
            .await
            .unwrap();

        // Execute post-ingestion compaction and cleanup
        post_ingestion_compaction_and_cleanup(&db, &store, "test_scope")
            .await
            .unwrap();

        // Verify MOC.md generation
        let moc_path = temp_dir.path().join("wiki/test_scope/MOC.md");
        assert!(moc_path.exists());
        let content = std::fs::read_to_string(moc_path).unwrap();
        assert!(content.contains("[[wiki/test_scope/test_node|test_node]]"));

        // Verify archived file move on disk
        let archived_disk_path = temp_dir.path().join("archive/test_scope/archived_ep.md");
        assert!(archived_disk_path.exists(), "Archived file should be moved to archive/test_scope/");

        // Verify SurrealDB vault_path update
        let mut resp = db
            .db
            .query("SELECT * FROM episode WHERE id = episode:archived_ep;")
            .await
            .unwrap();
        let raws: Vec<crate::db::EpisodeRaw> = resp.take(0).unwrap();
        let updated_eps: Vec<crate::contracts::Episode> = raws.into_iter().map(|r| r.into()).collect();
        assert_eq!(
            updated_eps[0].vault_path,
            Some("archive/test_scope/archived_ep.md".to_string()),
            "Database vault_path should be updated to match moved file location"
        );
    }
}
