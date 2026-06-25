use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::cognitive::forge::{extract_pdf_text, chunk_text, parse_markdown_toc, split_into_logical_sections, TOCEntry};
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

    // Create a temporary project directory to act as the workspace root
    let proj_dir = tmp.path().join("testscope");
    fs::create_dir_all(&proj_dir)?;
    fs::write(proj_dir.join("Cargo.toml"), "")?;

    // Set env var to mock LLM calls and configure active scope
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", proj_dir.to_string_lossy().to_string());
    }

    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(&vault_root)?);

    let forge = mythrax_core::cognitive::forge::Forge::new(backend.clone(), store.clone());
    
    // Ingest a document under normalized "testscope" scope
    forge.ingest_document("Some document text to analyze.", "testscope", "test_source").await?;

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
    let rules = backend.get_wisdom("test_pattern", Some("dynamic"), 5, 0, 0.0).await?;
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

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
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

#[test]
fn test_markdown_toc_parsing() {
    let md_content = "\
# Section 1
This is text in section 1.

## Subsection 1.1
This is subsection 1.1 text.

# Section 2
Text in section 2.
";
    let entries = parse_markdown_toc(md_content);
    
    assert_eq!(entries.len(), 3);
    
    assert_eq!(entries[0].title, "Section 1");
    assert_eq!(entries[0].start_byte, md_content.find("# Section 1").unwrap());
    assert_eq!(entries[0].end_byte, md_content.find("## Subsection 1.1").unwrap());
    
    assert_eq!(entries[1].title, "Subsection 1.1");
    assert_eq!(entries[1].start_byte, md_content.find("## Subsection 1.1").unwrap());
    assert_eq!(entries[1].end_byte, md_content.find("# Section 2").unwrap());
    
    assert_eq!(entries[2].title, "Section 2");
    assert_eq!(entries[2].start_byte, md_content.find("# Section 2").unwrap());
    assert_eq!(entries[2].end_byte, md_content.len());
}

#[tokio::test]
async fn test_extract_toc_via_llm_mock() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    
    let backend = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let tmp = tempdir()?;
    let store = std::sync::Arc::new(mythrax_core::store::MarkdownStore::new(tmp.path())?);
    
    let forge = mythrax_core::cognitive::forge::Forge::new(backend, store);
    
    let content = "Some document text to analyze.";
    let toc = forge.extract_toc_via_llm(content).await?;
    
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "test_title");
    assert_eq!(toc[0].start_byte, 0);
    assert_eq!(toc[0].end_byte, content.len());
    

    Ok(())
}

#[test]
fn test_logical_section_splitting_and_grouping() {
    let content = "Small section one text. Small section two text. Large section three text that will be split.";
    let toc = vec![
        TOCEntry {
            title: "Sec 1".to_string(),
            start_byte: 0,
            end_byte: 23,
        },
        TOCEntry {
            title: "Sec 2".to_string(),
            start_byte: 24,
            end_byte: 47,
        },
        TOCEntry {
            title: "Sec 3".to_string(),
            start_byte: 48,
            end_byte: content.len(),
        },
    ];
    
    let sections = split_into_logical_sections(content, &toc);
    
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].title, "Sec 1 - Sec 3");
    assert_eq!(sections[0].content.trim(), content.trim());
    
    let many_words: Vec<String> = (0..22000).map(|i| format!("w{}", i)).collect();
    let large_text = many_words.join(" ");
    let large_content = format!("Small intro. {}", large_text);
    
    let large_toc = vec![
        TOCEntry {
            title: "Small Intro".to_string(),
            start_byte: 0,
            end_byte: 12,
        },
        TOCEntry {
            title: "Huge Body".to_string(),
            start_byte: 13,
            end_byte: large_content.len(),
        },
    ];
    
    let large_sections = split_into_logical_sections(&large_content, &large_toc);
    
    assert!(large_sections.len() >= 3);
    assert_eq!(large_sections[0].title, "Small Intro");
    assert!(large_sections[1].title.starts_with("Huge Body (Part 1)"));
    assert!(large_sections[2].title.starts_with("Huge Body (Part 2)"));
}

#[test]
fn test_second_pass_character_chunking() {
    let mut very_long_text = String::new();
    for _ in 1..=5000 {
        very_long_text.push_str("This is a line of text that is fairly repetitive to build up characters quickly.\n");
    }
    assert!(very_long_text.len() > 100_000);

    let toc = vec![
        TOCEntry {
            title: "Long Chapter".to_string(),
            start_byte: 0,
            end_byte: very_long_text.len(),
        }
    ];

    let sections = split_into_logical_sections(&very_long_text, &toc);
    assert!(sections.len() >= 3);
    assert!(sections[0].title.starts_with("Long Chapter (Part 1)"));
    assert!(sections[1].title.starts_with("Long Chapter (Part 1)"));
    assert!(sections[2].title.starts_with("Long Chapter (Part 2)"));
    assert!(sections[0].content.len() <= 100_000);
    assert!(sections[1].content.len() <= 100_000);
    assert!(sections[2].content.len() <= 100_000);
}

