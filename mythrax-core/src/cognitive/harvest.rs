use crate::contracts::WisdomRule;
use crate::db::StorageBackend;
use crate::llm::LLMClient;
use crate::store::MarkdownStore;
use anyhow::Result;
use std::collections::HashMap;
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
                    && let Ok(skill) = parse_skill_file(&skill_md_path)
                {
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
            combined_text.push_str(&format!(
                "Episode Title: {}\nContent:\n{}\n\n",
                ep.title, ep.content
            ));
        }

        if combined_text.is_empty() {
            return Ok(());
        }

        let sys_prompt = "You are a user profiling cognitive engine. Analyze the development log history and synthesize the user's style preferences, coding habits, development constraints, and frequently preferred architectural paradigms.";
        let prompt_text = format!("Developer log history:\n\n{}", combined_text);
        let profile_md = self
            .llm
            .completion(db, Some(sys_prompt), &prompt_text)
            .await?;

        let relative_path = "wiki/user_profile.md";
        let file_content = format!(
            "---\ntype: \"user_profile\"\n---\n\n# User Profile & Development Style\n\n{}",
            profile_md
        );
        store.write_file(relative_path, &file_content)?;

        db.save_profile_key("style_preferences", &profile_md)
            .await?;
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

        // Generate embeddings for clustering
        let mut emb_refs: Vec<Vec<f32>> = Vec::new();
        for skill in &skills {
            let emb = db.embed(&skill.description).await?;
            emb_refs.push(emb);
        }

        // Perform DBSCAN clustering
        let emb_refs_slices: Vec<&[f32]> = emb_refs.iter().map(|v| v.as_slice()).collect();
        let cluster_assignments = crate::cognitive::synthesis::dbscan(&emb_refs_slices, 0.10, 2);

        // Group skills by cluster ID, ignoring noise (None)
        let mut clusters: HashMap<usize, Vec<SkillInfo>> = HashMap::new();
        for (i, cluster_id) in cluster_assignments.iter().enumerate() {
            if let Some(cid) = cluster_id {
                clusters
                    .entry(*cid)
                    .or_insert_with(Vec::new)
                    .push(skills[i].clone());
            }
        }

        // Process each cluster with size >= 2
        for (_cid, cluster_skills) in clusters {
            if cluster_skills.len() < 2 {
                continue;
            }

            let mut skills_prompt = String::new();
            for skill in &cluster_skills {
                skills_prompt.push_str(&format!(
                    "Skill Name: {}\nDescription: {}\nInstructions:\n{}\n\n",
                    skill.name, skill.description, skill.body
                ));
            }

            let sys_prompt = "You are a cognitive harvester. Analyze the following developer skills/playbooks and perform a cross-skill interaction analysis. Identify potential conflicts, overlapping constraints, or compounding rules. Formulate resulting Wisdom Rules to resolve conflicts or enforce compounding constraints.";
            let prompt_text = format!(
                "Skills:\n\n{}Respond ONLY with a JSON array of Wisdom Rules, each containing:\n- target_pattern\n- action_to_avoid\n- causal_explanation\n- prescribed_remedy",
                skills_prompt
            );
            let response = self
                .llm
                .completion(db, Some(sys_prompt), &prompt_text)
                .await?;

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
                    let rule_path = format!(
                        "wisdom/skills/{}_{}.md",
                        r.target_pattern.replace([' ', '/'], "_"),
                        &rule_uuid[..8]
                    );
                    let rule_md = format!(
                        "---\ntarget_pattern: \"{}\"\naction_to_avoid: \"{}\"\ncausal_explanation: \"{}\"\nprescribed_remedy: \"{}\"\ntier: \"skills\"\nscope: \"general\"\ngenerator_name: \"CrossSkillHarvester\"\n---\n\n# Wisdom Rule: {}\n\n**Action to Avoid:** {}\n\n**Why:** {}\n\n**Prescribed Remedy:** {}",
                        r.target_pattern,
                        r.action_to_avoid,
                        r.causal_explanation,
                        r.prescribed_remedy,
                        r.target_pattern,
                        r.action_to_avoid,
                        r.causal_explanation,
                        r.prescribed_remedy
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
                        status: None,
                        superseded_at: None,
                        superseded_by: None,
                    };
                    let _ = db.save_wisdom_rule(&rule_contract).await;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SurrealBackend;
    use crate::store::MarkdownStore;
    use std::env;
    use std::sync::Mutex;
    use tempfile::tempdir;

    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_cross_skill_harvesting() {
        let tmp = tempdir().unwrap();
        let skill_dir = tmp.path().join("mock-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let skill_md_content = "---\nname: MockSkill\ndescription: A mock skill for testing\n---\nEnsure that no database queries are made without transactions.\n";
        std::fs::write(skill_dir.join("SKILL.md"), skill_md_content).unwrap();

        let parsed = parse_skill_file(&skill_dir.join("SKILL.md")).unwrap();
        assert_eq!(parsed.name, "MockSkill");
        assert_eq!(parsed.description, "A mock skill for testing");
        assert!(parsed.body.contains("Ensure that no database queries"));

        let scanned = scan_skills_in_dir(tmp.path());
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].name, "MockSkill");
    }

    #[tokio::test]
    async fn test_targeted_cross_skill_harvesting() {
        // Lock mutex to prevent race conditions with env vars
        let _guard = TEST_MUTEX.lock().unwrap();

        // Save original env vars
        let original_home = env::var("HOME").ok();
        let original_mock_llm = env::var("MYTHRAX_MOCK_LLM").ok();

        // Initialize in-memory DB (while original HOME is still active so ONNX loads)
        let db = SurrealBackend::new_in_memory().await.unwrap();
        db.init().await.unwrap();

        // If local embedder is not available, skip this test (ONNX models not present)
        if db.embed("test").await.is_err() {
            println!(
                "Skipping test_targeted_cross_skill_harvesting: model files not present in ~/.mythrax/models/"
            );
            return;
        }

        // Setup controlled environment for first check
        let tmp_home = tempdir().unwrap();
        let skills_dir = tmp_home.path().join(".gemini/config/skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        // Write mock skills (DbSkillA and DbSkillB are similar, FeSkillC is outlier)
        let skill_a_content = "---\nname: DbSkillA\ndescription: Ensure database transactions are used for all writes.\n---\nAlways wrap write operations in transactions.";
        let skill_a_dir = skills_dir.join("DbSkillA");
        std::fs::create_dir_all(&skill_a_dir).unwrap();
        std::fs::write(skill_a_dir.join("SKILL.md"), skill_a_content).unwrap();

        let skill_b_content = "---\nname: DbSkillB\ndescription: Ensure database transactions are used for all writes.\n---\nAlways wrap write operations in transactions.";
        let skill_b_dir = skills_dir.join("DbSkillB");
        std::fs::create_dir_all(&skill_b_dir).unwrap();
        std::fs::write(skill_b_dir.join("SKILL.md"), skill_b_content).unwrap();

        let skill_c_content = "---\nname: FeSkillC\ndescription: Ensure all UI components are stateless.\n---\nDo not use local state in React components.";
        let skill_c_dir = skills_dir.join("FeSkillC");
        std::fs::create_dir_all(&skill_c_dir).unwrap();
        std::fs::write(skill_c_dir.join("SKILL.md"), skill_c_content).unwrap();

        // Set HOME to our temp dir and set MOCK_LLM
        unsafe {
            env::set_var("HOME", tmp_home.path().to_str().unwrap());
            env::set_var("MYTHRAX_MOCK_LLM", "true");
        }

        // Create a mock store with a vault root
        let tmp_vault = tempdir().unwrap();
        let store = MarkdownStore::new(tmp_vault.path()).unwrap();

        // Run harvest
        let harvester = Harvester::new();
        let result = harvester.harvest_skills(&db, &store).await;
        assert!(result.is_ok());

        // Check that exactly 1 rule was generated (from the cluster of A and B)
        // C is an outlier and should be skipped.
        let rules = db.get_all_wisdom_rules().await.unwrap();
        assert_eq!(
            rules.len(),
            1,
            "Expected exactly 1 rule from clustered skills"
        );

        // Clear skills dir and prepare for the second check: outliers only
        std::fs::remove_dir_all(&skills_dir).unwrap();
        std::fs::create_dir_all(&skills_dir).unwrap();

        // Write outlier skills only (do not cluster)
        let skill_d_content = "---\nname: DbSkillOnly\ndescription: Ensure database transactions are used for all writes.\n---\nAlways wrap write operations in transactions.";
        let skill_d_dir = skills_dir.join("DbSkillOnly");
        std::fs::create_dir_all(&skill_d_dir).unwrap();
        std::fs::write(skill_d_dir.join("SKILL.md"), skill_d_content).unwrap();

        let skill_e_content = "---\nname: FeSkillOnly\ndescription: Ensure all UI components are stateless.\n---\nDo not use local state in React components.";
        let skill_e_dir = skills_dir.join("FeSkillOnly");
        std::fs::create_dir_all(&skill_e_dir).unwrap();
        std::fs::write(skill_e_dir.join("SKILL.md"), skill_e_content).unwrap();

        // Initialize a new isolated DB for the second check
        // We must restore HOME temporarily to load the embedder, then swap it back
        unsafe {
            if let Some(ref home) = original_home {
                env::set_var("HOME", home);
            } else {
                env::remove_var("HOME");
            }
        }
        let db2 = SurrealBackend::new_in_memory().await.unwrap();
        db2.init().await.unwrap();
        unsafe {
            env::set_var("HOME", tmp_home.path().to_str().unwrap());
        }

        let tmp_vault2 = tempdir().unwrap();
        let store2 = MarkdownStore::new(tmp_vault2.path()).unwrap();

        // Run harvest again
        let result2 = harvester.harvest_skills(&db2, &store2).await;
        assert!(result2.is_ok());

        // Check that no rules were generated (outliers don't cluster)
        let rules2 = db2.get_all_wisdom_rules().await.unwrap();
        assert_eq!(
            rules2.len(),
            0,
            "Expected no rules from outlier-only skills"
        );

        // Restore original env vars
        unsafe {
            if let Some(home) = original_home {
                env::set_var("HOME", home);
            } else {
                env::remove_var("HOME");
            }
            if let Some(mock) = original_mock_llm {
                env::set_var("MYTHRAX_MOCK_LLM", mock);
            } else {
                env::remove_var("MYTHRAX_MOCK_LLM");
            }
        }
    }
}
