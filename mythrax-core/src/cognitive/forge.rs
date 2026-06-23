use std::fs;
use std::path::Path;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use crate::contracts::{WisdomRule, WikiNode};
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

    /// Ingest a document, chunk it, and extract wisdom rules and wiki nodes using LLM
    pub async fn ingest_document(&self, content: &str, scope: &str, _source_name: &str) -> Result<()> {
        let is_markdown = _source_name.ends_with(".md") || _source_name.ends_with(".markdown");
        let toc = if is_markdown {
            parse_markdown_toc(content)
        } else {
            match self.extract_toc_via_llm(content).await {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("LLM TOC extraction failed: {}. Falling back to default TOC.", e);
                    vec![TOCEntry {
                        title: "Document Root".to_string(),
                        start_byte: 0,
                        end_byte: content.len(),
                    }]
                }
            }
        };

        let sections = split_into_logical_sections(content, &toc);
        
        for (i, section) in sections.iter().enumerate() {
            tracing::info!("Forging section {}/{} ({})", i + 1, sections.len(), section.title);
            
            // Extract Wisdom Rules
            let wisdom_sys = "You are a systems synthesizer. Analyze the text chunk and extract system-level Wisdom Rules to prevent mistakes. Respond ONLY with a JSON array of rules.";
            let wisdom_prompt = format!(
                "Text chunk:\n\n{}\n\nRespond ONLY with a JSON array of rules, each containing exactly:\n- target_pattern (string)\n- action_to_avoid (string)\n- causal_explanation (string)\n- prescribed_remedy (string)",
                section.content
            );
            
            match self.llm.completion(&*self.backend, Some(wisdom_sys), &wisdom_prompt).await {
                Ok(wisdom_res) => {
                    let trimmed = wisdom_res.trim();
                    let stripped = if trimmed.starts_with("```json") {
                        trimmed.strip_prefix("```json").unwrap_or(trimmed).strip_suffix("```").unwrap_or(trimmed).trim()
                    } else if trimmed.starts_with("```") {
                        trimmed.strip_prefix("```").unwrap_or(trimmed).strip_suffix("```").unwrap_or(trimmed).trim()
                    } else {
                        trimmed
                    };
                    
                    #[derive(Deserialize)]
                    struct RawWisdom {
                        target_pattern: String,
                        action_to_avoid: String,
                        causal_explanation: String,
                        prescribed_remedy: String,
                    }
                    match serde_json::from_str::<Vec<RawWisdom>>(stripped) {
                        Ok(rules) => {
                            for r in rules {
                                let rule_uuid = uuid::Uuid::new_v4().to_string();
                                let relative_path = format!(
                                    "wisdom/forge/{}_{}.md",
                                    r.target_pattern.replace([' ', '/'], "_"),
                                    &rule_uuid[..8]
                                );
                                
                                let rule_md = format!(
                                    "---\ntarget_pattern: \"{}\"\naction_to_avoid: \"{}\"\ncausal_explanation: \"{}\"\nprescribed_remedy: \"{}\"\ntier: \"dynamic\"\nscope: \"{}\"\ngenerator_name: \"ForgePipeline\"\n---\n\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}",
                                    r.target_pattern, r.action_to_avoid, r.causal_explanation, r.prescribed_remedy,
                                    scope, r.target_pattern, r.action_to_avoid, r.causal_explanation, r.prescribed_remedy
                                );
                                
                                self.store.write_file(&relative_path, &rule_md)?;
                                
                                let rule_contract = WisdomRule {
                                    id: None,
                                    target_pattern: r.target_pattern,
                                    action_to_avoid: r.action_to_avoid,
                                    causal_explanation: r.causal_explanation,
                                    prescribed_remedy: r.prescribed_remedy,
                                    tier: "dynamic".to_string(),
                                    scope: scope.to_string(),
                                    vault_path: Some(relative_path),
                                    embedding: None,
                                    source_episodes: vec![],
                                    generator_name: "ForgePipeline".to_string(),
                                    similarity: None,
                                    utility: None,
                                };
                                let _ = crate::cognitive::synthesis::save_wisdom_rule_with_deduplication(&*self.backend, &self.store, &rule_contract).await;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse WisdomRules JSON: {}. Response was: {}", e, stripped);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to get WisdomRules completion from LLM: {}", e);
                }
            }

            // Extract Wiki Nodes
            let wiki_sys = "You are a systems synthesizer. Analyze the text chunk and extract key concepts or architectural definitions for a systems wiki. Respond ONLY with a JSON array of nodes.";
            let wiki_prompt = format!(
                "Text chunk:\n\n{}\n\nRespond ONLY with a JSON array of nodes, each containing exactly:\n- name (string concept title)\n- content (string explanation or definition)",
                section.content
            );
            
            match self.llm.completion(&*self.backend, Some(wiki_sys), &wiki_prompt).await {
                Ok(wiki_res) => {
                    let trimmed = wiki_res.trim();
                    let stripped = if trimmed.starts_with("```json") {
                        trimmed.strip_prefix("```json").unwrap_or(trimmed).strip_suffix("```").unwrap_or(trimmed).trim()
                    } else if trimmed.starts_with("```") {
                        trimmed.strip_prefix("```").unwrap_or(trimmed).strip_suffix("```").unwrap_or(trimmed).trim()
                    } else {
                        trimmed
                    };
                    
                    #[derive(Deserialize)]
                    struct RawWiki {
                        name: String,
                        content: String,
                    }
                    match serde_json::from_str::<Vec<RawWiki>>(stripped) {
                        Ok(nodes) => {
                            for n in nodes {
                                let node_uuid = uuid::Uuid::new_v4().to_string();
                                let relative_path = format!(
                                    "wiki/forge/{}_{}.md",
                                    n.name.replace([' ', '/'], "_"),
                                    &node_uuid[..8]
                                );
                                
                                let wiki_md = format!(
                                    "---\nname: \"{}\"\nscope: \"{}\"\ngenerator_name: \"ForgePipeline\"\n---\n\n# {}\n\n{}",
                                    n.name, scope, n.name, n.content
                                );
                                
                                self.store.write_file(&relative_path, &wiki_md)?;
                                
                                let node_contract = WikiNode {
                                    id: None,
                                    name: n.name,
                                    content: n.content,
                                    scope: scope.to_string(),
                                    vault_path: Some(relative_path),
                                    embedding: None,
                                };
                                let _ = self.backend.save_wiki_node(&node_contract).await;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse WikiNodes JSON: {}. Response was: {}", e, stripped);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to get WikiNodes completion from LLM: {}", e);
                }
            }
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
        
        if entry_tokens > 20000 {
            // Flush current batch
            if !current_batch.is_empty() {
                sections.push(build_grouped_section(content, &current_batch));
                current_batch.clear();
                current_tokens = 0;
            }
            
            // Split the large entry using chunk_text
            // 15k size, 1k overlap
            let chunks = chunk_text(entry_content, 15000, 1000);
            for (idx, chunk) in chunks.into_iter().enumerate() {
                sections.push(LogicalSection {
                    title: format!("{} (Part {})", entry.title, idx + 1),
                    content: chunk,
                });
            }
        } else if current_tokens + entry_tokens > 20000 {
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
