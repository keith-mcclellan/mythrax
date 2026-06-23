use crate::db::StorageBackend;
use crate::store::MarkdownStore;
use anyhow::Result;
use std::path::{Path, PathBuf};

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

pub async fn bulk_ingest_vault(
    vault_root: &Path,
    source_dir: &Path,
    harness_type: &str,
    scope: &str,
    db: &dyn StorageBackend,
) -> Result<(usize, Vec<String>)> {
    let mut success_count = 0;
    let mut errors = Vec::new();

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
            let mut dirs = Vec::new();
            if let Ok(entries) = std::fs::read_dir(source_dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        let path = entry.path();
                        if path.file_name().map(|n| n == "quarantine").unwrap_or(false) {
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
                            dirs.push(path);
                        }
                    }
                }
            }

            let total_dirs = dirs.len();
            for (index, path) in dirs.into_iter().enumerate() {
                let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                let title = format!("antigravity_{}", dir_name);
                if existing_titles.contains(&title) {
                    tracing::info!("processing episode {} of {} complete (skipped - already exists)", index + 1, total_dirs);
                    continue;
                }

                let logs_dir = path.join(".system_generated/logs");
                let mut log_path = logs_dir.join("transcript.jsonl");
                if !log_path.exists() {
                    log_path = logs_dir.join("transcript_full.jsonl");
                }
                if log_path.exists() {
                    match parse_antigravity_log(&log_path) {
                        Ok(content) => {
                            let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                            let title = format!("antigravity_{}", dir_name);
                            let uuid = uuid::Uuid::new_v4().to_string();
                            let relative_path = format!("episodes/antigravity_{}_{}.md", dir_name, &uuid[..8]);
                            
                            let note_content = format!(
                                "---\ntitle: \"{}\"\nscope: \"{}\"\nsource: \"antigravity\"\n---\n\n{}",
                                title, scope, content
                            );
                            if store.write_file(&relative_path, &note_content).is_ok() {
                                let ep_save = crate::contracts::EpisodeSave {
                                    title,
                                    content: note_content,
                                    entities: vec![],
                                    scope: Some(scope.to_string()),
                                    vault_path: Some(relative_path),
                                    source_episode: None,
                                };
                                if db.save_episode(&ep_save).await.is_ok() {
                                    success_count += 1;
                                }
                            }
                        }
                        Err(e) => {
                            let err_msg = quarantine_file(&log_path, source_dir, &e.to_string());
                            errors.push(err_msg);
                        }
                    }
                }

                // --- Artifact pass: ingest markdown artifacts directly as WikiNodes ---
                // These files (walkthrough.md, implementation_plan.md, task.md, etc.)
                // are already human-readable structured markdown. They go straight into
                // wiki/artifacts/<conv_id>/ without any LLM processing. The ONNX
                // embedder runs on the raw content so artifacts are semantically
                // searchable — no LLM summarization step needed.
                let conv_id = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                if let Ok(file_entries) = std::fs::read_dir(&path) {
                    for file_entry in file_entries.flatten() {
                        let fpath = file_entry.path();
                        let is_md = fpath.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.eq_ignore_ascii_case("md"))
                            .unwrap_or(false);
                        if !is_md {
                            continue;
                        }
                        let file_stem = fpath.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        match std::fs::read_to_string(&fpath) {
                            Ok(artifact_content) if !artifact_content.trim().is_empty() => {
                                let node_name = format!("{}/{}", conv_id, file_stem);
                                let wiki_rel = format!(
                                    "wiki/artifacts/{}/{}.md",
                                    conv_id, file_stem
                                );
                                // Write the artifact as-is into the wiki vault
                                let _ = store.write_file(&wiki_rel, &artifact_content);
                                // Persist as a WikiNode in the DB — embedding computed
                                // by the ONNX model, no LLM call made here
                                let node = crate::contracts::WikiNode {
                                    id: None,
                                    name: node_name,
                                    content: artifact_content,
                                    scope: scope.to_string(),
                                    vault_path: Some(wiki_rel),
                                    embedding: None,
                                };
                                if db.save_wiki_node(&node).await.is_ok() {
                                    success_count += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // Log a clean progress message at INFO level
                tracing::info!("processing episode {} of {} complete", index + 1, total_dirs);
            }
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
                            let ep_save = crate::contracts::EpisodeSave {
                                title,
                                content: note_content,
                                entities: vec![],
                                scope: Some(scope.to_string()),
                                vault_path: Some(relative_path),
                                source_episode: None,
                            };
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
                    return Ok((0, vec![]));
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
                            let ep_save = crate::contracts::EpisodeSave {
                                title,
                                content: note_content,
                                entities: vec![],
                                scope: Some(scope.to_string()),
                                vault_path: Some(relative_path),
                                source_episode: None,
                            };
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
                            let ep_save = crate::contracts::EpisodeSave {
                                title,
                                content: note_content,
                                entities: vec![],
                                scope: Some(scope.to_string()),
                                vault_path: Some(relative_path),
                                source_episode: None,
                            };
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
                            let ep_save = crate::contracts::EpisodeSave {
                                title,
                                content: note_content,
                                entities: vec![],
                                scope: Some(scope.to_string()),
                                vault_path: Some(relative_path),
                                source_episode: None,
                            };
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
                            let ep_save = crate::contracts::EpisodeSave {
                                title,
                                content: note_content,
                                entities: vec![],
                                scope: Some(scope.to_string()),
                                vault_path: Some(relative_path),
                                source_episode: None,
                            };
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
                    return Ok((0, vec![]));
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
                            let ep_save = crate::contracts::EpisodeSave {
                                title,
                                content: note_content,
                                entities: vec![],
                                scope: Some(scope.to_string()),
                                vault_path: Some(relative_path),
                                source_episode: None,
                            };
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
                            let ep_save = crate::contracts::EpisodeSave {
                                title,
                                content: note_content,
                                entities: vec![],
                                scope: Some(scope.to_string()),
                                vault_path: Some(relative_path),
                                source_episode: None,
                            };
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
                            let ep_save = crate::contracts::EpisodeSave {
                                title,
                                content: note_content,
                                entities: vec![],
                                scope: Some(scope.to_string()),
                                vault_path: Some(relative_path),
                                source_episode: None,
                            };
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

    Ok((success_count, errors))
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
}
