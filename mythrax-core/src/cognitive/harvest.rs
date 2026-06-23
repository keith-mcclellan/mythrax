use crate::db::StorageBackend;
use crate::llm::LLMClient;
use crate::store::MarkdownStore;
use crate::contracts::WisdomRule;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct Harvester {
    llm: LLMClient,
}

#[derive(Debug, Clone)]
struct SkillInfo {
    name: String,
    description: String,
    body: String,
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
    }
    let fm: SkillFrontmatter = serde_yaml::from_str(yaml_str)?;
    Ok(SkillInfo {
        name: fm.name,
        description: fm.description,
        body: body.trim().to_string(),
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
                if skill_md_path.exists()
                    && let Ok(skill) = parse_skill_file(&skill_md_path) {
                        skills.push(skill);
                    }
            }
        }
    }
    skills
}

impl Default for Harvester {
    fn default() -> Self {
        Self::new()
    }
}

impl Harvester {
    pub fn new() -> Self {
        Self {
            llm: LLMClient::new(),
        }
    }

    #[allow(dead_code)]
    pub async fn synthesize_user_profile(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
    ) -> Result<()> {
        let episodes = db.get_all_episodes().await?;
        let mut combined_text = String::new();
        for ep in episodes {
            combined_text.push_str(&format!("Episode Title: {}\nContent:\n{}\n\n", ep.title, ep.content));
        }

        if combined_text.is_empty() {
            return Ok(());
        }

        let sys_prompt = "You are a user profiling cognitive engine. Analyze the development log history and synthesize the user's style preferences, coding habits, development constraints, and frequently preferred architectural paradigms.";
        let prompt_text = format!("Developer log history:\n\n{}", combined_text);
        let profile_md = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

        let relative_path = "wiki/user_profile.md";
        let file_content = format!(
            "---\ntype: \"user_profile\"\n---\n\n# User Profile & Development Style\n\n{}",
            profile_md
        );
        store.write_file(relative_path, &file_content)?;

        db.save_profile_key("style_preferences", &profile_md).await?;
        Ok(())
    }

    pub async fn harvest_skills(
        &self,
        db: &dyn StorageBackend,
        store: &MarkdownStore,
    ) -> Result<()> {
        let home = std::env::var("HOME").unwrap_or_default();
        let global_skills_dir = PathBuf::from(home).join(".gemini/config/skills");
        
        let mut project_skills_dir = PathBuf::from(".agents/skills");
        if !project_skills_dir.exists() {
            project_skills_dir = store.vault_root.join("../.agents/skills");
        }

        let mut skills = scan_skills_in_dir(&global_skills_dir);
        skills.extend(scan_skills_in_dir(&project_skills_dir));

        if skills.is_empty() {
            return Ok(());
        }

        let mut skills_prompt = String::new();
        for skill in &skills {
            skills_prompt.push_str(&format!(
                "Skill Name: {}\nDescription: {}\nInstructions:\n{}\n\n",
                skill.name, skill.description, skill.body
            ));
        }

        let sys_prompt = "You are a cognitive harvester. Analyze the following developer skills/playbooks and perform a cross-skill interaction analysis. Identify potential conflicts, overlapping constraints, or compounding rules. Formulate resulting Wisdom Rules to resolve conflicts or enforce compounding constraints.";
        let prompt_text = format!("Skills:\n\n{}Respond ONLY with a JSON array of Wisdom Rules, each containing:\n- target_pattern\n- action_to_avoid\n- causal_explanation\n- prescribed_remedy", skills_prompt);
        let response = self.llm.completion(db, Some(sys_prompt), &prompt_text).await?;

        #[derive(serde::Deserialize)]
        struct RawWisdom {
            target_pattern: String,
            action_to_avoid: String,
            causal_explanation: String,
            prescribed_remedy: String,
        }

        if let Ok(rules) = serde_json::from_str::<Vec<RawWisdom>>(&response) {
            for r in rules {
                let rule_uuid = uuid::Uuid::new_v4().to_string();
                let rule_path = format!("wisdom/skills/{}_{}.md", r.target_pattern.replace([' ', '/'], "_"), &rule_uuid[..8]);
                let rule_md = format!(
                    "---\ntarget_pattern: \"{}\"\naction_to_avoid: \"{}\"\ncausal_explanation: \"{}\"\nprescribed_remedy: \"{}\"\ntier: \"skills\"\nscope: \"general\"\ngenerator_name: \"CrossSkillHarvester\"\n---\n\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}",
                    r.target_pattern, r.action_to_avoid, r.causal_explanation, r.prescribed_remedy,
                    r.target_pattern, r.action_to_avoid, r.causal_explanation, r.prescribed_remedy
                );
                store.write_file(&rule_path, &rule_md)?;

                let rule_contract = WisdomRule {
                    id: None,
                    target_pattern: r.target_pattern,
                    action_to_avoid: r.action_to_avoid,
                    causal_explanation: r.causal_explanation,
                    prescribed_remedy: r.prescribed_remedy,
                    tier: "skills".to_string(),
                    scope: "general".to_string(),
                    vault_path: Some(rule_path),
                    embedding: None,
                    source_episodes: vec![],
                    generator_name: "CrossSkillHarvester".to_string(),
                    similarity: None,
                    utility: None,
                };
                let _ = db.save_wisdom_rule(&rule_contract).await;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cross_skill_harvesting() {
        let tmp = tempdir().unwrap();
        let skill_dir = tmp.path().join("mock-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let skill_md_content = "---
name: MockSkill
description: A mock skill for testing
---
Ensure that no database queries are made without transactions.
";
        std::fs::write(skill_dir.join("SKILL.md"), skill_md_content).unwrap();

        let parsed = parse_skill_file(&skill_dir.join("SKILL.md")).unwrap();
        assert_eq!(parsed.name, "MockSkill");
        assert_eq!(parsed.description, "A mock skill for testing");
        assert!(parsed.body.contains("Ensure that no database queries"));

        let scanned = scan_skills_in_dir(tmp.path());
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].name, "MockSkill");
    }
}
