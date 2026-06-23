use std::fs;
use std::path::Path;
use anyhow::{Result, Context};
use serde::Deserialize;
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
        let chunks = chunk_text(content, 2000, 200);
        
        for (i, chunk) in chunks.iter().enumerate() {
            tracing::info!("Forging chunk {}/{}", i + 1, chunks.len());
            
            // Extract Wisdom Rules
            let wisdom_sys = "You are a systems synthesizer. Analyze the text chunk and extract system-level Wisdom Rules to prevent mistakes. Respond ONLY with a JSON array of rules.";
            let wisdom_prompt = format!(
                "Text chunk:\n\n{}\n\nRespond ONLY with a JSON array of rules, each containing exactly:\n- target_pattern (string)\n- action_to_avoid (string)\n- causal_explanation (string)\n- prescribed_remedy (string)",
                chunk
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
                                let _ = self.backend.save_wisdom_rule(&rule_contract).await;
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
                chunk
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
