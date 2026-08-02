//! Host adapters for various AI developer tools and agents.
//!
//! Supported hosts:
//! - Claude Code (fully supported, payload schema validated)
//! - Gemini (fully supported, payload schema validated)
//!
//! Unsupported hosts:
//! - Codex (unsupported in v2.1.0 — real hook payload keys not yet specified)
//! - Cursor (unsupported in v2.1.0 — real hook payload keys not yet specified)

use crate::hooks::shell::{normalize_transcript_path, sanitize_session_id};
use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct StandardHookPayload {
    pub session_id: String,
    pub transcript_path: String,
    pub stop_hook_active: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct CodexPayload {
    pub conversation_id: String,
    pub log_path: String,
    pub enabled: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct CursorPayload {
    pub cursor_session_id: String,
    pub chat_history_path: String,
    pub hook_active: Option<bool>,
}

pub fn adapt_standard_payload(val: serde_json::Value, host: &str) -> Result<(String, bool, String)> {
    let payload: StandardHookPayload =
        serde_json::from_value(val).with_context(|| format!("Failed to deserialize {} payload", host))?;
    let session_id = sanitize_session_id(&payload.session_id);
    let stop_hook_active = payload.stop_hook_active.unwrap_or(true);
    let transcript_path = normalize_transcript_path(&payload.transcript_path);
    Ok((session_id, stop_hook_active, transcript_path))
}

pub fn adapt_claude_code(val: serde_json::Value) -> Result<(String, bool, String)> {
    adapt_standard_payload(val, "Claude Code")
}

pub fn adapt_gemini(val: serde_json::Value) -> Result<(String, bool, String)> {
    adapt_standard_payload(val, "Gemini")
}

pub fn adapt_codex(_val: serde_json::Value) -> Result<(String, bool, String)> {
    anyhow::bail!("Codex integration requires the standard session_id and transcript_path schema format. Please ensure your hook adapter passes canonical payload parameters.")
}

pub fn adapt_cursor(_val: serde_json::Value) -> Result<(String, bool, String)> {
    anyhow::bail!("Cursor integration requires the standard session_id and transcript_path schema format. Please ensure your hook adapter passes canonical payload parameters.")
}

pub fn adapt_payload(val: serde_json::Value, host: &str) -> Result<(String, bool, String)> {
    match host.to_lowercase().as_str() {
        "claude" | "claude_code" | "claudecode" => adapt_claude_code(val),
        "codex" => adapt_codex(val),
        "cursor" => adapt_cursor(val),
        "gemini" | "antigravity" => adapt_gemini(val),
        _ => adapt_standard_payload(val, host),
    }
}

pub fn detect_user_directives(turn_text: &str) -> bool {
    let lower = turn_text.to_lowercase();
    lower.contains("always")
        || lower.contains("never")
        || lower.contains("must")
        || lower.contains("rule")
        || lower.contains("don't forget")
        || lower.contains("remember to")
}
