use mythrax_core::hooks::shell::count_human_messages;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_count_human_messages_various_formats() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("transcript.jsonl");
    let mut file = File::create(&file_path).unwrap();

    // Write a set of messages in JSON lines format:
    // 1. Simple user string content (valid user turn)
    writeln!(file, r#"{{"role": "user", "content": "hello there"}}"#).unwrap();

    // 2. User with nested structure (valid user turn)
    writeln!(
        file,
        r#"{{"message": {{"role": "user", "content": "how are you"}}}}"#
    )
    .unwrap();

    // 3. User with command-message turn (should be ignored)
    writeln!(
        file,
        r#"{{"role": "user", "content": "run some command <command-message>"}}"#
    )
    .unwrap();

    // 4. User with array-form content blocks (valid user turn)
    writeln!(
        file,
        r#"{{"role": "user", "content": [{{"type": "text", "text": "hey"}}, {{"type": "tool_result", "content": "success"}}]}}"#
    ).unwrap();

    // 5. Assistant turn (should be ignored)
    writeln!(
        file,
        r#"{{"role": "assistant", "content": "I am an assistant"}}"#
    )
    .unwrap();

    let count = count_human_messages(file_path.to_str().unwrap());
    assert_eq!(count, 3); // Message 1, 2, and 4 are valid user turns.
}
