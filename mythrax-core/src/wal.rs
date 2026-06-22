#![allow(dead_code)]

use serde::{Serialize, Deserialize};
use std::fs::{OpenOptions, File};
use std::io::{Write, BufRead, BufReader};
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalEntry {
    pub id: String,
    pub timestamp: u64,
    pub action: String, // "save_episode" | "save_wisdom"
    pub payload: serde_json::Value,
    pub status: String, // "pending" | "committed"
}

pub struct WriteAheadLog {
    wal_path: PathBuf,
}

impl WriteAheadLog {
    pub fn new<P: AsRef<Path>>(wal_path: P) -> Result<Self> {
        let path = wal_path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Self { wal_path: path })
    }

    pub fn log_intent(&self, id: &str, action: &str, payload: &serde_json::Value) -> Result<()> {
        let entry = WalEntry {
            id: id.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            action: action.to_string(),
            payload: payload.clone(),
            status: "pending".to_string(),
        };
        self.append_entry(&entry)
    }

    pub fn log_commit(&self, id: &str) -> Result<()> {
        let entry = WalEntry {
            id: id.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            action: "".to_string(),
            payload: serde_json::Value::Null,
            status: "committed".to_string(),
        };
        self.append_entry(&entry)
    }

    fn append_entry(&self, entry: &WalEntry) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.wal_path)
            .context("Failed to open WAL file")?;
        
        let json_line = serde_json::to_string(entry)?;
        writeln!(file, "{}", json_line)?;
        file.sync_all()?;
        Ok(())
    }

    pub fn get_pending_entries(&self) -> Result<Vec<WalEntry>> {
        if !self.wal_path.exists() {
            return Ok(vec![]);
        }

        let file = File::open(&self.wal_path)?;
        let reader = BufReader::new(file);
        
        let mut entries = std::collections::HashMap::new();
        
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<WalEntry>(&line) {
                if entry.status == "pending" {
                    entries.insert(entry.id.clone(), entry);
                } else if entry.status == "committed" {
                    entries.remove(&entry.id);
                }
            }
        }

        Ok(entries.into_values().collect())
    }

    pub fn clear(&self) -> Result<()> {
        if self.wal_path.exists() {
            std::fs::remove_file(&self.wal_path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_wal_pending_and_commit() {
        let temp = NamedTempFile::new().unwrap();
        let wal = WriteAheadLog::new(temp.path()).unwrap();

        let payload = serde_json::json!({ "title": "Test" });
        wal.log_intent("ep1", "save_episode", &payload).unwrap();

        let pending = wal.get_pending_entries().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "ep1");

        wal.log_commit("ep1").unwrap();
        let pending = wal.get_pending_entries().unwrap();
        assert_eq!(pending.len(), 0);
    }
}
