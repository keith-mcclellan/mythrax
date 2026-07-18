use crate::db::StorageBackend;
use crate::store::MarkdownStore;
use anyhow::Result;
use std::path::{Path, PathBuf};
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
    let mut stmt = conn.prepare("SELECT key, value FROM ItemTable WHERE key LIKE 'composer:%' OR key LIKE 'chat:%';")?;
    let mut rows = stmt.query([])?;
    let mut result = String::new();
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        result.push_str(&format!("### Key: {}\n", key));
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&value) {
            result.push_str(&format!("```json\n{}\n```\n\n", serde_json::to_string_pretty(&json_val)?));
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

fn parse_antigravity_log(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    use std::io::BufRead;
    let mut markdown = String::new();
    for line_res in reader.lines() {
        let line = line_res?;
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) {
            let step_type = obj["type"].as_str().unwrap_or("");
            if step_type == "USER_INPUT" {
                if let Some(content) = obj["content"].as_str() {
                    markdown.push_str(&format!("## User Request\n{}\n\n", content));
                }
            } else if step_type == "PLANNER_RESPONSE"
                && let Some(content) = obj["content"].as_str() {
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
            let role = obj["role"].as_str()
                .or_else(|| obj["speaker"].as_str())
                .unwrap_or("unknown");
            let content = obj["content"].as_str()
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
            "---\ntitle: \"{}\"\nscope: \"{}\"\n---\n\n{}",
            file_stem, scope, content
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
                && let Some(end_idx) = trimmed[idx+1..].find('"') {
                    current_role = trimmed[idx+1..idx+1+end_idx].to_string();
                }
        } else if (trimmed.starts_with("content =") || trimmed.starts_with("content="))
            && let Some(idx) = trimmed.find('"')
                && let Some(end_idx) = trimmed[idx+1..].find('"') {
                    current_content = trimmed[idx+1..idx+1+end_idx].to_string();
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
    let filename = file_path.file_name().unwrap_or_else(|| std::ffi::OsStr::new("unknown_file"));
    let dest_path = quarantine_dir.join(filename);
    let move_res = std::fs::rename(file_path, &dest_path);
    if move_res.is_err()
        && std::fs::copy(file_path, &dest_path).is_ok() {
            let _ = std::fs::remove_file(file_path);
        }
    format!("Failed to parse {}: {}", file_path.display(), error_msg)
}

pub fn resolve_scope_from_path(path: &Path) -> Option<String> {
    let components: Vec<&str> = path.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Common generic directory names to skip
    let skip_names = [
        "brain", "antigravity", ".gemini", "episodes", "wiki", "wisdom", 
        "general", "archive", "users", "keith", "documents", "repos", 
        "workspace", "workspaces", "projects", ".system_generated", 
        "logs", "messages", "quarantine", "tempmediastorage", "target", 
        "src", "release", "debug",
        "git", "refs", "ref", "github", "lib", "bin", "tests", "test",
        "deps", "build", "dist", "node_modules", "vendor"
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
        if skip_names.iter().any(|&s| s == *comp_str || s == lower.as_str())
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
    let folder_prefixes = ["/Documents/", "/repos/", "/workspace/", "/workspaces/", "/projects/"];
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/keith".to_string());
    
    for prefix in &folder_prefixes {
        let mut start = 0;
        while let Some(idx) = content[start..].find(prefix) {
            let absolute_start = start + idx + prefix.len();
            let suffix = &content[absolute_start..];
            let len = suffix.chars()
                .take_while(|c| *c != '/' && !c.is_whitespace() && *c != '"' && *c != '\'' && *c != ',' && *c != '\\')
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
                        "brain", "antigravity", ".gemini", "episodes", "wiki", "wisdom", 
                        "general", "archive", "users", "keith", "documents", "repos", 
                        "workspace", "workspaces", "projects", ".system_generated", 
                        "logs", "messages", "quarantine", "tempmediastorage", "target", 
                        "src", "release", "debug",
                        "git", "refs", "ref", "github", "lib", "bin", "tests", "test",
                        "deps", "build", "dist", "node_modules", "vendor"
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

pub async fn bulk_ingest_vault(
    vault_root: &Path,
    source_dir: &Path,
    harness_type: &str,
    scope: &str,
    db: &dyn StorageBackend,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<(usize, Vec<String>, bool)> {
    let mut success_count = 0;
    let mut errors = Vec::new();
    let mut has_more = false;

    let store = MarkdownStore::new(vault_root)?;

    let existing_titles: std::collections::HashSet<String> = db.get_all_episodes()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.title)
        .collect();

    let find_files = |exts: &[&str]| -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(source_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    let path = entry.path();
                    if let Some(ext) = path.extension().and_then(|s| s.to_str())
                        && exts.contains(&ext.to_lowercase().as_str())
                            && !path.components().any(|c| c.as_os_str() == "quarantine") {
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
                        let log_exists = logs_dir.join("transcript.jsonl").exists() || logs_dir.join("transcript_full.jsonl").exists();
                        
                        let has_md = if let Ok(sub_entries) = std::fs::read_dir(&path) {
                            sub_entries.flatten().any(|se| {
                                se.path().extension()
                                    .and_then(|ext| ext.to_str())
                                    .map(|ext| ext.eq_ignore_ascii_case("md"))
                                    .unwrap_or(false)
                            })
                        } else {
                            false
                        };

                        if log_exists || has_md {
                            let mtime = std::fs::metadata(&path)
                                .and_then(|m| m.modified())
                                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                            dirs_with_time.push((path, mtime));
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

            let dirs: Vec<std::path::PathBuf> = dirs_with_time[start..end].iter().map(|d| d.0.clone()).collect();

            let _ingestion_guard = IngestionGuard::new();
            let llm = crate::llm::LLMClient::new();
            let mut last_episode_id: Option<String> = None;

            for (chunk_idx, dir_chunk) in dirs.chunks(50).enumerate() {
                let mut prompt = String::new();
                for (i, path) in dir_chunk.iter().enumerate() {
                    let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                    prompt.push_str(&format!("{}(:|-|:){}\n", i + 1, dir_name));
                }
                
                let sys_prompt = "You are a title generator. For each provided directory name, generate a concise, human-readable title. Format your response strictly as index(:|-|:)title, one per line.";
                let llm_resp = llm.routed_completion(db, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Extraction), Some(sys_prompt), &prompt).await.unwrap_or_default();
                let batched_titles = parse_batched_titles(&llm_resp, dir_chunk.len());

                for (local_idx, path) in dir_chunk.iter().enumerate() {
                    let current_index = start + chunk_idx * 50 + local_idx;
                    let dir_name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    
                    let title = if local_idx < batched_titles.len() {
                        batched_titles[local_idx].clone()
                    } else {
                        tracing::warn!("Title for {} missing from batch, falling back", dir_name);
                        let fb_prompt = format!("Generate a concise title for directory name: {}", dir_name);
                        llm.routed_completion(db, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Extraction), Some(sys_prompt), &fb_prompt).await.unwrap_or_else(|_| format!("antigravity_{}", dir_name))
                    };
                    
                    let part1_title = format!("{}_part1", title);
                    if existing_titles.contains(&title) || existing_titles.contains(&part1_title) {
                        tracing::info!("processing episode {} of {} complete (skipped - already exists)", current_index + 1, total_dirs);
                        continue;
                    }

                // Dynamically resolve scope for each conversation folder
                let relative_path = path.strip_prefix(source_dir).unwrap_or(&path);
                let resolved_scope = resolve_scope_from_path(relative_path)
                    .unwrap_or_else(|| {
                        let logs_dir = path.join(".system_generated/logs");
                        let mut log_path = logs_dir.join("transcript.jsonl");
                        if !log_path.exists() {
                            log_path = logs_dir.join("transcript_full.jsonl");
                        }
                        if log_path.exists() {
                            extract_scope_from_log(&log_path).unwrap_or_else(|| scope.to_string())
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
                            let is_md = fpath.extension()
                                .and_then(|e| e.to_str())
                                .map(|e| e.eq_ignore_ascii_case("md"))
                                .unwrap_or(false);
                            if is_md {
                                let file_stem = fpath.file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string();
                                if let Ok(artifact_content) = std::fs::read_to_string(&fpath) {
                                    if !artifact_content.trim().is_empty() {
                                        pre_scanned_artifacts.push((file_stem, artifact_content));
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
                            format!("wiki/{}/raw/{}_part{}.md", resolved_scope, file_stem, art_idx + 1)
                        } else {
                            format!("wiki/{}/raw/{}.md", resolved_scope, file_stem)
                        };
                        let wikilink = if total_art_chunks > 1 {
                            format!("wiki/{}/raw/{}_part{}", resolved_scope, file_stem, art_idx + 1)
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
                    match parse_antigravity_log(&log_path) {
                        Ok(content) => content,
                        Err(e) => {
                            let err_msg = quarantine_file(&log_path, source_dir, &e.to_string());
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
                }.unwrap_or_else(|| get_folder_created_at_fallback(&path));

                let uuid_suffix = uuid::Uuid::new_v4().to_string()[..8].to_string();

                // Chunk the parsed log to keep prompt sizes bounded
                let chunks = chunk_parsed_content(&parsed_content, 20_000);
                let total_chunks = chunks.len();
                let mut generated_parts = Vec::new();

                let parent_relative_path = format!("episodes/antigravity_{}_{}.md", dir_name, uuid_suffix);
                let parent_title = format!("antigravity_{}", dir_name);
                let mut parent_saved_id = String::new();

                // If multi-part, write the parent index document first and save it
                if total_chunks > 1 {
                    let mut parent_parts_list = String::new();
                    parent_parts_list.push_str("\n\n## Parts\n");
                    for chunk_idx in 0..total_chunks {
                        let part_path = format!("episodes/antigravity_{}_part{}_{}", dir_name, chunk_idx + 1, uuid_suffix);
                        parent_parts_list.push_str(&format!("- [[{}]]\n", part_path));
                    }
                    let parent_content = format!(
                        "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"antigravity\"\n---\n\n# {}\n{}",
                        parent_title, resolved_scope, parent_title, parent_parts_list
                    );
                    
                    if store.write_file(&parent_relative_path, &parent_content).is_ok() {
                        let parent_ep_save = crate::contracts::EpisodeSave::builder(
                            parent_title.clone(),
                            parent_content,
                        )
                        .scope(Some(resolved_scope.clone()))
                        .vault_path(Some(parent_relative_path.clone()))
                        .session_id(Some(dir_name.clone()))
                        .created_at(Some(created_at_opt.clone()))
                        .build();
                        if let Ok(ep_id) = db.save_episode(&parent_ep_save).await {
                            success_count += 1;
                            parent_saved_id = ep_id;
                        }
                    }
                }

                for (chunk_idx, chunk_text) in chunks.iter().enumerate() {
                    let part_title = if total_chunks > 1 {
                        format!("antigravity_{}_part{}", dir_name, chunk_idx + 1)
                    } else {
                        format!("antigravity_{}", dir_name)
                    };
                    
                    let relative_path = if total_chunks > 1 {
                        format!("episodes/antigravity_{}_part{}_{}.md", dir_name, chunk_idx + 1, uuid_suffix)
                    } else {
                        format!("episodes/antigravity_{}_{}.md", dir_name, uuid_suffix)
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
                        let parent_target = parent_relative_path.strip_suffix(".md").unwrap_or(&parent_relative_path);
                        nav_callout.push_str(&format!("> Parent: [[{}]]\n", parent_target));
                        
                        let prev_str = if chunk_idx > 0 {
                            let prev_path = format!("episodes/antigravity_{}_part{}_{}", dir_name, chunk_idx, uuid_suffix);
                            format!("[[{}]]", prev_path)
                        } else {
                            "None".to_string()
                        };
                        
                        let next_str = if chunk_idx + 1 < total_chunks {
                            let next_path = format!("episodes/antigravity_{}_part{}_{}", dir_name, chunk_idx + 2, uuid_suffix);
                            format!("[[{}]]", next_path)
                        } else {
                            "None".to_string()
                        };
                        
                        nav_callout.push_str(&format!("> Prev: {} | Next: {}\n", prev_str, next_str));
                    }

                    let note_content = format!(
                        "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"antigravity\"\n---\n\n{}{}{}",
                        part_title, resolved_scope, chunk_text, linked_artifacts_section, nav_callout
                    );

                    if store.write_file(&relative_path, &note_content).is_ok() {
                        let ep_save = crate::contracts::EpisodeSave::builder(
                            part_title.clone(),
                            note_content,
                        )
                        .scope(Some(resolved_scope.clone()))
                        .vault_path(Some(relative_path.clone()))
                        .session_id(Some(dir_name.clone()))
                        .created_at(Some(created_at_opt.clone()))
                        .build();
                        if let Ok(episode_saved_id) = db.save_episode(&ep_save).await {
                            success_count += 1;
                            generated_parts.push((part_title, relative_path, episode_saved_id));
                        }
                    }
                }

                // Downcast and establish SurrealDB relationships
                if let Some(surreal) = db.as_any().downcast_ref::<crate::db::SurrealBackend>() {
                    if total_chunks > 1 && !parent_saved_id.is_empty() {
                        if let Ok(parent_thing) = crate::db::parse_record_id(&parent_saved_id) {
                            for (_, _, part_saved_id) in &generated_parts {
                                if let Ok(child_thing) = crate::db::parse_record_id(part_saved_id) {
                                    let query_parent = "RELATE $child_thing -> relates_to -> $parent_thing UNIQUE CONTENT { relation: 'parent', created_at: time::now() };";
                                    let _ = surreal.db.query(query_parent)
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
                            let _ = surreal.db.query(query_next)
                                .bind(("part_n", part_n.clone()))
                                .bind(("part_n_plus_1", part_n_plus_1.clone()))
                                .await;

                            let query_prev = "RELATE $part_n_plus_1 -> relates_to -> $part_n UNIQUE CONTENT { relation: 'prev', created_at: time::now() };";
                            let _ = surreal.db.query(query_prev)
                                .bind(("part_n_plus_1", part_n_plus_1))
                                .bind(("part_n", part_n))
                                .await;
                        }
                    }
                }

                if let Some(surreal) = db.as_any().downcast_ref::<crate::db::SurrealBackend>() {
                    let current_primary_id = if total_chunks > 1 && !parent_saved_id.is_empty() {
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
                                let _ = surreal.db.query(query_followed)
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
                                let link_target = rel_path.strip_suffix(".md").unwrap_or(rel_path);
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
                        ..Default::default()
                    };

                    if let Ok(wiki_node_id) = db.save_wiki_node(&node).await {
                        success_count += 1;
                        for (_, _, ep_saved_id) in &generated_parts {
                            let _ = db.relate_nodes(ep_saved_id, &wiki_node_id, None, None, None).await;
                        }
                    }
                }

                // Log a clean progress message at INFO level
                tracing::info!("processing episode {} of {} complete", current_index + 1, total_dirs);
                } // End inner loop
            } // End outer loop
        }
        "claude" => {
            let files = find_files(&["jsonl"]);
            for file in files {
                let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                let title = format!("claude_{}", file_stem);
                if existing_titles.contains(&title) {
                    continue;
                }
                match parse_claude_log(&file) {
                    Ok(content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = format!("claude_{}", file_stem);
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path = format!("episodes/claude_{}_{}.md", file_stem, &uuid[..8]);
                        
                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"claude\"\n---\n\n{}",
                            title, scope, content
                        );
                        if store.write_file(&relative_path, &note_content).is_ok() {
                            let ep_save = crate::contracts::EpisodeSave::builder(
                                title,
                                note_content,
                            )
                            .scope(Some(scope.to_string()))
                            .vault_path(Some(relative_path))
                            .build();
                            if db.save_episode(&ep_save).await.is_ok() {
                                success_count += 1;
                            }
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
                if existing_titles.contains(&title) {
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
                        if store.write_file(&relative_path, &note_content).is_ok() {
                            let ep_save = crate::contracts::EpisodeSave::builder(
                                title,
                                note_content,
                            )
                            .scope(Some(scope.to_string()))
                            .vault_path(Some(relative_path))
                            .build();
                            if db.save_episode(&ep_save).await.is_ok() {
                                success_count += 1;
                            }
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
                if existing_titles.contains(&title) {
                    continue;
                }
                match parse_codex_log(&file) {
                    Ok(content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = format!("codex_{}", file_stem);
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path = format!("episodes/codex_{}_{}.md", file_stem, &uuid[..8]);
                        
                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"codex\"\n---\n\n{}",
                            title, scope, content
                        );
                        if store.write_file(&relative_path, &note_content).is_ok() {
                            let ep_save = crate::contracts::EpisodeSave::builder(
                                title,
                                note_content,
                            )
                            .scope(Some(scope.to_string()))
                            .vault_path(Some(relative_path))
                            .build();
                            if db.save_episode(&ep_save).await.is_ok() {
                                success_count += 1;
                            }
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
                if existing_titles.contains(&title) {
                    continue;
                }
                match parse_opencode_log(&file) {
                    Ok(content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = format!("opencode_{}", file_stem);
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path = format!("episodes/opencode_{}_{}.md", file_stem, &uuid[..8]);
                        
                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"opencode\"\n---\n\n{}",
                            title, scope, content
                        );
                        if store.write_file(&relative_path, &note_content).is_ok() {
                            let ep_save = crate::contracts::EpisodeSave::builder(
                                title,
                                note_content,
                            )
                            .scope(Some(scope.to_string()))
                            .vault_path(Some(relative_path))
                            .build();
                            if db.save_episode(&ep_save).await.is_ok() {
                                success_count += 1;
                            }
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
                if existing_titles.contains(&title) {
                    continue;
                }
                match parse_openclaw_log(&file) {
                    Ok(content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = format!("openclaw_{}", file_stem);
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path = format!("episodes/openclaw_{}_{}.md", file_stem, &uuid[..8]);
                        
                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"openclaw\"\n---\n\n{}",
                            title, scope, content
                        );
                        if store.write_file(&relative_path, &note_content).is_ok() {
                            let ep_save = crate::contracts::EpisodeSave::builder(
                                title,
                                note_content,
                            )
                            .scope(Some(scope.to_string()))
                            .vault_path(Some(relative_path))
                            .build();
                            if db.save_episode(&ep_save).await.is_ok() {
                                success_count += 1;
                            }
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
                if existing_titles.contains(&title) {
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
                        if store.write_file(&relative_path, &note_content).is_ok() {
                            let ep_save = crate::contracts::EpisodeSave::builder(
                                title,
                                note_content,
                            )
                            .scope(Some(scope.to_string()))
                            .vault_path(Some(relative_path))
                            .build();
                            if db.save_episode(&ep_save).await.is_ok() {
                                success_count += 1;
                            }
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
                if existing_titles.contains(&title) {
                    continue;
                }
                match parse_generic_jsonl(&file) {
                    Ok(content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = format!("generic_{}", file_stem);
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path = format!("episodes/generic_{}_{}.md", file_stem, &uuid[..8]);
                        
                        let note_content = format!(
                            "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"generic_jsonl\"\n---\n\n{}",
                            title, scope, content
                        );
                        if store.write_file(&relative_path, &note_content).is_ok() {
                            let ep_save = crate::contracts::EpisodeSave::builder(
                                title,
                                note_content,
                            )
                            .scope(Some(scope.to_string()))
                            .vault_path(Some(relative_path))
                            .build();
                            if db.save_episode(&ep_save).await.is_ok() {
                                success_count += 1;
                            }
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
                if existing_titles.contains(&title) {
                    continue;
                }
                match parse_generic_markdown(&file, scope) {
                    Ok(note_content) => {
                        let file_stem = file.file_stem().unwrap_or_default().to_string_lossy();
                        let title = file_stem.to_string();
                        let uuid = uuid::Uuid::new_v4().to_string();
                        let relative_path = format!("episodes/{}_{}.md", file_stem, &uuid[..8]);
                        
                        if store.write_file(&relative_path, &note_content).is_ok() {
                            let ep_save = crate::contracts::EpisodeSave::builder(
                                title,
                                note_content,
                            )
                            .scope(Some(scope.to_string()))
                            .vault_path(Some(relative_path))
                            .build();
                            if db.save_episode(&ep_save).await.is_ok() {
                                success_count += 1;
                            }
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

    Ok((success_count, errors, has_more))
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
            if current_section.is_empty() {
                current_section = line.to_string();
            } else {
                current_section.push('\n');
                current_section.push_str(line);
            }
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
        tracing::warn!("Parsed titles count ({}) does not match expected chunk size ({}). Falling back.", titles.len(), expected_count);
        Vec::new()
    }
}
