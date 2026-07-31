use crate::llm::LLMClient;
use crate::store::MarkdownStore;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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
            llm: LLMClient::default(),
        }
    }

    /// Splits text into semantic chunks based on paragraph/line boundaries,
    /// targeting 1,000–2,000 tokens per chunk.
    pub fn semantic_chunk_text(&self, content: &str) -> Vec<String> {
        crate::vault::ingestion::chunk_parsed_content(content, 20_000)
    }

    /// Ingest a document, chunk it, extract wisdom rules and wiki concepts using LLM,
    /// and save/relate all of them with a single parallel batch embedding pass.
    pub async fn ingest_document(
        &self,
        content: &str,
        scope: &str,
        _source_name: &str,
    ) -> Result<()> {
        let _facts = crate::cognitive::pipeline::forge_document(
            &*self.backend,
            &self.store,
            _source_name,
            scope,
            Some(&self.llm),
            Some(content),
        )
        .await?;
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

        let res = self
            .llm
            .routed_completion(
                &*self.backend,
                &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Extraction),
                Some(system_instruction),
                &prompt,
            )
            .await?;
        let stripped = crate::llm::strip_code_fences(&res);

        #[derive(Deserialize)]
        struct RawTOCEntry {
            title: String,
            start_phrase: String,
        }

        let raw_entries: Vec<RawTOCEntry> =
            serde_json::from_str(&stripped).context("Failed to parse LLM TOC output")?;

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
                    tracing::warn!(
                        "Could not locate TOC start phrase: {:?}",
                        entry.start_phrase
                    );
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

static CACHED_TOKENIZER: std::sync::OnceLock<Option<tokenizers::Tokenizer>> =
    std::sync::OnceLock::new();

fn get_cached_tokenizer() -> Option<&'static tokenizers::Tokenizer> {
    CACHED_TOKENIZER
        .get_or_init(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            let tokenizer_path = Path::new(&home).join(".mythrax/models/tokenizer.json");
            if tokenizer_path.exists() {
                tokenizers::Tokenizer::from_file(&tokenizer_path).ok()
            } else {
                None
            }
        })
        .as_ref()
}

/// Chunk text into token-sized chunks (or word fallbacks)
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if let Some(tokenizer) = get_cached_tokenizer() {
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
    if let Some(tokenizer) = get_cached_tokenizer() {
        if let Ok(encoding) = tokenizer.encode(text, false) {
            return encoding.get_ids().len();
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
            return LogicalSection {
                title: "Empty Section".to_string(),
                content: String::new(),
            };
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

    // Second pass: Ensure no section exceeds the character limit (20,000 characters)
    let mut final_sections = Vec::new();
    for section in sections {
        if section.content.len() > 20_000 {
            let chunks = crate::vault::ingestion::chunk_parsed_content(&section.content, 20_000);
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
