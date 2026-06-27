//! Host adapters for various AI developer tools and agents.
//!
//! Supported hosts:
//! - Claude Code (fully supported, payload schema validated)
//! - Gemini (fully supported, payload schema validated)
//!
//! Unsupported hosts:
//! - Codex (unsupported in v2.1.0 — real hook payload keys not yet specified)
//! - Cursor (unsupported in v2.1.0 — real hook payload keys not yet specified)

use serde::Deserialize;
use anyhow::{Result, Context};
use crate::hooks::shell::{sanitize_session_id, normalize_transcript_path};

#[derive(Deserialize, Debug)]
pub struct ClaudeCodePayload {
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

#[derive(Deserialize, Debug)]
pub struct GeminiPayload {
    pub session_id: String,
    pub transcript_path: String,
    pub stop_hook_active: Option<bool>,
}

pub fn adapt_claude_code(val: serde_json::Value) -> Result<(String, bool, String)> {
    let payload: ClaudeCodePayload = serde_json::from_value(val)
        .context("Failed to deserialize Claude Code payload")?;
    let session_id = sanitize_session_id(&payload.session_id);
    // default: active unless host explicitly disables (Epic 6 cadence)
    let stop_hook_active = payload.stop_hook_active.unwrap_or(true);
    let transcript_path = normalize_transcript_path(&payload.transcript_path);
    Ok((session_id, stop_hook_active, transcript_path))
}

pub fn adapt_codex(_val: serde_json::Value) -> Result<(String, bool, String)> {
    anyhow::bail!("Codex is unsupported in v2.1.0 — real hook payload keys not yet specified")
}

pub fn adapt_cursor(_val: serde_json::Value) -> Result<(String, bool, String)> {
    anyhow::bail!("Cursor is unsupported in v2.1.0 — real hook payload keys not yet specified")
}

pub fn adapt_gemini(val: serde_json::Value) -> Result<(String, bool, String)> {
    let payload: GeminiPayload = serde_json::from_value(val)
        .context("Failed to deserialize Gemini payload")?;
    let session_id = sanitize_session_id(&payload.session_id);
    // default: active unless host explicitly disables (Epic 6 cadence)
    let stop_hook_active = payload.stop_hook_active.unwrap_or(true);
    let transcript_path = normalize_transcript_path(&payload.transcript_path);
    Ok((session_id, stop_hook_active, transcript_path))
}

pub fn adapt_payload(val: serde_json::Value, host: &str) -> Result<(String, bool, String)> {
    match host.to_lowercase().as_str() {
        "claude" | "claude_code" | "claudecode" => adapt_claude_code(val),
        "codex" => adapt_codex(val),
        "cursor" => adapt_cursor(val),
        "gemini" | "antigravity" => adapt_gemini(val),
        _ => adapt_gemini(val),
    }
}
