use crate::contracts::{WikiNode, WisdomRule};
use crate::db::StorageBackend;
use crate::store::MarkdownStore;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub body: String,
    pub scope: String,
    pub path: PathBuf,
    pub is_meta: bool,
}

fn parse_skill_file(path: &Path) -> Result<SkillInfo> {
    let content = std::fs::read_to_string(path)?;
    if !content.starts_with("---") {
        anyhow::bail!("No frontmatter in SKILL.md");
    }
    let parts: Vec<&str> = content.split("---").collect();
    if parts.len() < 3 {
        anyhow::bail!("Invalid frontmatter in SKILL.md");
    }
    let yaml_str = parts[1];
    let body = parts[2..].join("---");

    #[derive(serde::Deserialize)]
    struct SkillFrontmatter {
        name: String,
        description: String,
        scope: Option<String>,
        generator_name: Option<String>,
    }
    let fm: SkillFrontmatter = serde_yaml::from_str(yaml_str)?;
    let is_meta = fm.generator_name.as_deref() == Some("MetaSkillSynthesizer");

    let scope = if let Some(s) = fm.scope {
        s
    } else if is_meta {
        let folder_name = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        if folder_name.starts_with("meta-") {
            folder_name.strip_prefix("meta-").unwrap().to_string()
        } else {
            "general".to_string()
        }
    } else {
        "general".to_string()
    };

    Ok(SkillInfo {
        name: fm.name,
        description: fm.description,
        body: body.trim().to_string(),
        scope,
        path: path.to_path_buf(),
        is_meta,
    })
}

fn scan_skills_in_dir(dir: &Path) -> Vec<SkillInfo> {
    let mut skills = Vec::new();
    if !dir.exists() {
        return skills;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let skill_md_path = entry.path().join("SKILL.md");
                if skill_md_path.exists() {
                    if let Ok(skill) = parse_skill_file(&skill_md_path) {
                        skills.push(skill);
                    }
                }
            }
        }
    }
    skills
}

pub fn scan_all_skills(store: &MarkdownStore) -> Vec<SkillInfo> {
    let home = std::env::var("HOME").unwrap_or_default();
    let global_skills_dir = PathBuf::from(home).join(".gemini/config/skills");

    let mut project_skills_dir = PathBuf::from(".agents/skills");
    if !project_skills_dir.exists() {
        project_skills_dir = store.vault_root.join("../.agents/skills");
    }

    let mut skills = scan_skills_in_dir(&global_skills_dir);
    skills.extend(scan_skills_in_dir(&project_skills_dir));
    skills
}

pub struct MetaSkillSynthesizer {
    llm: crate::llm::LLMClient,
}

impl Default for MetaSkillSynthesizer {
    fn default() -> Self {
        Self::new()
    }
}

impl MetaSkillSynthesizer {
    pub fn new() -> Self {
        Self {
            llm: crate::llm::LLMClient::new(),
        }
    }

    pub async fn synthesize_meta_skills(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
    ) -> Result<Vec<String>> {
        let wisdom_rules = db.get_all_wisdom_rules().await?;
        let wiki_nodes = db.get_all_wiki_nodes().await?;
        let skills = scan_all_skills(store);

        let mut active_scopes = db.get_active_scopes().await?;
        for rule in &wisdom_rules {
            if !rule.scope.trim().is_empty() && !active_scopes.contains(&rule.scope) {
                active_scopes.push(rule.scope.clone());
            }
        }
        for node in &wiki_nodes {
            if !node.scope.trim().is_empty() && !active_scopes.contains(&node.scope) {
                active_scopes.push(node.scope.clone());
            }
        }
        for sk in &skills {
            if !sk.scope.trim().is_empty() && !active_scopes.contains(&sk.scope) {
                active_scopes.push(sk.scope.clone());
            }
        }
        if !active_scopes.contains(&"general".to_string()) {
            active_scopes.push("general".to_string());
        }

        let mut scope_rules: HashMap<String, Vec<WisdomRule>> = HashMap::new();
        let mut scope_nodes: HashMap<String, Vec<WikiNode>> = HashMap::new();
        let mut scope_skills: HashMap<String, Vec<SkillInfo>> = HashMap::new();

        for rule in wisdom_rules {
            let sc = if rule.scope.trim().is_empty() {
                "general".to_string()
            } else {
                rule.scope.clone()
            };
            scope_rules.entry(sc).or_default().push(rule);
        }

        for node in wiki_nodes {
            let sc = if node.scope.trim().is_empty() {
                "general".to_string()
            } else {
                node.scope.clone()
            };
            scope_nodes.entry(sc).or_default().push(node);
        }

        for sk in skills {
            scope_skills.entry(sk.scope.clone()).or_default().push(sk);
        }

        let mut published_skills = Vec::new();

        for scope in &active_scopes {
            let rules = scope_rules.get(scope).cloned().unwrap_or_default();
            let nodes = scope_nodes.get(scope).cloned().unwrap_or_default();
            let sks = scope_skills.get(scope).cloned().unwrap_or_default();

            if rules.is_empty() && nodes.is_empty() {
                continue;
            }

            let mut prompt_context = String::new();
            prompt_context.push_str("### Wisdom Rules:\n");
            for r in &rules {
                prompt_context.push_str(&format!(
                    "- Target: {}\n  Avoid: {}\n  Why: {}\n  Remedy: {}\n\n",
                    r.target_pattern, r.action_to_avoid, r.causal_explanation, r.prescribed_remedy
                ));
            }

            prompt_context.push_str("### Wiki Nodes / Design Documents:\n");
            for n in &nodes {
                let text_limit = std::cmp::min(n.content.len(), 3000);
                prompt_context.push_str(&format!(
                    "- Title: {}\n  Content:\n{}\n\n",
                    n.name,
                    &n.content[..text_limit]
                ));
            }

            prompt_context.push_str("### Existing Playbooks / Instructions:\n");
            for sk in &sks {
                if !sk.is_meta {
                    prompt_context.push_str(&format!(
                        "- Name: {}\n  Description: {}\n  Body:\n{}\n\n",
                        sk.name, sk.description, sk.body
                    ));
                }
            }

            let sys_prompt = "You are a meta-skill synthesizer. Analyze the provided wisdom rules, playbooks, and design documents, and generate a cohesive, unified AI Agent Playbook (SKILL.md). \
                              To maintain token consciousness and prevent context bloat: \
                              1. Keep the generated playbook lightweight (under 400 lines) by summarizing core principles rather than reproducing large reference documents. \
                              2. Explicitly instruct the AI agent to query the Mythrax memory engine using vector search tools (`search_wisdom`, `search_memories`) for details (provide suggested query terms). \
                              Output ONLY the SKILL.md content starting with YAML frontmatter containing:\n\
                              ---\n\
                              name: meta-<scope-name>\n\
                              description: <active-voice-summary-of-this-scope-rules>\n\
                              generator_name: MetaSkillSynthesizer\n\
                              ---";

            let prompt_text = format!(
                "Scope: {}\n\nContext Data:\n\n{}Output ONLY the complete SKILL.md file, starting with frontmatter:",
                scope, prompt_context
            );

            tracing::info!("Synthesizing meta-skill for scope: {}", scope);
            let response = self
                .llm
                .completion(db, Some(sys_prompt), &prompt_text)
                .await?;
            let trimmed = response.trim();
            let stripped = if trimmed.starts_with("```markdown") {
                trimmed
                    .strip_prefix("```markdown")
                    .unwrap_or(trimmed)
                    .strip_suffix("```")
                    .unwrap_or(trimmed)
                    .trim()
            } else if trimmed.starts_with("```") {
                trimmed
                    .strip_prefix("```")
                    .unwrap_or(trimmed)
                    .strip_suffix("```")
                    .unwrap_or(trimmed)
                    .trim()
            } else {
                trimmed
            };

            let project_skills_dir = store.vault_root.join("../.agents/skills");
            let skill_dir = project_skills_dir.join(format!("meta-{}", scope));
            std::fs::create_dir_all(&skill_dir)?;
            let skill_file = skill_dir.join("SKILL.md");
            std::fs::write(&skill_file, stripped)?;

            published_skills.push(format!("meta-{}", scope));
        }

        Ok(published_skills)
    }

    pub async fn detect_skill_merges(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
    ) -> Result<Vec<serde_json::Value>> {
        let skills = scan_all_skills(store);
        if skills.len() < 2 {
            return Ok(vec![]);
        }

        let mut embeddings = Vec::new();
        for sk in &skills {
            let emb = db.embed(&sk.description).await?;
            embeddings.push(emb);
        }

        let mut suggestions = Vec::new();

        for i in 0..skills.len() {
            for j in (i + 1)..skills.len() {
                let sim = 1.0
                    - crate::cognitive::synthesis::cosine_distance(&embeddings[i], &embeddings[j]);
                if sim > 0.85 {
                    let sys_prompt = "You are a skill merge validator. Review two candidate playbooks and determine if they should be consolidated. Respond ONLY with a JSON object: { \"should_merge\": bool, \"suggested_name\": \"string\", \"reason\": \"string\" }.";
                    let prompt_text = format!(
                        "Playbook 1: {}\nDescription: {}\n\nPlaybook 2: {}\nDescription: {}\n\nValidate if these should merge:",
                        skills[i].name,
                        skills[i].description,
                        skills[j].name,
                        skills[j].description
                    );

                    if let Ok(res_str) = self
                        .llm
                        .completion(db, Some(sys_prompt), &prompt_text)
                        .await
                    {
                        let trimmed = res_str.trim();
                        let stripped = if trimmed.starts_with("```json") {
                            trimmed
                                .strip_prefix("```json")
                                .unwrap_or(trimmed)
                                .strip_suffix("```")
                                .unwrap_or(trimmed)
                                .trim()
                        } else if trimmed.starts_with("```") {
                            trimmed
                                .strip_prefix("```")
                                .unwrap_or(trimmed)
                                .strip_suffix("```")
                                .unwrap_or(trimmed)
                                .trim()
                        } else {
                            trimmed
                        };

                        #[derive(serde::Deserialize)]
                        struct MergeValidation {
                            should_merge: bool,
                            suggested_name: String,
                            reason: String,
                        }

                        if let Ok(val) = serde_json::from_str::<MergeValidation>(stripped) {
                            if val.should_merge {
                                suggestions.push(serde_json::json!({
                                    "source_skills": vec![skills[i].name.clone(), skills[j].name.clone()],
                                    "suggested_target_name": val.suggested_name,
                                    "similarity": sim,
                                    "reason": val.reason,
                                }));
                            }
                        }
                    }
                }
            }
        }

        let suggestions_file = "wiki/skill_merge_suggestions.md";
        let mut file_content = String::new();
        file_content.push_str("# Skill Merge Suggestions\n\n");
        if suggestions.is_empty() {
            file_content.push_str("No redundant or overlapping skills detected.\n");
        } else {
            for sug in &suggestions {
                let sources = sug["source_skills"].as_array().unwrap();
                let src_names: Vec<String> = sources
                    .iter()
                    .map(|s| s.as_str().unwrap().to_string())
                    .collect();
                file_content.push_str(&format!(
                    "## Merge Candidate: {}\n- **Source Skills**: {}\n- **Semantic Similarity**: {:.2}\n- **Reason**: {}\n\n",
                    sug["suggested_target_name"].as_str().unwrap(),
                    src_names.join(", "),
                    sug["similarity"].as_f64().unwrap(),
                    sug["reason"].as_str().unwrap()
                ));
            }
        }
        store.write_file(suggestions_file, &file_content)?;

        Ok(suggestions)
    }

    pub async fn merge_skills(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
        source_skills: &[String],
        target_name: &str,
    ) -> Result<String> {
        let skills = scan_all_skills(store);
        let mut source_playbooks = Vec::new();

        for name in source_skills {
            if let Some(sk) = skills.iter().find(|s| {
                &s.name == name
                    || s.path
                        .parent()
                        .and_then(|p| p.file_name())
                        .map(|f| f.to_string_lossy().to_string())
                        .as_ref()
                        == Some(name)
            }) {
                source_playbooks.push(sk.clone());
            }
        }

        if source_playbooks.is_empty() {
            anyhow::bail!("No matching source skills found to merge.");
        }

        let mut combined_body = String::new();
        for sk in &source_playbooks {
            combined_body.push_str(&format!(
                "### Skill: {}\nDescription: {}\nBody:\n{}\n\n",
                sk.name, sk.description, sk.body
            ));
        }

        let sys_prompt = "You are a meta-skill synthesizer. Consolidate the provided playbooks into a single, unified agent playbook (SKILL.md). \
                          Keep it lightweight and explicitly instruct the agent to use vector search for details. \
                          Output ONLY the complete SKILL.md content starting with YAML frontmatter containing:\n\
                          ---\n\
                          name: meta-<scope-name>\n\
                          description: <active-voice-summary-of-this-scope-rules>\n\
                          generator_name: MetaSkillSynthesizer\n\
                          ---";

        let prompt_text = format!(
            "Target Name: {}\n\nPlaybooks to Merge:\n\n{}Output ONLY the consolidated SKILL.md:",
            target_name, combined_body
        );

        let response = self
            .llm
            .completion(db, Some(sys_prompt), &prompt_text)
            .await?;
        let trimmed = response.trim();
        let stripped = if trimmed.starts_with("```markdown") {
            trimmed
                .strip_prefix("```markdown")
                .unwrap_or(trimmed)
                .strip_suffix("```")
                .unwrap_or(trimmed)
                .trim()
        } else if trimmed.starts_with("```") {
            trimmed
                .strip_prefix("```")
                .unwrap_or(trimmed)
                .strip_suffix("```")
                .unwrap_or(trimmed)
                .trim()
        } else {
            trimmed
        };

        let project_skills_dir = store.vault_root.join("../.agents/skills");
        let skill_dir = project_skills_dir.join(format!("meta-{}", target_name));
        std::fs::create_dir_all(&skill_dir)?;
        let skill_file = skill_dir.join("SKILL.md");
        std::fs::write(&skill_file, stripped)?;

        let now = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string();
        let trash_dir = store.vault_root.join("../.trash");
        let archive_dir = store.vault_root.join("../.agents/archive/skills");

        for sk in source_playbooks {
            let parent_path = sk
                .path
                .parent()
                .context("No parent folder for skill file")?;
            let folder_name = parent_path
                .file_name()
                .context("No folder name")?
                .to_string_lossy()
                .to_string();

            if sk.is_meta {
                std::fs::create_dir_all(&trash_dir)?;
                let dest = trash_dir.join(format!("{}_{}", folder_name, &now));
                tracing::info!("Trashing meta-skill: {:?} to {:?}", parent_path, dest);
                std::fs::rename(parent_path, dest)?;
            } else {
                std::fs::create_dir_all(&archive_dir)?;
                let dest = archive_dir.join(&folder_name);
                tracing::info!("Archiving custom skill: {:?} to {:?}", parent_path, dest);
                std::fs::rename(parent_path, dest)?;
            }
        }

        Ok(format!("meta-{}", target_name))
    }
}
