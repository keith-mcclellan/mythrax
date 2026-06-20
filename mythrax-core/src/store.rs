use anyhow::{Result, Context};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use crate::secret_filter::SecretFilter;

pub struct MarkdownStore {
    pub vault_root: PathBuf,
}

impl MarkdownStore {
    pub fn new<P: AsRef<Path>>(vault_root: P) -> Result<Self> {
        let root = vault_root.as_ref().to_path_buf();
        fs::create_dir_all(&root).context("Failed to create vault root directory")?;
        
        // Initialize vault folders
        fs::create_dir_all(root.join("episodes"))?;
        fs::create_dir_all(root.join("wisdom/pinned"))?;
        fs::create_dir_all(root.join("wisdom/permanent"))?;
        fs::create_dir_all(root.join("wisdom/dynamic"))?;
        fs::create_dir_all(root.join("general"))?;
        fs::create_dir_all(root.join("archive"))?;

        Ok(Self { vault_root: root })
    }

    pub fn write_file(&self, relative_path: &str, content: &str) -> Result<()> {
        let dest_path = self.vault_root.join(relative_path);
        
        // Ensure parent directory exists
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = dest_path.with_extension("tmp");

        // 1. Run SecretFilter scanning
        let sanitized_content = SecretFilter::clean(content);

        // 2. Write to temporary file
        let mut file = File::create(&tmp_path)
            .context("Failed to create temporary vault file")?;
        file.write_all(sanitized_content.as_bytes())?;
        file.sync_all()?;

        // 3. Atomically replace destination (standard POSIX rename)
        fs::rename(tmp_path, dest_path)
            .context("Failed to atomically rename temporary vault file")?;
            
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_atomic_write() {
        let tmp = tempdir().unwrap();
        let store = MarkdownStore::new(tmp.path()).unwrap();

        let rel_path = "episodes/test_episode.md";
        let content = "title: Test\napi_key: 'secret'\nThis is my episode body.";
        store.write_file(rel_path, content).unwrap();

        let dest = tmp.path().join(rel_path);
        assert!(dest.exists());

        let read_content = fs::read_to_string(dest).unwrap();
        assert!(read_content.contains("[REDACTED]"));
        assert!(!read_content.contains("secret"));
    }
}
