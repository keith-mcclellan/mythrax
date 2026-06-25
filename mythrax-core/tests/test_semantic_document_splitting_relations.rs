use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::cognitive::forge::Forge;
use mythrax_core::db::{SurrealBackend, StorageBackend, parse_record_id};
use mythrax_core::store::MarkdownStore;

#[tokio::test]
async fn test_semantic_document_splitting_relations() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(MarkdownStore::new(&vault_root)?);

    let forge = Forge::new(backend.clone(), store.clone());

    // 1. Generate a sample document containing several paragraphs.
    // One of them must be very large to trigger fallback splitting (> 2,000 tokens).
    // Let's repeat "word " 2200 times.
    let large_para = (0..2200).map(|_| "word").collect::<Vec<_>>().join(" ");
    let document_content = format!(
        "Paragraph 1 is small.\n\n{}\n\nParagraph 3 is also small.",
        large_para
    );

    // 2. Ingest the document
    let source_name = "test_doc.md";
    forge.ingest_document(&document_content, "testscope", source_name).await?;

    // 3. Query the database to verify the parent WikiNode is created
    let mut parent_resp = backend.db.query("SELECT * FROM wiki_node WHERE name = $name;")
        .bind(("name", source_name))
        .await?;
    let parents: Vec<serde_json::Value> = parent_resp.take(0)?;
    assert_eq!(parents.len(), 1, "There should be exactly one parent WikiNode");
    let parent_node = &parents[0];
    let parent_id_str = parent_node["id"].as_str().expect("Parent node must have an ID").to_string();

    // 4. Query the database to verify the chunk WikiNodes are created
    let mut chunk_resp = backend.db.query("SELECT * FROM wiki_node WHERE name CONTAINS $name_pat ORDER BY name;")
        .bind(("name_pat", format!("{} - Chunk", source_name)))
        .await?;
    let chunks: Vec<serde_json::Value> = chunk_resp.take(0)?;
    assert!(chunks.len() >= 2, "There should be at least two chunks");

    // 5. Verify the relates_to edges: chunk -> parent (relation: "parent")
    for chunk in &chunks {
        let chunk_id_str = chunk["id"].as_str().expect("Chunk must have an ID").to_string();
        let mut rel_resp = backend.db.query("SELECT * FROM relates_to WHERE in = $chunk_id AND out = $parent_id AND relation = 'parent';")
            .bind(("chunk_id", parse_record_id(&chunk_id_str)?))
            .bind(("parent_id", parse_record_id(&parent_id_str)?))
            .await?;
        let rels: Vec<serde_json::Value> = rel_resp.take(0)?;
        assert_eq!(rels.len(), 1, "Each chunk must relate to parent as 'parent'");
    }

    // 6. Verify sequential bidirectional links between adjacent chunks
    // Chunk N relates to Chunk N+1 with relation "next"
    // Chunk N+1 relates to Chunk N with relation "prev"
    for i in 0..chunks.len().saturating_sub(1) {
        let chunk_n_id = chunks[i]["id"].as_str().expect("Chunk must have ID").to_string();
        let chunk_n_plus_1_id = chunks[i + 1]["id"].as_str().expect("Chunk must have ID").to_string();

        // next link
        let mut next_resp = backend.db.query("SELECT * FROM relates_to WHERE in = $from AND out = $to AND relation = 'next';")
            .bind(("from", parse_record_id(&chunk_n_id)?))
            .bind(("to", parse_record_id(&chunk_n_plus_1_id)?))
            .await?;
        let next_rels: Vec<serde_json::Value> = next_resp.take(0)?;
        assert_eq!(next_rels.len(), 1, "Chunk {} should relate to Chunk {} as 'next'", chunk_n_id, chunk_n_plus_1_id);

        // prev link
        let mut prev_resp = backend.db.query("SELECT * FROM relates_to WHERE in = $from AND out = $to AND relation = 'prev';")
            .bind(("from", parse_record_id(&chunk_n_plus_1_id)?))
            .bind(("to", parse_record_id(&chunk_n_id)?))
            .await?;
        let prev_rels: Vec<serde_json::Value> = prev_resp.take(0)?;
        assert_eq!(prev_rels.len(), 1, "Chunk {} should relate to Chunk {} as 'prev'", chunk_n_plus_1_id, chunk_n_id);
    }

    // 7. Verify files are written to the store on disk
    let wiki_dir = vault_root.join("wiki").join("forge");
    assert!(wiki_dir.exists(), "Wiki forge directory should exist");

    // Read directory and check that we have parent and chunk files
    let paths = fs::read_dir(wiki_dir)?;
    let mut parent_count = 0;
    let mut chunk_count = 0;
    for path in paths {
        let path = path?.path();
        let filename = path.file_name().unwrap().to_string_lossy();
        if filename.starts_with("parent_") {
            parent_count += 1;
        } else if filename.starts_with("chunk_") {
            chunk_count += 1;
        }
    }
    assert_eq!(parent_count, 1, "There should be exactly one parent file on disk");
    assert!(chunk_count >= 2, "There should be at least two chunk files on disk");

    Ok(())
}
