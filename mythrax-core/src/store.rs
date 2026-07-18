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

        let store = Self { vault_root: root };
        
        // Ensure vault structure, directories, MOC.md, and gitignore exclusions are created/updated
        store.ensure_vault_structure()?;

        // Clean up zombie STM files
        if let Err(e) = cleanup_zombie_stm_files(&store.vault_root) {
            tracing::warn!("Failed to clean up zombie STM files: {}", e);
        }

        // Set backup exclusion attribute on database directory
        if let Err(e) = set_db_backup_exclusion() {
            tracing::warn!("Failed to set database backup exclusion: {}", e);
        }

        Ok(store)
    }

    pub fn ensure_vault_structure(&self) -> Result<()> {
        // Create new subdirectories
        fs::create_dir_all(self.vault_root.join("directions"))?;
        fs::create_dir_all(self.vault_root.join("insights"))?;
        fs::create_dir_all(self.vault_root.join("pruned"))?;
        fs::create_dir_all(self.vault_root.join("wisdom"))?;
        fs::create_dir_all(self.vault_root.join("reference"))?;

        // 1. Manage gitignore to ignore .handoffs/
        let gitignore_path = self.vault_root.join(".gitignore");
        let mut gitignore_content = if gitignore_path.exists() {
            fs::read_to_string(&gitignore_path)?
        } else {
            String::new()
        };

        let has_handoffs = gitignore_content.lines().any(|l| {
            let trimmed = l.trim();
            trimmed == ".handoffs/" || trimmed == ".handoffs"
        });

        if !has_handoffs {
            if !gitignore_content.ends_with('\n') && !gitignore_content.is_empty() {
                gitignore_content.push('\n');
            }
            gitignore_content.push_str(".handoffs/\n");
            fs::write(&gitignore_path, &gitignore_content)?;
            set_file_permissions_644(&gitignore_path)?;
        }

        // 2. Generate MOC.md at the vault root
        let moc_path = self.vault_root.join("MOC.md");
        let moc_content = r#"# Map of Content (MOC)

Welcome to the Mythrax Vault.

## Vault Folders
- [[directions/|Directions]]
- [[insights/|Insights]]
- [[pruned/|Pruned]]
- [[wisdom/|Wisdom]]
- [[reference/|Reference]]
"#;
        fs::write(&moc_path, moc_content)?;
        set_file_permissions_644(&moc_path)?;

        Ok(())
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
        fs::rename(tmp_path, &dest_path)
            .context("Failed to atomically rename temporary vault file")?;
            
        // Enforce 0644 file permissions
        set_file_permissions_644(&dest_path)?;

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
    fs::rename(tmp_path, &file_path)?;
    set_file_permissions_644(&file_path)?;
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

#[cfg(unix)]
fn set_file_permissions_644<P: AsRef<Path>>(path: P) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let path = path.as_ref();
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_permissions_644<P: AsRef<Path>>(_path: P) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_exclude_from_backup<P: AsRef<Path>>(path: P) -> Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path_str = CString::new(path.as_ref().as_os_str().as_bytes())?;
    let attr_name = CString::new("com.apple.metadata:com_apple_backup_excludeItem")?;
    let attr_value = b"com.apple.backupd";

    unsafe {
        let res = libc::setxattr(
            path_str.as_ptr(),
            attr_name.as_ptr(),
            attr_value.as_ptr() as *const libc::c_void,
            attr_value.len(),
            0,
            0,
        );
        if res != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EEXIST) {
                tracing::warn!(
                    "Failed to set xattr backup exclusion on {:?}: {}",
                    path.as_ref(),
                    err
                );
            }
        }
    }
    Ok(())
}

fn set_db_backup_exclusion() -> Result<()> {
    // Bypass in test environment to avoid modifying the live ~/.mythrax directory
    if std::env::var("MYTHRAX_TEST_MOCK").is_ok() || std::env::var("CARGO_MANIFEST_DIR").is_ok() {
        return Ok(());
    }

    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return Ok(());
    }
    
    let mythrax_dir = PathBuf::from(&home).join(".mythrax");
    let db_dir = mythrax_dir.join("db.nosync");
    
    // Create the DB directory if it doesn't exist
    fs::create_dir_all(&db_dir)?;
    
    // Create `.nosync` files
    let nosync_file1 = mythrax_dir.join(".nosync");
    if !nosync_file1.exists() {
        let _ = fs::write(&nosync_file1, "");
        let _ = set_file_permissions_644(&nosync_file1);
    }
    let nosync_file2 = db_dir.join(".nosync");
    if !nosync_file2.exists() {
        let _ = fs::write(&nosync_file2, "");
        let _ = set_file_permissions_644(&nosync_file2);
    }
    
    #[cfg(target_os = "macos")]
    {
        let _ = set_exclude_from_backup(&mythrax_dir);
        let _ = set_exclude_from_backup(&db_dir);
    }
    Ok(())
}

fn is_mythrax_process_alive(pid_val: i32) -> bool {
    use sysinfo::{System, Pid};
    let mut system = System::new();
    let pid = Pid::from(pid_val as usize);
    system.refresh_process(pid);
    if let Some(process) = system.process(pid) {
        let name = process.name();
        name.contains("mythrax")
    } else {
        false
    }
}

pub fn cleanup_zombie_stm_files<P: AsRef<Path>>(vault_root: P) -> Result<()> {
    let handoffs_dir = vault_root.as_ref().join(".handoffs");
    if !handoffs_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(handoffs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                if filename.starts_with("stm_") && filename.ends_with(".json") {
                    let id_str = &filename[4..filename.len() - 5];
                    let mut pid_to_check = None;

                    // 1. Try to parse filename ID as PID
                    if let Ok(pid) = id_str.parse::<i32>() {
                        pid_to_check = Some(pid);
                    } else {
                        // 2. Try to parse file contents to see if it has a pid/session_pid field
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&content) {
                                if let Some(p) = json_val.get("pid").and_then(|v| v.as_i64()) {
                                    pid_to_check = Some(p as i32);
                                } else if let Some(p) = json_val.get("_pid").and_then(|v| v.as_i64()) {
                                    pid_to_check = Some(p as i32);
                                }
                            }
                        }
                    }

                    if let Some(pid) = pid_to_check {
                        if !is_mythrax_process_alive(pid) {
                            tracing::info!("Cleaning up orphaned/zombie STM file: {:?}", path);
                            let _ = fs::remove_file(path);
                        }
                    }
                }
            }
        }
    }
    Ok(())
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

    #[test]
    fn test_ensure_vault_structure() {
        let tmp = tempdir().unwrap();
        let _store = MarkdownStore::new(tmp.path()).unwrap();

        // 1. Check directories
        assert!(tmp.path().join("directions").exists());
        assert!(tmp.path().join("insights").exists());
        assert!(tmp.path().join("pruned").exists());
        assert!(tmp.path().join("wisdom").exists());
        assert!(tmp.path().join("reference").exists());

        // 2. Check MOC.md content
        let moc_path = tmp.path().join("MOC.md");
        assert!(moc_path.exists());
        let moc_content = fs::read_to_string(&moc_path).unwrap();
        assert!(moc_content.contains("# Map of Content (MOC)"));
        assert!(moc_content.contains("directions/"));
        assert!(moc_content.contains("insights/"));

        // 3. Check gitignore content
        let gitignore_path = tmp.path().join(".gitignore");
        assert!(gitignore_path.exists());
        let gitignore_content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(gitignore_content.contains(".handoffs/"));
        
        // 4. File permissions check (on Unix/macOS)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&moc_path).unwrap();
            let mode = metadata.permissions().mode();
            assert_eq!(mode & 0o777, 0o644);

            let gitignore_metadata = fs::metadata(&gitignore_path).unwrap();
            let gitignore_mode = gitignore_metadata.permissions().mode();
            assert_eq!(gitignore_mode & 0o777, 0o644);
        }
    }

    #[test]
    fn test_zombie_cleanup() {
        let tmp = tempdir().unwrap();
        let handoffs_dir = tmp.path().join(".handoffs");
        fs::create_dir_all(&handoffs_dir).unwrap();

        // 1. Create a dead PID file (using a very high PID that is unlikely to be running, e.g., 999999)
        let dead_pid = 999999;
        let dead_file = handoffs_dir.join(format!("stm_{}.json", dead_pid));
        fs::write(&dead_file, "{}").unwrap();

        // 2. Create an alive PID file (using our own process PID)
        let alive_pid = std::process::id();
        let alive_file = handoffs_dir.join(format!("stm_{}.json", alive_pid));
        fs::write(&alive_file, "{}").unwrap();

        // Run cleanup
        cleanup_zombie_stm_files(tmp.path()).unwrap();

        // Dead PID file should be cleaned up
        assert!(!dead_file.exists());
    }

    #[test]
    fn test_permissions_enforced_644() {
        let tmp = tempdir().unwrap();
        let store = MarkdownStore::new(tmp.path()).unwrap();

        let rel_path = "episodes/test_perm.md";
        store.write_file(rel_path, "Some content").unwrap();

        let dest = tmp.path().join(rel_path);
        assert!(dest.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(dest).unwrap();
            let mode = metadata.permissions().mode();
            assert_eq!(mode & 0o777, 0o644);
        }
    }
}
