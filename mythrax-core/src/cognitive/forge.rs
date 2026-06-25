use std::fs;
use std::path::Path;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use crate::contracts::WikiNode;
use crate::db::StorageBackend;
use crate::llm::LLMClient;
use crate::store::MarkdownStore;

pub struct Forge {
    backend: std::sync::Arc<crate::db::SurrealBackend>,
    store: std::sync::Arc<MarkdownStore>,
    llm: LLMClient,
}

impl Forge {
    pub fn new(
        backend: std::sync::Arc<crate::db::SurrealBackend>,
        store: std::sync::Arc<MarkdownStore>,
    ) -> Self {
        Self {
            backend,
            store,
            llm: LLMClient::new(),
        }
    }

    /// Splits text into semantic chunks based on paragraph/line boundaries,
    /// targeting 1,000–2,000 tokens per chunk.
    pub fn semantic_chunk_text(&self, content: &str) -> Vec<String> {
        let mut segments: Vec<String> = Vec::new();
        
        // 1. Split by top-level paragraphs (\n\n)
        let paragraphs: Vec<&str> = content.split("\n\n").collect();

        for para in paragraphs {
            let trimmed_para = para.trim();
            if trimmed_para.is_empty() {
                continue;
            }

            // 2. Check if paragraph exceeds 2,000 tokens
            if count_tokens(trimmed_para) > 2000 {
                // Split paragraph by lines (\n)
                let lines: Vec<&str> = trimmed_para.split('\n').collect();
                
                for line in lines {
                    let trimmed_line = line.trim();
                    if trimmed_line.is_empty() {
                        continue;
                    }

                    // 3. Check if line exceeds 2,000 tokens
                    if count_tokens(trimmed_line) > 2000 {
                        // Fallback: Use standard chunking for large lines
                        let sub_chunks = chunk_text(trimmed_line, 1500, 150);
                        for sub_chunk in sub_chunks {
                            segments.push(sub_chunk);
                        }
                    } else {
                        // Add line directly to segments
                        segments.push(trimmed_line.to_string());
                    }
                }
            } else {
                // Add paragraph directly to segments
                segments.push(trimmed_para.to_string());
            }
        }

        // 4. Group segments into final chunks (1,000–2,000 tokens)
        let mut chunks: Vec<String> = Vec::new();
        let mut current_group: Vec<String> = Vec::new();
        let mut current_tokens: usize = 0;

        for seg in segments {
            let seg_tokens = count_tokens(&seg);

            // Case A: Adding this segment exceeds 2,000 tokens
            if current_tokens + seg_tokens > 2000 {
                if !current_group.is_empty() {
                    // Flush current group
                    let joined = current_group.join("\n\n");
                    chunks.push(joined);
                    current_group.clear();
                    current_tokens = 0;
                }

                // If the segment itself is > 2000 (should be handled by fallback, but safety check)
                if seg_tokens > 2000 {
                    chunks.push(seg);
                } else {
                    // Start new group with this segment
                    current_group.push(seg);
                    current_tokens = seg_tokens;
                }
            } 
            // Case B: Adding this segment brings us into the 1,000–2,000 target range
            else if current_tokens + seg_tokens >= 1000 && current_tokens + seg_tokens <= 2000 {
                current_group.push(seg);
                // Flush immediately as we hit the target range
                let joined = current_group.join("\n\n");
                chunks.push(joined);
                current_group.clear();
                current_tokens = 0;
            } 
            // Case C: Adding this segment is still below 1,000 tokens
            else {
                current_group.push(seg);
                current_tokens += seg_tokens;
            }
        }

        // Flush any remaining content
        if !current_group.is_empty() {
            let joined = current_group.join("\n\n");
            chunks.push(joined);
        }

        chunks
    }

    /// Ingest a document, chunk it, and extract wisdom rules and wiki nodes using LLM
    pub async fn ingest_document(&self, content: &str, scope: &str, _source_name: &str) -> Result<()> {
        let sanitized_source_name = _source_name.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
        let uuid_prefix = &uuid::Uuid::new_v4().to_string()[..8];

        // 1. Chunk the document content using semantic_chunk_text
        let chunks = self.semantic_chunk_text(content);

        // 2. Save the parent index node as a WikiNode in SurrealDB and write it to the store
        let parent_path = format!("wiki/forge/parent_{}_{}.md", sanitized_source_name, uuid_prefix);
        let parent_md = format!(
            "---\nname: \"{}\"\nscope: \"{}\"\ngenerator_name: \"ForgePipeline\"\n---\n\n# {}\n\n{}",
            _source_name, scope, _source_name, content
        );
        self.store.write_file(&parent_path, &parent_md)?;

        let parent_node = WikiNode {
            id: None,
            name: _source_name.to_string(),
            content: content.to_string(),
            scope: scope.to_string(),
            vault_path: Some(parent_path),
            embedding: None,
        };
        let parent_id_str = self.backend.save_wiki_node(&parent_node).await?;

        // 3. Save each chunk node as a WikiNode in SurrealDB and write it to the store
        let mut chunk_ids = Vec::new();
        for (idx, chunk_text) in chunks.iter().enumerate() {
            let chunk_name = format!("{} - Chunk {}", _source_name, idx + 1);
            let chunk_uuid_prefix = &uuid::Uuid::new_v4().to_string()[..8];
            let chunk_path = format!("wiki/forge/chunk_{}_{}.md", sanitized_source_name, chunk_uuid_prefix);
            let chunk_md = format!(
                "---\nname: \"{}\"\nscope: \"{}\"\ngenerator_name: \"ForgePipeline\"\n---\n\n# {}\n\n{}",
                chunk_name, scope, chunk_name, chunk_text
            );
            self.store.write_file(&chunk_path, &chunk_md)?;

            let chunk_node = WikiNode {
                id: None,
                name: chunk_name,
                content: chunk_text.to_string(),
                scope: scope.to_string(),
                vault_path: Some(chunk_path),
                embedding: None,
            };
            let chunk_id_str = self.backend.save_wiki_node(&chunk_node).await?;
            chunk_ids.push(chunk_id_str);
        }

        // 4. Relate each chunk to the parent index node using a relates_to edge in SurrealDB
        for chunk_id_str in &chunk_ids {
            let chunk_thing = crate::db::parse_record_id(chunk_id_str)?;
            let parent_thing = crate::db::parse_record_id(&parent_id_str)?;
            let query = "RELATE $chunk_id -> relates_to -> $parent_id UNIQUE CONTENT { relation: 'parent', created_at: time::now() };";
            self.backend.db.query(query)
                .bind(("chunk_id", chunk_thing))
                .bind(("parent_id", parent_thing))
                .await?
                .check().context("Failed to relate chunk to parent")?;
        }

        // 5. Establish bidirectional sequential links between adjacent chunks
        for i in 0..chunk_ids.len().saturating_sub(1) {
            let chunk_n_thing = crate::db::parse_record_id(&chunk_ids[i])?;
            let chunk_n_plus_1_thing = crate::db::parse_record_id(&chunk_ids[i + 1])?;

            // Chunk N -> Chunk N+1 with relation "next"
            let query_next = "RELATE $chunk_n -> relates_to -> $chunk_n_plus_1 UNIQUE CONTENT { relation: 'next', created_at: time::now() };";
            self.backend.db.query(query_next)
                .bind(("chunk_n", chunk_n_thing.clone()))
                .bind(("chunk_n_plus_1", chunk_n_plus_1_thing.clone()))
                .await?
                .check().context("Failed to relate chunk next")?;

            // Chunk N+1 -> Chunk N with relation "prev"
            let query_prev = "RELATE $chunk_n_plus_1 -> relates_to -> $chunk_n UNIQUE CONTENT { relation: 'prev', created_at: time::now() };";
            self.backend.db.query(query_prev)
                .bind(("chunk_n_plus_1", chunk_n_plus_1_thing))
                .bind(("chunk_n", chunk_n_thing))
                .await?
                .check().context("Failed to relate chunk prev")?;
        }

        Ok(())
    }

    /// Extract Table of Contents from document content using LLM pre-pass
    pub async fn extract_toc_via_llm(&self, content: &str) -> Result<Vec<TOCEntry>> {
        let system_instruction = "You are a document structure analyzer. Identify the logical sections of the text. Respond ONLY with a JSON array of objects, with no markdown fences, explanation, or other text.";
        let prompt = format!(
            "Analyze the text below and extract a Table of Contents (TOC). For each logical section/chapter, identify:\n\
             - title: the name of the section\n\
             - start_phrase: the exact first 15-30 characters of the section to locate it uniquely in the text\n\n\
             Text:\n\
             \"\"\"\n\
             {}\n\
             \"\"\"\n\n\
             Respond ONLY with a JSON array, like:\n\
             [\n\
               {{\"title\": \"Introduction\", \"start_phrase\": \"It was a dark and\"}}\n\
             ]",
            content
        );

        let res = self.llm.completion(&*self.backend, Some(system_instruction), &prompt).await?;
        let trimmed = res.trim();
        let stripped = if trimmed.starts_with("```json") {
            trimmed.strip_prefix("```json").unwrap_or(trimmed).strip_suffix("```").unwrap_or(trimmed).trim()
        } else if trimmed.starts_with("```") {
            trimmed.strip_prefix("```").unwrap_or(trimmed).strip_suffix("```").unwrap_or(trimmed).trim()
        } else {
            trimmed
        };

        #[derive(Deserialize)]
        struct RawTOCEntry {
            title: String,
            start_phrase: String,
        }

        let raw_entries: Vec<RawTOCEntry> = serde_json::from_str(stripped)
            .context("Failed to parse LLM TOC output")?;

        let mut current_entries = Vec::new();
        for entry in raw_entries {
            if let Some(pos) = content.find(&entry.start_phrase) {
                current_entries.push((entry.title, pos));
            } else {
                let lower_content = content.to_lowercase();
                let lower_phrase = entry.start_phrase.to_lowercase();
                if let Some(pos) = lower_content.find(&lower_phrase) {
                    current_entries.push((entry.title, pos));
                } else {
                    tracing::warn!("Could not locate TOC start phrase: {:?}", entry.start_phrase);
                }
            }
        }

        current_entries.sort_by_key(|&(_, pos)| pos);

        let mut entries = Vec::new();
        for i in 0..current_entries.len() {
            let (title, start_byte) = current_entries[i].clone();
            let end_byte = if i + 1 < current_entries.len() {
                current_entries[i + 1].1
            } else {
                content.len()
            };
            entries.push(TOCEntry {
                title,
                start_byte,
                end_byte,
            });
        }

        if entries.is_empty() {
            entries.push(TOCEntry {
                title: "Document Root".to_string(),
                start_byte: 0,
                end_byte: content.len(),
            });
        }

        Ok(entries)
    }
}

/// Parse a PDF from path and return its text content
pub fn extract_pdf_text(path: &Path) -> Result<String> {
    let bytes = fs::read(path).context("Failed to read PDF file bytes")?;
    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| anyhow::anyhow!("PDF extraction failed: {}", e))?;
    Ok(text)
}

/// Chunk text into token-sized chunks (or word fallbacks)
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    use tokenizers::Tokenizer;
    
    let home = std::env::var("HOME").unwrap_or_default();
    let tokenizer_path = Path::new(&home).join(".mythrax/models/tokenizer.json");
    
    if tokenizer_path.exists() {
        if let Ok(tokenizer) = Tokenizer::from_file(&tokenizer_path) {
            if let Ok(encoding) = tokenizer.encode(text, false) {
                let ids = encoding.get_ids();
                let mut chunks = Vec::new();
                let mut start = 0;
                while start < ids.len() {
                    let end = std::cmp::min(start + chunk_size, ids.len());
                    let chunk_ids = &ids[start..end];
                    if let Ok(chunk_text) = tokenizer.decode(chunk_ids, false) {
                        chunks.push(chunk_text);
                    }
                    if end == ids.len() {
                        break;
                    }
                    start += chunk_size - overlap;
                }
                return chunks;
            }
        }
    }
    
    // Fallback: Word-based chunking
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let end = std::cmp::min(start + chunk_size, words.len());
        let chunk_words = &words[start..end];
        chunks.push(chunk_words.join(" "));
        if end == words.len() {
            break;
        }
        let step = chunk_size.saturating_sub(overlap);
        if step == 0 {
            break; // Guard against infinite loop if chunk_size <= overlap
        }
        start += step;
    }
    chunks
}

/// Runs Automated Skill Skeletonization on a SKILL.md file:
/// extracts Examples and References, saves them to subfolders, and rewrites SKILL.md as a lean skeleton.
#[allow(dead_code)] // public API; wired to CLI in a future PR
pub fn skeletonize_skill_file(skill_path: &Path) -> Result<()> {
    let content = fs::read_to_string(skill_path).context("Failed to read SKILL.md file")?;
    
    let (skeleton, examples_opt, references_opt) = skeletonize_skill(&content);
    
    let skill_dir = skill_path.parent().context("Failed to get skill directory")?;
    
    if let Some(examples_content) = examples_opt {
        let examples_dir = skill_dir.join("examples");
        fs::create_dir_all(&examples_dir)?;
        let examples_path = examples_dir.join("examples.md");
        fs::write(&examples_path, examples_content)?;
    }
    
    if let Some(references_content) = references_opt {
        let references_dir = skill_dir.join("references");
        fs::create_dir_all(&references_dir)?;
        let references_path = references_dir.join("references.md");
        fs::write(&references_path, references_content)?;
    }
    
    fs::write(skill_path, skeleton)?;
    
    Ok(())
}

#[allow(dead_code)] // called only via skeletonize_skill_file
fn skeletonize_skill(content: &str) -> (String, Option<String>, Option<String>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut frontmatter = String::new();
    let mut i = 0;
    
    // Parse frontmatter
    if i < lines.len() && lines[i].trim() == "---" {
        frontmatter.push_str(lines[i]);
        frontmatter.push('\n');
        i += 1;
        while i < lines.len() && lines[i].trim() != "---" {
            frontmatter.push_str(lines[i]);
            frontmatter.push('\n');
            i += 1;
        }
        if i < lines.len() {
            frontmatter.push_str(lines[i]);
            frontmatter.push('\n');
            i += 1;
        }
    }
    
    let mut main_body = String::new();
    let mut examples_body = String::new();
    let mut references_body = String::new();
    
    enum State {
        Main,
        Examples,
        References,
    }
    
    let mut state = State::Main;
    
    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("## Examples") {
            state = State::Examples;
            main_body.push_str(line);
            main_body.push_str("\n\nDetailed examples and playbooks have been moved to [examples/examples.md](examples/examples.md).\n\n");
            i += 1;
            continue;
        } else if line.starts_with("## References") {
            state = State::References;
            main_body.push_str(line);
            main_body.push_str("\n\nDetailed reference documentation has been moved to [references/references.md](references/references.md).\n\n");
            i += 1;
            continue;
        } else if line.starts_with("## ") {
            state = State::Main;
        }
        
        match state {
            State::Main => {
                main_body.push_str(line);
                main_body.push('\n');
            }
            State::Examples => {
                examples_body.push_str(line);
                examples_body.push('\n');
            }
            State::References => {
                references_body.push_str(line);
                references_body.push('\n');
            }
        }
        i += 1;
    }
    
    let mut updated_content = frontmatter;
    updated_content.push_str(&main_body);
    
    let examples_opt = if examples_body.trim().is_empty() { None } else { Some(examples_body) };
    let references_opt = if references_body.trim().is_empty() { None } else { Some(references_body) };
    
    (updated_content, examples_opt, references_opt)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TOCEntry {
    pub title: String,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Debug, Clone)]
pub struct LogicalSection {
    pub title: String,
    pub content: String,
}

pub fn count_tokens(text: &str) -> usize {
    use tokenizers::Tokenizer;
    
    let home = std::env::var("HOME").unwrap_or_default();
    let tokenizer_path = Path::new(&home).join(".mythrax/models/tokenizer.json");
    
    if tokenizer_path.exists() {
        if let Ok(tokenizer) = Tokenizer::from_file(&tokenizer_path) {
            if let Ok(encoding) = tokenizer.encode(text, false) {
                return encoding.get_ids().len();
            }
        }
    }
    
    // Fallback: Word-based count
    text.split_whitespace().count()
}

pub fn parse_markdown_toc(content: &str) -> Vec<TOCEntry> {
    let mut entries = Vec::new();
    let mut current_title: Option<String> = None;
    let mut current_start = 0;
    
    let base_ptr = content.as_ptr() as usize;
    
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
            if hash_count > 0 && trimmed.chars().nth(hash_count) == Some(' ') {
                let title = trimmed[hash_count..].trim().to_string();
                let line_offset = line.as_ptr() as usize - base_ptr;
                
                if let Some(prev_title) = current_title.take() {
                    entries.push(TOCEntry {
                        title: prev_title,
                        start_byte: current_start,
                        end_byte: line_offset,
                    });
                }
                current_title = Some(title);
                current_start = line_offset;
            }
        }
    }
    
    if let Some(prev_title) = current_title {
        entries.push(TOCEntry {
            title: prev_title,
            start_byte: current_start,
            end_byte: content.len(),
        });
    }
    
    if entries.is_empty() {
        entries.push(TOCEntry {
            title: "Document Root".to_string(),
            start_byte: 0,
            end_byte: content.len(),
        });
    }
    
    entries
}

pub fn split_into_logical_sections(content: &str, toc: &[TOCEntry]) -> Vec<LogicalSection> {
    let mut sections = Vec::new();
    let mut current_batch = Vec::new();
    let mut current_tokens = 0;
    
    let build_grouped_section = |content: &str, batch: &[TOCEntry]| -> LogicalSection {
        if batch.is_empty() {
            return LogicalSection { title: "Empty Section".to_string(), content: String::new() };
        }
        let start = batch[0].start_byte;
        let end = batch[batch.len() - 1].end_byte;
        let title = if batch.len() == 1 {
            batch[0].title.clone()
        } else {
            format!("{} - {}", batch[0].title, batch[batch.len() - 1].title)
        };
        LogicalSection {
            title,
            content: content[start..end].to_string(),
        }
    };

    for entry in toc {
        let entry_content = &content[entry.start_byte..entry.end_byte];
        let entry_tokens = count_tokens(entry_content);
        
        if entry_tokens > 24000 {
            // Flush current batch
            if !current_batch.is_empty() {
                sections.push(build_grouped_section(content, &current_batch));
                current_batch.clear();
                current_tokens = 0;
            }
            
            // Split the large entry using chunk_text
            // 24k size, 2.4k overlap
            let chunks = chunk_text(entry_content, 24000, 2400);
            for (idx, chunk) in chunks.into_iter().enumerate() {
                sections.push(LogicalSection {
                    title: format!("{} (Part {})", entry.title, idx + 1),
                    content: chunk,
                });
            }
        } else if current_tokens + entry_tokens > 24000 {
            // Flush current batch
            if !current_batch.is_empty() {
                sections.push(build_grouped_section(content, &current_batch));
                current_batch.clear();
            }
            current_batch.push(entry.clone());
            current_tokens = entry_tokens;
        } else {
            current_batch.push(entry.clone());
            current_tokens += entry_tokens;
        }
    }
    
    // Flush remaining
    if !current_batch.is_empty() {
        sections.push(build_grouped_section(content, &current_batch));
    }
    
    // Second pass: Ensure no section exceeds the character limit (100,000 characters)
    let mut final_sections = Vec::new();
    for section in sections {
        if section.content.len() > 100_000 {
            let chunks = crate::vault::ingestion::chunk_parsed_content(&section.content, 100_000);
            for (idx, chunk) in chunks.into_iter().enumerate() {
                final_sections.push(LogicalSection {
                    title: format!("{} (Part {})", section.title, idx + 1),
                    content: chunk,
                });
            }
        } else {
            final_sections.push(section);
        }
    }
    
    // If no sections produced (guardrail)
    if final_sections.is_empty() {
        final_sections.push(LogicalSection {
            title: "Document Root".to_string(),
            content: content.to_string(),
        });
    }
    
    final_sections
}
