use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, anyhow};
use crate::db::StorageBackend;

fn get_jitter() -> u64 {
    use std::time::SystemTime;
    if let Ok(duration) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        (duration.as_nanos() % 50) as u64
    } else {
        25
    }
}

fn run_git_command_with_retry(args: &[&str], dir: &Path) -> Result<std::process::ExitStatus> {
    use std::time::Duration;
    let mut attempts = 0;
    let max_attempts = 5;
    let mut delay = Duration::from_millis(100);

    loop {
        attempts += 1;
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status();

        match status {
            Ok(s) if s.success() => return Ok(s),
            Ok(s) => {
                if attempts >= max_attempts {
                    return Ok(s);
                }
            }
            Err(e) => {
                if attempts >= max_attempts {
                    return Err(e.into());
                }
            }
        }

        let sleep_dur = delay + Duration::from_millis(get_jitter());
        std::thread::sleep(sleep_dur);
        delay *= 2;
    }
}

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

        let branch_name = format!("htr_branch_{}", node_id);
        
        // Check if branch exists
        let branch_exists = Command::new("git")
            .args(["show-ref", "--verify", &format!("refs/heads/{}", branch_name)])
            .current_dir(&self.repo_path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let status = if branch_exists {
            run_git_command_with_retry(
                &[
                    "worktree",
                    "add",
                    &worktree_dir,
                    &branch_name,
                ],
                &self.repo_path,
            )?
        } else {
            let res = run_git_command_with_retry(
                &[
                    "worktree",
                    "add",
                    "-b",
                    &branch_name,
                    &worktree_dir,
                    commit_sha,
                ],
                &self.repo_path,
            );
            
            if res.is_err() || !res.as_ref().unwrap().success() {
                // Fallback: try checking it out as a detached head
                run_git_command_with_retry(
                    &[
                        "worktree",
                        "add",
                        "--detach",
                        &worktree_dir,
                        commit_sha,
                    ],
                    &self.repo_path,
                )?
            } else {
                res?
            }
        };

        if !status.success() {
            return Err(anyhow!("Failed to add git worktree at {} for branch/commit {}", worktree_dir, commit_sha));
        }

        // Apply code changes if present
        if let Some(changes) = code_changes {
            let mut has_changes = false;
            for (rel_path, content) in changes {
                let file_path = worktree_path.join(rel_path);
                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&file_path, content)?;
                
                let add_status = Command::new("git")
                    .args(["add", rel_path])
                    .current_dir(&worktree_path)
                    .status();
                if let Ok(status) = add_status {
                    if status.success() {
                        has_changes = true;
                    }
                }
            }
            if has_changes {
                let _ = Command::new("git")
                    .args(["commit", "-m", &format!("HTR Auto-Commit for node {}", node_id)])
                    .current_dir(&worktree_path)
                    .status();
            }
        }

        let has_shell_operators = test_command.contains('&') || test_command.contains('|') || test_command.contains('>') || test_command.contains('<') || test_command.contains(';');

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
                            args.push(std::mem::replace(&mut current_arg, String::new()));
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

        // Set unique target directory target/htr_<node_uuid>
        let target_dir = format!("target/htr_{}", node_id);
        cmd.env("CARGO_TARGET_DIR", &target_dir);
        cmd.env("CARGO_NET_OFFLINE", "true");

        // Override daemon API/DB server ports dynamically to avoid lock contention
        let port_offset = (node_id.chars().map(|c| c as u32).sum::<u32>() % 1000) as u16;
        let api_port = 20000 + port_offset * 2;
        let db_port = 20000 + port_offset * 2 + 1;
        cmd.env("MYTHRAX_API_PORT", api_port.to_string());
        cmd.env("MYTHRAX_DB_PORT", db_port.to_string());

        let output = cmd.output()?;

        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let mut combined_logs = format!("{}\n{}", stdout, stderr);

        if !success {
            if let Ok(Some((explanation, remedy))) = backend.diagnose_error_internal(&stderr, &stdout).await {
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

        // Remove worktree: git worktree remove --force /tmp/worktree-node-<id>
        let _ = run_git_command_with_retry(
            &["worktree", "remove", "--force", &worktree_dir],
            &self.repo_path,
        );

        // Do NOT delete the branch pointers htr_branch_<node_uuid> on cleanup; preserve them.

        // Ensure folder is deleted
        let path = Path::new(&worktree_dir);
        if path.exists() {
            let _ = std::fs::remove_dir_all(path);
        }

        // Clean up cargo target directory if exists
        let target_dir = format!("target/htr_{}", node_id);
        let target_path = Path::new(&target_dir);
        if target_path.exists() {
            let _ = std::fs::remove_dir_all(target_path);
        }

        Ok(())
    }
}
