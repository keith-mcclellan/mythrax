use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn sanitize_session_id_strips_unsafe() {
    assert_eq!(
        mythrax_core::hooks::shell::sanitize_session_id("a/b c.d"),
        "abcd"
    );
    assert_eq!(
        mythrax_core::hooks::shell::sanitize_session_id("session_123-abc"),
        "session_123-abc"
    );
    assert_eq!(
        mythrax_core::hooks::shell::sanitize_session_id(""),
        "unknown"
    );
}

#[test]
fn normalize_path_preserves_windows_drive() {
    let p = mythrax_core::hooks::shell::normalize_transcript_path("C:\\Users\\me\\s.jsonl");
    assert_eq!(p, "C:/Users/me/s.jsonl"); // backslashes->slashes, colon kept
}

#[test]
fn count_human_messages_skips_command_messages() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("transcript.jsonl");
    let mut file = File::create(&file_path).unwrap();

    // JSONL content: 2 human user messages (one with "<command-message>"), 1 assistant message
    let lines = vec![
        r#"{"role": "user", "content": "hello world"}"#,
        r#"{"role": "user", "content": "running <command-message> test"}"#,
        r#"{"role": "assistant", "content": "hi there"}"#,
    ];

    for line in lines {
        writeln!(file, "{}", line).unwrap();
    }

    let path_str = file_path.to_string_lossy();
    let count = mythrax_core::hooks::shell::count_human_messages(&path_str);
    assert_eq!(count, 1); // Only the first user message should be counted as human, the second is skipped
}
