use crate::db::StorageBackend;
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct ArborExecutor {
    repo_path: PathBuf,
}

impl ArborExecutor {
    pub fn new(repo_path: PathBuf) -> Self {
        Self { repo_path }
    }

    pub async fn execute(
        &self,
        node_id: &str,
        commit_sha: &str,
        test_command: &str,
        code_changes: &Option<std::collections::HashMap<String, String>>,
        backend: &crate::db::SurrealBackend,
    ) -> Result<(bool, String)> {
        let worktree_dir = format!("/tmp/worktree-node-{}", node_id);
        let worktree_path = PathBuf::from(&worktree_dir);

        // Ensure clean state: remove if already exists
        if worktree_path.exists() {
            let _ = self.cleanup_worktree(node_id);
        }

        // git worktree add -b worktree-node-<id> /tmp/worktree-node-<id> <commit_sha>
        let branch_name = format!("worktree-node-{}", node_id);

        // Check if branch already exists and delete it to avoid conflict
        let _ = Command::new("git")
            .args(["branch", "-D", &branch_name])
            .current_dir(&self.repo_path)
            .status();

        let status = Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &branch_name,
                &worktree_dir,
                commit_sha,
            ])
            .current_dir(&self.repo_path)
            .status()?;

        if !status.success() {
            // Fallback: try checking it out as a detached head
            let status2 = Command::new("git")
                .args(["worktree", "add", "--detach", &worktree_dir, commit_sha])
                .current_dir(&self.repo_path)
                .status()?;
            if !status2.success() {
                return Err(anyhow!(
                    "Failed to add git worktree at {} for commit {}",
                    worktree_dir,
                    commit_sha
                ));
            }
        }

        // Apply code changes if present
        if let Some(changes) = code_changes {
            for (rel_path, content) in changes {
                let file_path = worktree_path.join(rel_path);
                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&file_path, content)?;
            }
        }

        // Spawns a subprocess to execute the test suite in the temp directory.
        // If shell operators (pipe, redirect, chain) are detected, we use sh -c.
        // Otherwise we split respecting quotes and execute natively without shell wrapping.
        let has_shell_operators = test_command.contains('&')
            || test_command.contains('|')
            || test_command.contains('>')
            || test_command.contains('<')
            || test_command.contains(';');

        let mut cmd = if has_shell_operators {
            let mut c = Command::new("sh");
            c.arg("-c").arg(test_command);
            c
        } else {
            let mut args = Vec::new();
            let mut current_arg = String::new();
            let mut in_quotes = false;
            let mut quote_char = '\0';
            for c in test_command.chars() {
                match c {
                    '"' | '\'' if !in_quotes => {
                        in_quotes = true;
                        quote_char = c;
                    }
                    '"' | '\'' if in_quotes && c == quote_char => {
                        in_quotes = false;
                        quote_char = '\0';
                    }
                    ' ' | '\t' if !in_quotes => {
                        if !current_arg.is_empty() {
                            args.push(current_arg.clone());
                            current_arg.clear();
                        }
                    }
                    _ => {
                        current_arg.push(c);
                    }
                }
            }
            if !current_arg.is_empty() {
                args.push(current_arg);
            }
            if args.is_empty() {
                return Err(anyhow!("Empty test command"));
            }
            let mut c = Command::new(&args[0]);
            if args.len() > 1 {
                c.args(&args[1..]);
            }
            c
        };
        cmd.current_dir(&worktree_path);

        // T6: Set unique CARGO_TARGET_DIR and offline env vars
        let target_dir = format!("/tmp/cargo-target-node-{}", node_id);
        cmd.env("CARGO_TARGET_DIR", &target_dir);
        cmd.env("CARGO_NET_OFFLINE", "true");

        let output = cmd.output()?;

        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let mut combined_logs = format!("{}\n{}", stdout, stderr);

        if !success {
            if let Ok(Some((explanation, remedy))) =
                backend.diagnose_error_internal(&stderr, &stdout).await
            {
                combined_logs.push_str(&format!(
                    "\n---\n[MYTHRAX AUTO-DIAGNOSTIC]: A matching failure was resolved in the database.\n- Causal Explanation: {}\n- Prescribed Remedy: {}\n---\n",
                    explanation, remedy
                ));
            }
        }

        // Clean up
        self.cleanup_worktree(node_id)?;

        Ok((success, combined_logs))
    }

    pub fn cleanup_worktree(&self, node_id: &str) -> Result<()> {
        let worktree_dir = format!("/tmp/worktree-node-{}", node_id);
        let branch_name = format!("worktree-node-{}", node_id);

        // Remove worktree: git worktree remove --force /tmp/worktree-node-<id>
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", &worktree_dir])
            .current_dir(&self.repo_path)
            .status();

        // Delete branch: git branch -D worktree-node-<id>
        let _ = Command::new("git")
            .args(["branch", "-D", &branch_name])
            .current_dir(&self.repo_path)
            .status();

        // Ensure folder is deleted
        let path = Path::new(&worktree_dir);
        if path.exists() {
            let _ = std::fs::remove_dir_all(path);
        }

        // Clean up cargo target directory if exists
        let target_dir = format!("/tmp/cargo-target-node-{}", node_id);
        let target_path = Path::new(&target_dir);
        if target_path.exists() {
            let _ = std::fs::remove_dir_all(target_path);
        }

        Ok(())
    }
}
