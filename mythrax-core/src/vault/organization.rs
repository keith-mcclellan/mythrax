use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    ReferenceAst,
    ReferenceDocs,
    ReferenceForged,
    Fact,
    Insight,
    Direction,
    Hypothesis,
    Episode,
    Skill,
}

impl NodeType {
    pub fn from_category_str(cat: &str) -> Self {
        match cat.to_lowercase().as_str() {
            "ast" | "reference_ast" => NodeType::ReferenceAst,
            "docs" | "reference_docs" => NodeType::ReferenceDocs,
            "forged" | "reference_forged" => NodeType::ReferenceForged,
            "fact" | "facts" => NodeType::Fact,
            "direction" | "directions" => NodeType::Direction,
            "hypothesis" | "hypotheses" => NodeType::Hypothesis,
            "episode" | "episodes" => NodeType::Episode,
            "skill" | "skills" => NodeType::Skill,
            _ => NodeType::Insight,
        }
    }
}

/// Slugifies a title string, capping it at `max_len` characters on a word boundary.
pub fn slugify_title(title: &str, max_len: usize) -> String {
    let sanitized: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();

    let trimmed: String = sanitized
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join("-");

    if trimmed.is_empty() {
        return "untitled".to_string();
    }

    if trimmed.len() <= max_len {
        return trimmed;
    }

    let candidate = &trimmed[..max_len];
    if let Some(last_dash) = candidate.rfind('-') {
        if last_dash > 0 {
            return candidate[..last_dash].to_string();
        }
    }
    candidate.to_string()
}

/// Generates a canonical `<slug_65>-<hash_8>.md` filename for a title and content pair.
pub fn canonical_slug(title: &str, content: &str) -> String {
    use sha2::{Digest, Sha256};
    let base_slug = slugify_title(title, 65);
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    let hash_hex = hex::encode(&result[..4]);
    format!("{}-{}.md", base_slug, hash_hex)
}

/// Generates a relative path for storing an episode note under `.episodes/<YYYY-MM>/<filename>`.
pub fn episode_relative_path(filename: &str) -> String {
    let month = chrono::Utc::now().format("%Y-%m").to_string();
    format!(".episodes/{}/{}", month, filename)
}

/// Routes a filename to its canonical typed directory relative to `vault_root`.
pub fn typed_vault_path(
    vault_root: &Path,
    scope: &str,
    node_type: NodeType,
    filename: &str,
) -> PathBuf {
    let relative = match node_type {
        NodeType::ReferenceAst => format!("wiki/{}/references/ast", scope),
        NodeType::ReferenceDocs => format!("wiki/{}/references/docs", scope),
        NodeType::ReferenceForged => format!("wiki/{}/references/forged", scope),
        NodeType::Fact => format!("wiki/{}/facts", scope),
        NodeType::Insight => format!("wiki/{}/insights", scope),
        NodeType::Direction => format!("wiki/{}/directions", scope),
        NodeType::Hypothesis => format!("wiki/{}/hypotheses", scope),
        NodeType::Episode => {
            let month = chrono::Utc::now().format("%Y-%m").to_string();
            format!(".episodes/{}", month)
        }
        NodeType::Skill => "wisdom/skills".to_string(),
    };
    vault_root.join(relative).join(filename)
}

/// Resolves a path for writing a file to the vault, handling collisions.
/// If a collision occurs (the file exists):
/// - If the existing file has identical content, we can return the same path (or a flag to skip).
/// - If the existing file is different, we can resolve it by generating a suffix (e.g., `_1`, `_2`).
///
/// Ensures parent directories are created.
pub fn organize_file(
    vault_root: &Path,
    category: &str, // e.g., "episodes", "wisdom", "wiki"
    filename: &str, // e.g., "my_note.md"
    content: &str,
) -> Result<PathBuf> {
    let category_dir = vault_root.join(category);
    fs::create_dir_all(&category_dir).context(format!(
        "Failed to create category directory {:?}",
        category_dir
    ))?;

    let base_path = category_dir.join(filename);
    if !base_path.exists() {
        return Ok(base_path);
    }

    // Read existing content
    let existing_content = fs::read_to_string(&base_path).ok();
    if let Some(existing) = existing_content
        && existing == content
    {
        // Content is identical, safe to return base path (overwrite is a no-op)
        return Ok(base_path);
    }

    // Collision! Resolve by adding a numeric suffix.
    let stem = base_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("note");
    let extension = base_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("md");

    let mut counter = 1;
    loop {
        let new_filename = format!("{}_{}.{}", stem, counter, extension);
        let candidate_path = category_dir.join(new_filename);
        if !candidate_path.exists() {
            return Ok(candidate_path);
        }

        // If candidate exists, check content equality
        let candidate_content = fs::read_to_string(&candidate_path).ok();
        if let Some(cand_existing) = candidate_content
            && cand_existing == content
        {
            return Ok(candidate_path);
        }
        counter += 1;
    }
}

pub fn has_enough_disk_space(_path: &Path, min_bytes: u64) -> bool {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();
    if disks.is_empty() {
        return true;
    }
    for disk in &disks {
        if disk.available_space() > 0 && disk.available_space() < min_bytes {
            tracing::warn!("Low disk space warning: {} bytes available", disk.available_space());
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_organize_file_no_collision() {
        let temp = tempdir().unwrap();
        let path = organize_file(temp.path(), "episodes", "test_note.md", "hello world").unwrap();
        assert_eq!(path, temp.path().join("episodes").join("test_note.md"));
        assert!(!path.exists());
    }

    #[test]
    fn test_organize_file_collision_identical_content() {
        let temp = tempdir().unwrap();
        let category_dir = temp.path().join("episodes");
        fs::create_dir_all(&category_dir).unwrap();
        let base_path = category_dir.join("test_note.md");
        fs::write(&base_path, "hello world").unwrap();

        let path = organize_file(temp.path(), "episodes", "test_note.md", "hello world").unwrap();
        assert_eq!(path, base_path);
    }

    #[test]
    fn test_organize_file_collision_different_content() {
        let temp = tempdir().unwrap();
        let category_dir = temp.path().join("episodes");
        fs::create_dir_all(&category_dir).unwrap();
        let base_path = category_dir.join("test_note.md");
        fs::write(&base_path, "hello world").unwrap();

        let path =
            organize_file(temp.path(), "episodes", "test_note.md", "different content").unwrap();
        assert_eq!(path, category_dir.join("test_note_1.md"));
    }

    #[test]
    fn test_canonical_slug_capping_and_crc32() {
        let title = "This is an extremely long title that exceeds sixty five characters and should be safely trimmed at a word boundary";
        let content = "Sample note content for CRC32 calculation";
        let slug = canonical_slug(title, content);
        assert!(slug.ends_with(".md"));
        let stem = &slug[..slug.len() - 3];
        let parts: Vec<&str> = stem.split('-').collect();
        let crc = parts.last().unwrap();
        assert_eq!(crc.len(), 8, "CRC32 hex hash must be 8 hex characters");
        assert!(
            stem.len() <= 74, // 65 char base + 1 dash + 8 crc = 74
            "Stem length must not exceed 74 characters, got: {}",
            stem.len()
        );
    }

    #[test]
    fn test_episode_relative_path() {
        let path = episode_relative_path("claude_log_123.md");
        assert!(path.starts_with(".episodes/"));
        assert!(path.ends_with("/claude_log_123.md"));
        let month = chrono::Utc::now().format("%Y-%m").to_string();
        assert_eq!(path, format!(".episodes/{}/claude_log_123.md", month));
    }

    #[test]
    fn test_typed_vault_path_routing() {
        let temp = tempdir().unwrap();
        let p1 = typed_vault_path(temp.path(), "mythrax", NodeType::Fact, "fact-001.md");
        assert_eq!(p1, temp.path().join("wiki/mythrax/facts/fact-001.md"));

        let p2 = typed_vault_path(temp.path(), "general", NodeType::ReferenceAst, "ast-002.md");
        assert_eq!(p2, temp.path().join("wiki/general/references/ast/ast-002.md"));

        let p3 = typed_vault_path(temp.path(), "mythrax", NodeType::Skill, "skill-003.md");
        assert_eq!(p3, temp.path().join("wisdom/skills/skill-003.md"));
    }

    #[test]
    fn test_disk_space_guard() {
        let temp = tempdir().unwrap();
        assert!(has_enough_disk_space(temp.path(), 1));
        assert!(!has_enough_disk_space(temp.path(), u64::MAX));
    }
}
