use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, anyhow};

pub struct ArborExecutor {
    repo_path: PathBuf,
}

impl ArborExecutor {
    pub fn new(repo_path: PathBuf) -> Self {
        Self { repo_path }
    }

    pub fn execute(
        &self,
        node_id: &str,
        commit_sha: &str,
        test_command: &str,
        code_changes: &Option<std::collections::HashMap<String, String>>,
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
                .args([
                    "worktree",
                    "add",
                    "--detach",
                    &worktree_dir,
                    commit_sha,
                ])
                .current_dir(&self.repo_path)
                .status()?;
            if !status2.success() {
                return Err(anyhow!("Failed to add git worktree at {} for commit {}", worktree_dir, commit_sha));
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

        // Spawns a subprocess to execute the test suite in the temp directory
        let output = Command::new("sh")
            .arg("-c")
            .arg(test_command)
            .current_dir(&worktree_path)
            .output()?;

        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined_logs = format!("{}\n{}", stdout, stderr);

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

        Ok(())
    }
}
