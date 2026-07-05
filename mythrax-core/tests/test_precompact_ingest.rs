use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;

use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;

#[tokio::test]
async fn precompact_persists_raw_tool_output() -> anyhow::Result<()> {
    // 1. Build in-memory backend + MarkdownStore(tempdir) + WatchIgnoreList
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    // 2. Write a temp transcript.jsonl containing a user turn and a tool output
    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;

    // We write a user turn and a tool turn
    // Standard shape: we can check how Claude Code or other host transcript formats represent tool outputs,
    // but a general standard is a JSON line representing each message/turn.
    // In runner.rs, we parsed QuestionEntry containing haystack_sessions, which contains role & content.
    // Let's make sure our mine_transcript can parse a simple array of messages or JSONL entries.
    // The spec says:
    // "mine_transcript: read JSONL line-by-line; for each user turn and each tool-result turn, build an EpisodeSave... with content = raw text verbatim."
    // Let's write standard JSONL records representing a user message and a tool response.
    let turns = vec![
        r#"{"role": "user", "content": "Run the compile command."}"#,
        r#"{"role": "tool", "content": "Compilation successful: RAW_TOOL_PAYLOAD_XYZ"}"#,
    ];

    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();

    // 3. Call mine_transcript
    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess1",
        &path_str,
        backend.as_ref(),
        &store,
        &ignore,
    )
    .await?;

    // 4. Assert returned count >= 2
    assert!(count >= 2);

    // 5. Query the backend to verify the raw tool payload was indexed verbatim
    let response = backend
        .search(
            "RAW_TOOL_PAYLOAD_XYZ",
            Some("general"),
            false, // deep_insight
            5,     // limit
            0,     // offset
            0.0,   // threshold
            None,  // token_budget
            false, // allow_downward
            true,  // include_episodes
            true,  // include_artifacts
        )
        .await?;

    assert!(response.total_matches > 0);

    // Check that at least one result contains the verbatim payload
    let found = response
        .results
        .iter()
        .any(|r| r.content.contains("RAW_TOOL_PAYLOAD_XYZ"));
    assert!(
        found,
        "Verbatim tool output was not found in the search results"
    );

    Ok(())
}

#[tokio::test]
async fn precompact_persists_array_form_tool_result_blocks() -> anyhow::Result<()> {
    // Real Claude/Codex transcripts represent a user turn's `content` as an ARRAY
    // of content blocks (text + tool_result), not a flat string. The old
    // deserializer typed content as Option<String>, so these lines failed to parse
    // and the verbatim tool output was silently dropped. This exercises the
    // array-of-blocks path through extract_text().
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;

    // A user turn whose content is an array of blocks, including a tool_result
    // whose own content is itself an array of text blocks (the nested shape used
    // by real transcripts). Also covers the `message`-nested wrapper form.
    let turns = vec![
        r#"{"role":"user","content":[{"type":"text","text":"Here is the build output."},{"type":"tool_result","content":[{"type":"text","text":"BLOCK_TOOL_PAYLOAD_ABC compiled ok"}]}]}"#,
        r#"{"message":{"role":"user","content":[{"type":"tool_result","content":"NESTED_TOOL_PAYLOAD_DEF"}]}}"#,
    ];
    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();
    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess-blocks",
        &path_str,
        backend.as_ref(),
        store.as_ref(),
        &ignore,
    )
    .await?;
    assert!(
        count >= 2,
        "expected both array-form turns mined, got {}",
        count
    );

    for payload in ["BLOCK_TOOL_PAYLOAD_ABC", "NESTED_TOOL_PAYLOAD_DEF"] {
        let response = backend
            .search(
                payload,
                Some("general"),
                false,
                5,
                0,
                0.0,
                None,
                false,
                true,
                true,
            )
            .await?;
        let found = response.results.iter().any(|r| r.content.contains(payload));
        assert!(
            found,
            "verbatim tool output {} was dropped from array-form content",
            payload
        );
    }

    Ok(())
}
