use mythrax_core::hooks::adapters::adapt_payload;
use serde_json::json;

#[test]
fn test_claude_code_payload_maps_to_canonical() {
    let payload = json!({
        "session": "claude-session_#123",
        "transcript": "C:\\Users\\Keith\\Documents\\transcript.json",
        "active": true
    });

    let (session_id, stop_hook_active, transcript_path) = adapt_payload(payload, "claude").unwrap();

    assert_eq!(session_id, "claude-session_123");
    assert_eq!(stop_hook_active, true);
    assert_eq!(transcript_path, "C:/Users/Keith/Documents/transcript.json");
}

#[test]
fn test_codex_payload_maps_to_canonical() {
    let payload = json!({
        "conversation_id": "codex!_session",
        "log_path": "/var/log/codex/transcript.json",
        "enabled": false
    });

    let (session_id, stop_hook_active, transcript_path) = adapt_payload(payload, "codex").unwrap();

    assert_eq!(session_id, "codex_session");
    assert_eq!(stop_hook_active, false);
    assert_eq!(transcript_path, "/var/log/codex/transcript.json");
}

#[test]
fn test_cursor_payload_maps_to_canonical() {
    let payload = json!({
        "cursor_session_id": "cursor-123",
        "chat_history_path": "/Users/keith/.cursor/history.json",
        "hook_active": true
    });

    let (session_id, stop_hook_active, transcript_path) = adapt_payload(payload, "cursor").unwrap();

    assert_eq!(session_id, "cursor-123");
    assert_eq!(stop_hook_active, true);
    assert_eq!(transcript_path, "/Users/keith/.cursor/history.json");
}

#[test]
fn test_gemini_payload_maps_to_canonical() {
    let payload = json!({
        "session_id": "gemini-456",
        "transcript_path": "/Users/keith/.gemini/transcript.json",
        "stop_hook_active": false
    });

    let (session_id, stop_hook_active, transcript_path) = adapt_payload(payload, "gemini").unwrap();

    assert_eq!(session_id, "gemini-456");
    assert_eq!(stop_hook_active, false);
    assert_eq!(transcript_path, "/Users/keith/.gemini/transcript.json");
}
