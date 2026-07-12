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
        fs::create_dir_all(root.join("wisdom/skills"))?;
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

    pub fn append_link_to_file(&self, file_path: &str, section_title: &str, link_path: &str, link_label: &str) -> Result<()> {
        let dest_path = self.vault_root.join(file_path);
        if !dest_path.exists() {
            return Ok(());
        }
        let mut content = fs::read_to_string(&dest_path)?;
        let link_target = link_path.strip_suffix(".md").unwrap_or(link_path);
        let link_str = format!("- [[{}|{}]]", link_target, link_label);

        if content.contains(&link_str) {
            return Ok(());
        }

        let section_header = format!("## {}", section_title);
        if !content.contains(&section_header) {
            if !content.ends_with('\n') && !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&format!("\n{}\n", section_header));
        }

        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&link_str);
        self.write_file(file_path, &content)?;
        Ok(())
    }
}

use std::sync::RwLock;

static WORKSPACE_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

pub fn set_workspace_root(path: PathBuf) {
    if let Ok(mut lock) = WORKSPACE_ROOT.write() {
        *lock = Some(path);
    }
}

pub fn get_workspace_root() -> Option<PathBuf> {
    if let Ok(lock) = WORKSPACE_ROOT.read() {
        lock.clone()
    } else {
        None
    }
}

pub fn clear_workspace_root() {
    if let Ok(mut lock) = WORKSPACE_ROOT.write() {
        *lock = None;
    }
}

pub fn find_vault_root() -> PathBuf {
    if let Some(root) = get_workspace_root() {
        return root;
    }
    if let Ok(val) = std::env::var("MYTHRAX_VAULT_ROOT") {
        return PathBuf::from(val);
    }
    if let Ok(val) = std::env::var("MYTHRAX_WORKSPACE_ROOT") {
        return PathBuf::from(val);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        let config_path = PathBuf::from(&home).join(".mythrax").join("config.json");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(config_val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(vault_root) = config_val["vault_root"].as_str() {
                        return PathBuf::from(vault_root);
                    }
                }
            }
        }
    }
    PathBuf::from(&home).join("mythrax-vault")
}

pub fn save_stm_file(session_id: &str, key: &str, value: &str) -> Result<()> {
    let root = find_vault_root();
    let handoffs_dir = root.join(".handoffs");
    tracing::debug!("save_stm_file session_id={} root={:?} handoffs_dir={:?}", session_id, root, handoffs_dir);
    fs::create_dir_all(&handoffs_dir)?;

    let file_path = handoffs_dir.join(format!("stm_{}.json", session_id));
    
    let mut map = if file_path.exists() {
        let content = fs::read_to_string(&file_path).unwrap_or_default();
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content)
            .unwrap_or_else(|_| serde_json::Map::new())
    } else {
        serde_json::Map::new()
    };

    let sanitized_value = SecretFilter::clean(value);
    map.insert(key.to_string(), serde_json::Value::String(sanitized_value));

    let updated_content = serde_json::to_string_pretty(&map)?;
    
    let tmp_path = file_path.with_extension("tmp");
    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(updated_content.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(tmp_path, file_path)?;
    Ok(())
}

pub fn delete_stm_file(session_id: &str) -> Result<()> {
    let root = find_vault_root();
    let file_path = root.join(".handoffs").join(format!("stm_{}.json", session_id));
    if file_path.exists() {
        fs::remove_file(file_path)?;
    }
    Ok(())
}

pub fn read_config_json() -> serde_json::Value {
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        let config_path = PathBuf::from(&home).join(".mythrax").join("config.json");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(config_val) = serde_json::from_str::<serde_json::Value>(&content) {
                    return config_val;
                }
            }
        }
    }
    serde_json::Value::Null
}

pub fn get_config_val_str(key1: &str, key2: &str, default: &str) -> String {
    let config = read_config_json();
    if let Some(val) = config.get(key1).and_then(|v| v.get(key2)) {
        if let Some(s) = val.as_str() {
            return s.to_string();
        }
    }
    let flat_key = format!("{}.{}", key1, key2);
    if let Some(val) = config.get(&flat_key) {
        if let Some(s) = val.as_str() {
            return s.to_string();
        }
    }
    default.to_string()
}

pub fn get_config_val_int(key1: &str, key2: &str, default: i64) -> i64 {
    let config = read_config_json();
    if let Some(val) = config.get(key1).and_then(|v| v.get(key2)) {
        if let Some(i) = val.as_i64() {
            return i;
        }
    }
    let flat_key = format!("{}.{}", key1, key2);
    if let Some(val) = config.get(&flat_key) {
        if let Some(i) = val.as_i64() {
            return i;
        }
    }
    default
}

pub fn get_config_val_float(key1: &str, key2: &str, default: f64) -> f64 {
    let config = read_config_json();
    if let Some(val) = config.get(key1).and_then(|v| v.get(key2)) {
        if let Some(f) = val.as_f64() {
            return f;
        }
    }
    let flat_key = format!("{}.{}", key1, key2);
    if let Some(val) = config.get(&flat_key) {
        if let Some(f) = val.as_f64() {
            return f;
        }
    }
    default
}

pub fn get_config_val_bool(key1: &str, key2: &str, default: bool) -> bool {
    let config = read_config_json();
    if let Some(val) = config.get(key1).and_then(|v| v.get(key2)) {
        if let Some(b) = val.as_bool() {
            return b;
        }
    }
    let flat_key = format!("{}.{}", key1, key2);
    if let Some(val) = config.get(&flat_key) {
        if let Some(b) = val.as_bool() {
            return b;
        }
    }
    default
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

    #[test]
    fn test_find_vault_root() {
        unsafe {
            std::env::set_var("MYTHRAX_VAULT_ROOT", "/tmp/vault_test_env");
        }
        assert_eq!(find_vault_root(), PathBuf::from("/tmp/vault_test_env"));
        unsafe {
            std::env::remove_var("MYTHRAX_VAULT_ROOT");
        }

        set_workspace_root(PathBuf::from("/tmp/workspace_test_env"));
        assert_eq!(find_vault_root(), PathBuf::from("/tmp/workspace_test_env"));
        // Clean up
        if let Ok(mut lock) = WORKSPACE_ROOT.write() {
            *lock = None;
        }
    }

    #[test]
    fn test_append_link_to_file() {
        let tmp = tempdir().unwrap();
        let store = MarkdownStore::new(tmp.path()).unwrap();

        let rel_path = "episodes/test_episode.md";
        let content = "title: Test\nSome episode content.";
        store.write_file(rel_path, content).unwrap();

        // 1. Append a link for the first time
        store.append_link_to_file(rel_path, "Insights & Summaries", "wiki/scope/insights/My_Insight.md", "My Insight").unwrap();
        
        let dest = tmp.path().join(rel_path);
        let read_content_1 = fs::read_to_string(&dest).unwrap();
        assert!(read_content_1.contains("## Insights & Summaries"));
        assert!(read_content_1.contains("- [[wiki/scope/insights/My_Insight|My Insight]]"));

        // 2. Append the same link again (should not duplicate)
        store.append_link_to_file(rel_path, "Insights & Summaries", "wiki/scope/insights/My_Insight.md", "My Insight").unwrap();
        let read_content_2 = fs::read_to_string(&dest).unwrap();
        
        // Count occurrences of the link string
        let occurrences = read_content_2.matches("[[wiki/scope/insights/My_Insight|My Insight]]").count();
        assert_eq!(occurrences, 1);
    }
}
