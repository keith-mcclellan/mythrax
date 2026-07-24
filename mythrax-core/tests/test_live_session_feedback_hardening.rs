use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_live_session_feedback_hardening() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;

    // We write:
    // 1. User turn (user_input)
    // 2. Assistant turn (agent_thought)
    // 3. User turn with correction keyword (user_feedback + is_correction)
    let turns = vec![
        r#"{"role": "user", "content": "Please implement the database helper."}"#,
        r#"{"role": "assistant", "content": "I have created the database helper class with connection logic."}"#,
        r#"{"role": "user", "content": "Actually, you forgot to specify the path to the database!"}"#,
    ];

    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();

    // Run mine_transcript
    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess-live-feedback",
        &path_str,
        backend.as_ref(),
        &store,
        &ignore,
    )
    .await?;

    assert_eq!(count, 3);

    // Let's query the episodes and verify their type
    let episodes = backend.get_all_episodes().await?;
    assert_eq!(episodes.len(), 3);

    // Find the feedback episode (Turn 3) and assistant episode (Turn 2)
    let ep_feedback = episodes
        .iter()
        .find(|e| e.content.contains("forgot to specify"))
        .unwrap();
    let ep_assistant = episodes
        .iter()
        .find(|e| e.content.contains("I have created"))
        .unwrap();

    assert_eq!(ep_feedback.node_type.as_deref(), Some("user_feedback"));
    assert_eq!(ep_assistant.node_type.as_deref(), Some("agent_thought"));

    // Check if the 'corrects' edge exists between Turn 3 (feedback) and Turn 2 (assistant)
    let mut db_res = backend
        .db
        .query("SELECT VALUE in FROM relates_to WHERE out = $assistant AND relation = 'corrects';")
        .bind((
            "assistant",
            mythrax_core::db::parse_record_id(ep_assistant.id.as_ref().unwrap())?,
        ))
        .await?;
    let corrects_sources: Vec<surrealdb::types::RecordId> = db_res.take(0)?;
    assert_eq!(
        corrects_sources.len(),
        1,
        "Should create a 'corrects' relation from feedback to agent thought"
    );

    // Run LLM critic directly to diagnose/guarantee execution
    mythrax_core::mcp_routes::write_handlers::run_llm_critic(
        backend.clone(),
        store.clone(),
        ep_feedback.content.clone(),
        Some("general".to_string()),
        Some(ep_feedback.id.clone().unwrap()),
    )
    .await
    .unwrap();

    // Check if a WisdomRule was saved via the LLM critic
    let rules = backend.get_all_wisdom_rules().await?;
    assert!(
        !rules.is_empty(),
        "Should extract and save at least one WisdomRule via LLM critic"
    );

    Ok(())
}
