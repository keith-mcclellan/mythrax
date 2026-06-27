use std::fs::File;
use std::io::{BufRead, BufReader};
use serde::Deserialize;

pub const SAVE_INTERVAL: usize = 15;

#[derive(Deserialize)]
struct SimpleMessage {
    role: Option<String>,
    content: Option<serde_json::Value>,
    message: Option<NestedMessage>,
}

#[derive(Deserialize)]
struct NestedMessage {
    role: Option<String>,
    content: Option<serde_json::Value>,
}

fn extract_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => {
            let mut parts: Vec<String> = Vec::new();
            for block in blocks {
                if let serde_json::Value::String(s) = block {
                    parts.push(s.clone());
                    continue;
                }
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    parts.push(t.to_string());
                }
                if let Some(inner) = block.get("content") {
                    let inner_text = extract_text(inner);
                    if !inner_text.is_empty() {
                        parts.push(inner_text);
                    }
                }
                for key in ["output", "input"] {
                    if let Some(s) = block.get(key).and_then(|v| v.as_str()) {
                        parts.push(s.to_string());
                    }
                }
            }
            parts.join("\n")
        }
        serde_json::Value::Object(_) => {
            extract_text(&serde_json::Value::Array(vec![value.clone()]))
        }
        _ => String::new(),
    }
}

pub fn sanitize_session_id(id: &str) -> String {
    let s: String = id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if s.is_empty() {
        "unknown".to_string()
    } else {
        s
    }
}

pub fn normalize_transcript_path(path: &str) -> String {
    path.replace('\\', "/")
        .chars()
        .filter(|&c| c != '\x00' && c != '\r' && c != '\n')
        .collect()
}

pub fn count_human_messages(path: &str) -> usize {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    let reader = BufReader::new(file);
    let mut count = 0;

    for line in reader.lines() {
        if let Ok(line_str) = line {
            if let Ok(msg) = serde_json::from_str::<SimpleMessage>(&line_str) {
                let role = msg.role.clone().or_else(|| msg.message.as_ref().and_then(|m| m.role.clone()));
                let content = msg.content.clone().or_else(|| msg.message.as_ref().and_then(|m| m.content.clone()));
                
                if let (Some(r), Some(c)) = (role, content) {
                    let text = extract_text(&c);
                    if r == "user" && !text.contains("<command-message>") {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}
