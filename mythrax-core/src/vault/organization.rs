use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

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
}
