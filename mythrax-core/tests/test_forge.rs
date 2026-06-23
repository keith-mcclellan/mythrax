use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::cognitive::forge::{extract_pdf_text, chunk_text};
use mythrax_core::db::{SurrealBackend, StorageBackend};

fn create_lopdf_pdf() -> Vec<u8> {
    use lopdf::{Document, Object, Dictionary, Stream};
    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let content_id = doc.new_object_id();
    
    let content = b"BT /F1 12 Tf 72 712 Td (Hello World from Mythrax PDF Extractor) Tj ET";
    let content_stream = Stream::new(Dictionary::new(), content.to_vec());
    doc.objects.insert(content_id, Object::Stream(content_stream));
    
    let mut page_dict = Dictionary::new();
    page_dict.set("Type", "Page");
    page_dict.set("Parent", pages_id);
    page_dict.set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
    page_dict.set("Contents", content_id);
    
    let mut resources = Dictionary::new();
    let mut fonts = Dictionary::new();
    let mut font_dict = Dictionary::new();
    font_dict.set("Type", "Font");
    font_dict.set("Subtype", "Type1");
    font_dict.set("BaseFont", "Helvetica");
    fonts.set("F1", font_dict);
    resources.set("Font", fonts);
    page_dict.set("Resources", resources);
    
    doc.objects.insert(page_id, Object::Dictionary(page_dict));
    
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", "Pages");
    pages_dict.set("Kids", vec![page_id.into()]);
    pages_dict.set("Count", 1);
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
    
    let mut catalog_dict = Dictionary::new();
    catalog_dict.set("Type", "Catalog");
    catalog_dict.set("Pages", pages_id);
    let catalog_id = doc.add_object(catalog_dict);
    
    doc.trailer.set("Root", catalog_id);
    
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

#[test]
fn test_pdf_extraction() -> Result<()> {
    let tmp = tempdir()?;
    let pdf_path = tmp.path().join("test.pdf");
    let pdf_bytes = create_lopdf_pdf();
    fs::write(&pdf_path, pdf_bytes)?;

    let extracted_text = extract_pdf_text(&pdf_path)?;
    assert!(extracted_text.contains("Hello World"));
    assert!(extracted_text.contains("Mythrax"));
    Ok(())
}

#[test]
fn test_text_chunking() {
    // Generate a long text and verify chunk size and overlap
    let words: Vec<String> = (0..3000).map(|i| format!("word{}", i)).collect();
    let long_text = words.join(" ");
    
    let chunks = chunk_text(&long_text, 2000, 200);
    assert!(!chunks.is_empty());
    assert!(chunks.len() >= 2);
    
    // Robustly check overlap content: there should be common words between the two chunks.
    let words_in_c0: std::collections::HashSet<&str> = chunks[0].split_whitespace().collect();
    let words_in_c1: std::collections::HashSet<&str> = chunks[1].split_whitespace().collect();
    let overlap_words: Vec<&&str> = words_in_c0.intersection(&words_in_c1).collect();
    assert!(!overlap_words.is_empty(), "There must be overlapping words between chunks");
}

#[tokio::test]
async fn test_ingest_document() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;

    // Set env var to mock LLM calls
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);

    let forge = mythrax_core::cognitive::forge::Forge::new(backend.clone(), store.clone());
    
    // Ingest a document
    forge.ingest_document("Some document text to analyze.", "test_scope", "test_source").await?;

    // Verify wisdom rules and wiki nodes are written and saved to db
    // 1. Files on disk
    let wisdom_dir = vault_root.join("wisdom/forge");
    let wiki_dir = vault_root.join("wiki/forge");

    assert!(wisdom_dir.exists());
    assert!(wiki_dir.exists());

    let wisdom_files: Vec<_> = fs::read_dir(&wisdom_dir)?.collect();
    let wiki_files_vec: Vec<_> = fs::read_dir(&wiki_dir)?.filter_map(|e| e.ok()).collect();

    assert!(!wisdom_files.is_empty(), "Should write at least one wisdom rule file");
    assert!(!wiki_files_vec.is_empty(), "Should write at least one wiki node file");

    // 2. Records in SurrealDB
    // Query dynamic wisdom rules in DB
    let rules = backend.get_wisdom("test_pattern", "dynamic", 5, 0, 0.0).await?;
    assert!(!rules.results.is_empty(), "Should save wisdom rules in SurrealDB");

    // Query wiki nodes: verify vault_path is persisted in DB
    let first_wiki_path = wiki_files_vec[0].path();
    let relative_wiki = first_wiki_path
        .strip_prefix(&vault_root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let wiki_node_id = backend.get_wiki_node_id_by_vault_path(&relative_wiki).await?;
    assert!(
        wiki_node_id.is_some(),
        "Wiki node at '{}' should be persisted in SurrealDB",
        relative_wiki
    );

    // Clear environment variable
    unsafe {
        std::env::remove_var("MYTHRAX_MOCK_LLM");
    }

    Ok(())
}

#[test]
fn test_skeletonize_skill_workflow() -> Result<()> {
    let tmp = tempdir()?;
    let skill_path = tmp.path().join("SKILL.md");
    
    let skill_content = r#"---
name: test-skill
description: "A test skill"
---

# Test Skill

Some main instructions here.

## Examples
Here is an example code block:
```rust
fn example() {}
```

## References
Here is a reference link:
[rustdoc](https://rust-lang.org)
"#;

    fs::write(&skill_path, skill_content)?;
    
    mythrax_core::cognitive::forge::skeletonize_skill_file(&skill_path)?;
    
    // Check rewritten SKILL.md
    let rewritten = fs::read_to_string(&skill_path)?;
    assert!(rewritten.contains("name: test-skill"));
    assert!(rewritten.contains("Detailed examples and playbooks have been moved to [examples/examples.md](examples/examples.md)."));
    assert!(rewritten.contains("Detailed reference documentation has been moved to [references/references.md](references/references.md)."));
    assert!(!rewritten.contains("fn example() {}"));
    assert!(!rewritten.contains("[rustdoc]"));

    // Check examples/examples.md
    let examples_path = tmp.path().join("examples/examples.md");
    assert!(examples_path.exists());
    let examples_content = fs::read_to_string(examples_path)?;
    assert!(examples_content.contains("fn example() {}"));

    // Check references/references.md
    let references_path = tmp.path().join("references/references.md");
    assert!(references_path.exists());
    let references_content = fs::read_to_string(references_path)?;
    assert!(references_content.contains("[rustdoc]"));

    Ok(())
}
