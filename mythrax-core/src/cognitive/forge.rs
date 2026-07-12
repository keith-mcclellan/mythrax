use std::fs;
use std::path::Path;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use crate::contracts::{WikiNode, WisdomRule, ForgedConcept, ForgedRule};
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
        crate::vault::ingestion::chunk_parsed_content(content, 20_000)
    }

    /// Ingest a document, chunk it, extract wisdom rules and wiki concepts using LLM,
    /// and save/relate all of them with a single parallel batch embedding pass.
    pub async fn ingest_document(&self, content: &str, scope: &str, _source_name: &str) -> Result<()> {
        let normalized_scope = {
            let s = scope.trim().to_lowercase();
            let cleaned: String = s.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect();
            if cleaned.is_empty() {
                "general".to_string()
            } else {
                cleaned
            }
        };

        let sanitized_source_name = _source_name.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
        let uuid_prefix = &uuid::Uuid::new_v4().to_string()[..8];

        // 1. Chunk the document content using semantic_chunk_text
        let chunks = self.semantic_chunk_text(content);

        // 2. Perform AI-driven extraction of concepts and rules for each chunk
        let mut chunks_data = Vec::new();
        for (idx, chunk_text) in chunks.iter().enumerate() {
            let concepts = match self.extract_concepts(chunk_text).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Concept extraction failed for chunk {}: {:?}", idx + 1, e);
                    vec![]
                }
            };
            let rules = match self.extract_rules(chunk_text).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Rule extraction failed for chunk {}: {:?}", idx + 1, e);
                    vec![]
                }
            };
            chunks_data.push((chunk_text, concepts, rules));
        }

        // 3. Collect all texts that need to be embedded across parent, chunks, concepts, and rules
        let mut texts_to_embed = Vec::new();
        
        // Parent text
        texts_to_embed.push(format!("{}: {}", _source_name, content));
        
        // Chunks, concepts, and rules texts
        for (idx, (chunk_text, concepts, rules)) in chunks_data.iter().enumerate() {
            let chunk_name = format!("{} - Chunk {}", _source_name, idx + 1);
            texts_to_embed.push(format!("{}: {}", chunk_name, chunk_text));
            
            for concept in concepts {
                texts_to_embed.push(format!("{}: {}", concept.name, concept.content));
            }
            
            for rule in rules {
                texts_to_embed.push(format!(
                    "Pattern: {}\nAvoid: {}\nWhy: {}\nRemedy: {}",
                    rule.target_pattern, rule.action_to_avoid, rule.causal_explanation, rule.prescribed_remedy
                ));
            }
        }

        // 4. Run parallel batch embedding to eliminate ONNX runtime lock contention
        let embeddings = self.backend.embed_batch(&texts_to_embed).await?;

        let total_chunks = chunks.len();
        let chunk_uuids: Vec<String> = (0..total_chunks).map(|_| uuid::Uuid::new_v4().to_string()[..8].to_string()).collect();

        // 5. Save the parent index node as a WikiNode in SurrealDB and write it to the store
        let parent_path = format!("wiki/{}/parent_{}_{}.md", normalized_scope, sanitized_source_name, uuid_prefix);
        
        let mut chunks_index = String::new();
        chunks_index.push_str("\n\n## Chunks\n");
        for idx in 0..total_chunks {
            let chunk_path = format!("wiki/{}/chunk_{}_{}.md", normalized_scope, sanitized_source_name, chunk_uuids[idx]);
            let chunk_target = chunk_path.strip_suffix(".md").unwrap_or(&chunk_path);
            let chunk_name = format!("{} - Chunk {}", _source_name, idx + 1);
            chunks_index.push_str(&format!("- [[{}|{}]]\n", chunk_target, chunk_name));
        }

        let parent_md = format!(
            "---\nname: \"{}\"\nscope: \"{}\"\ngenerator_name: \"ForgePipeline\"\n---\n\n# {}\n\n{}{}",
            _source_name, normalized_scope, _source_name, content, chunks_index
        );
        self.store.write_file(&parent_path, &parent_md)?;

        let parent_node = WikiNode {
            id: None,
            name: _source_name.to_string(),
            content: content.to_string(),
            scope: normalized_scope.clone(),
            vault_path: Some(parent_path.clone()),
            embedding: Some(embeddings[0].clone()),
        };
        let parent_id_str = self.backend.save_wiki_node(&parent_node).await?;

        // 6. Save each chunk, its concepts, and rules, and link them bidirectionally
        let mut chunk_ids = Vec::new();
        let mut embed_idx = 1;

        for (idx, (chunk_text, concepts, rules)) in chunks_data.into_iter().enumerate() {
            let chunk_name = format!("{} - Chunk {}", _source_name, idx + 1);
            let chunk_uuid_prefix = &chunk_uuids[idx];
            let chunk_path = format!("wiki/{}/chunk_{}_{}.md", normalized_scope, sanitized_source_name, chunk_uuid_prefix);

            let mut nav_callout = String::new();
            nav_callout.push_str("\n\n> [!INFO]- Navigation\n");
            let parent_target = parent_path.strip_suffix(".md").unwrap_or(&parent_path);
            nav_callout.push_str(&format!("> Parent: [[{}|{}]]\n", parent_target, _source_name));
            
            let prev_str = if idx > 0 {
                let prev_path = format!("wiki/{}/chunk_{}_{}", normalized_scope, sanitized_source_name, chunk_uuids[idx - 1]);
                let prev_name = format!("Chunk {}", idx);
                format!("[[{}|{}]]", prev_path, prev_name)
            } else {
                "None".to_string()
            };
            
            let next_str = if idx + 1 < total_chunks {
                let next_path = format!("wiki/{}/chunk_{}_{}", normalized_scope, sanitized_source_name, chunk_uuids[idx + 1]);
                let next_name = format!("Chunk {}", idx + 2);
                format!("[[{}|{}]]", next_path, next_name)
            } else {
                "None".to_string()
            };
            
            nav_callout.push_str(&format!("> Prev: {} | Next: {}\n", prev_str, next_str));

            let chunk_md = format!(
                "---\nname: \"{}\"\nscope: \"{}\"\ngenerator_name: \"ForgePipeline\"\n---\n\n# {}\n\n{}{}",
                chunk_name, normalized_scope, chunk_name, chunk_text, nav_callout
            );
            self.store.write_file(&chunk_path, &chunk_md)?;

            let chunk_embedding = embeddings[embed_idx].clone();
            embed_idx += 1;

            let chunk_node = WikiNode {
                id: None,
                name: chunk_name,
                content: chunk_text.to_string(),
                scope: normalized_scope.clone(),
                vault_path: Some(chunk_path),
                embedding: Some(chunk_embedding),
            };
            let chunk_id_str = self.backend.save_wiki_node(&chunk_node).await?;
            chunk_ids.push(chunk_id_str.clone());

            let chunk_thing = crate::db::parse_record_id(&chunk_id_str)?;

            // Save extracted concepts and relate them to the chunk
            let mut concept_ids = Vec::new();
            for concept in concepts {
                let sanitized_concept_name = concept.name.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
                let concept_uuid_prefix = &uuid::Uuid::new_v4().to_string()[..8];
                let concept_path = format!("wiki/{}/concept_{}_{}.md", normalized_scope, sanitized_concept_name, concept_uuid_prefix);
                let concept_md = format!(
                    "---\nname: \"{}\"\nscope: \"{}\"\ngenerator_name: \"ForgePipeline\"\n---\n\n# {}\n\n{}",
                    concept.name.replace('"', "\\\""), normalized_scope, concept.name, concept.content
                );
                self.store.write_file(&concept_path, &concept_md)?;

                let concept_embedding = embeddings[embed_idx].clone();
                embed_idx += 1;

                let concept_node = WikiNode {
                    id: None,
                    name: concept.name.clone(),
                    content: concept.content.clone(),
                    scope: normalized_scope.clone(),
                    vault_path: Some(concept_path),
                    embedding: Some(concept_embedding),
                };
                let concept_id_str = self.backend.save_wiki_node(&concept_node).await?;
                concept_ids.push(concept_id_str.clone());

                let concept_thing = crate::db::parse_record_id(&concept_id_str)?;

                // Relate Concept -> Chunk (relation: "extracted_from")
                let query = "RELATE $concept_id -> relates_to -> $chunk_id UNIQUE CONTENT { relation: 'extracted_from', created_at: time::now() };";
                self.backend.db.query(query)
                    .bind(("concept_id", concept_thing))
                    .bind(("chunk_id", chunk_thing.clone()))
                    .await?
                    .check().context("Failed to relate concept to chunk")?;
            }

            // Save extracted rules and relate them to the chunk and concepts
            for rule in rules {
                let sanitized_rule_name = rule.target_pattern.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
                let rule_uuid_prefix = &uuid::Uuid::new_v4().to_string()[..8];
                let rule_path = format!("wisdom/{}/rule_{}_{}.md", normalized_scope, sanitized_rule_name, rule_uuid_prefix);
                let rule_md = format!(
                    "---\ntarget_pattern: \"{}\"\naction_to_avoid: \"{}\"\ncausal_explanation: \"{}\"\nprescribed_remedy: \"{}\"\ntier: \"forge\"\nscope: \"{}\"\ngenerator_name: \"ForgePipeline\"\n---\n\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}",
                    rule.target_pattern.replace('"', "\\\""),
                    rule.action_to_avoid.replace('"', "\\\""),
                    rule.causal_explanation.replace('"', "\\\""),
                    rule.prescribed_remedy.replace('"', "\\\""),
                    normalized_scope,
                    rule.target_pattern,
                    rule.action_to_avoid,
                    rule.causal_explanation,
                    rule.prescribed_remedy
                );
                self.store.write_file(&rule_path, &rule_md)?;

                let rule_embedding = embeddings[embed_idx].clone();
                embed_idx += 1;

                let rule_node = WisdomRule {
                    id: None,
                    target_pattern: rule.target_pattern.clone(),
                    action_to_avoid: rule.action_to_avoid.clone(),
                    causal_explanation: rule.causal_explanation.clone(),
                    prescribed_remedy: rule.prescribed_remedy.clone(),
                    tier: crate::contracts::Tier::Project,
                    scope: normalized_scope.clone(),
                    vault_path: Some(rule_path),
                    embedding: Some(rule_embedding),
                    source_episodes: vec![chunk_id_str.clone()],
                    generator_name: "ForgePipeline".to_string(),
                    similarity: None,
                    utility: Some(5.0),
                    status: Some("active".to_string()),
                    superseded_at: None,
                    superseded_by: None,
                    rule_type: None,
                };
                let rule_id_str = self.backend.save_wisdom_rule(&rule_node).await?;
                let rule_thing = crate::db::parse_record_id(&rule_id_str)?;

                // Relate Rule -> Chunk (relation: "extracted_from")
                let query_rule_chunk = "RELATE $rule_id -> relates_to -> $chunk_id UNIQUE CONTENT { relation: 'extracted_from', created_at: time::now() };";
                self.backend.db.query(query_rule_chunk)
                    .bind(("rule_id", rule_thing.clone()))
                    .bind(("chunk_id", chunk_thing.clone()))
                    .await?
                    .check().context("Failed to relate rule to chunk")?;

                // Relate Rule -> Concept in this chunk
                for concept_id_str in &concept_ids {
                    let concept_thing = crate::db::parse_record_id(concept_id_str)?;
                    let query_rule_concept = "RELATE $rule_id -> relates_to -> $concept_id UNIQUE CONTENT { created_at: time::now() };";
                    self.backend.db.query(query_rule_concept)
                        .bind(("rule_id", rule_thing.clone()))
                        .bind(("concept_id", concept_thing))
                        .await?
                        .check().context("Failed to relate rule to concept")?;
                }
            }
        }

        // 7. Relate each chunk to the parent index node using a relates_to edge in SurrealDB
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

        // 8. Establish bidirectional sequential links between adjacent chunks
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

    async fn extract_concepts(&self, chunk_text: &str) -> Result<Vec<ForgedConcept>> {
        let system_instruction = "You are a concept extraction assistant. Extract Wiki Concepts as a JSON array of objects, with no markdown fences, explanation, or other text.";
        let prompt = format!(
            "Identify and extract key concepts from the text below. For each concept, provide:\n\
             - name: the name of the concept\n\
             - content: a brief definition or explanation of the concept\n\n\
             Text:\n\
             \"\"\"\n\
             {}\n\
             \"\"\"\n\n\
             Respond ONLY with a JSON array, like:\n\
             [\n\
               {{\"name\": \"Concept Name\", \"content\": \"Concept explanation.\"}}\n\
             ]",
            chunk_text
        );
        
        let res = self.llm.routed_completion(self.backend.as_ref(), &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Extraction), Some(system_instruction), &prompt).await?;
        let stripped = crate::llm::strip_code_fences(&res);
        
        let concepts: Vec<ForgedConcept> = serde_json::from_str(&stripped)
            .context("Failed to parse concepts JSON")?;
        Ok(concepts)
    }

    async fn extract_rules(&self, chunk_text: &str) -> Result<Vec<ForgedRule>> {
        let system_instruction = "You are a wisdom extraction assistant. Extract system-level Wisdom Rules as a JSON array of objects, with no markdown fences, explanation, or other text.";
        let prompt = format!(
            "Identify and extract Wisdom Rules from the text below. For each rule, provide:\n\
             - target_pattern: the name or pattern to avoid/address\n\
             - action_to_avoid: the specific bad action or mistake\n\
             - causal_explanation: the reason why it is bad\n\
             - prescribed_remedy: the specific fix or best practice\n\n\
             Text:\n\
             \"\"\"\n\
             {}\n\
             \"\"\"\n\n\
             Respond ONLY with a JSON array, like:\n\
             [\n\
               {{\n\
                 \"target_pattern\": \"Avoid Hardcoded API Keys\",\n\
                 \"action_to_avoid\": \"hardcoding api_key = 'sk-...'\n\",\n\
                 \"causal_explanation\": \"This leaks credentials to source control.\",\n\
                 \"prescribed_remedy\": \"Use environment variables or vault references instead.\"\n\
               }}\n\
             ]",
            chunk_text
        );
        
        let res = self.llm.routed_completion(self.backend.as_ref(), &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Extraction), Some(system_instruction), &prompt).await?;
        let stripped = crate::llm::strip_code_fences(&res);
        
        let rules: Vec<ForgedRule> = serde_json::from_str(&stripped)
            .context("Failed to parse rules JSON")?;
        Ok(rules)
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

        let res = self.llm.routed_completion(&*self.backend, &crate::contracts::TaskProfile::new(crate::contracts::TaskArchetype::Extraction), Some(system_instruction), &prompt).await?;
        let stripped = crate::llm::strip_code_fences(&res);

        #[derive(Deserialize)]
        struct RawTOCEntry {
            title: String,
            start_phrase: String,
        }

        let raw_entries: Vec<RawTOCEntry> = serde_json::from_str(&stripped)
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

static CACHED_TOKENIZER: std::sync::OnceLock<Option<tokenizers::Tokenizer>> = std::sync::OnceLock::new();

fn get_cached_tokenizer() -> Option<&'static tokenizers::Tokenizer> {
    CACHED_TOKENIZER.get_or_init(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        let tokenizer_path = Path::new(&home).join(".mythrax/models/tokenizer.json");
        if tokenizer_path.exists() {
            tokenizers::Tokenizer::from_file(&tokenizer_path).ok()
        } else {
            None
        }
    }).as_ref()
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
