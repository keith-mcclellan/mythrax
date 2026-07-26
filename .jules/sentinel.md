## 2026-06-27 - Command Injection in Git Worktrees
**Vulnerability:** Command injection via `sh -c` invocation for dynamic test commands (`test_command`) in HTR git worktree execution (`cognitive/executor.rs`).
**Learning:** The fallback to `sh -c` to support shell operators (`&`, `|`, `>`, `<`, `;`) allows arbitrary execution and breaks agent isolation boundaries if the `test_command` is maliciously crafted or unsanitized.
**Prevention:** Avoid running dynamically constructed commands in git worktrees using raw POSIX shell invocations. Always enforce direct argument execution using `std::process::Command::new(program).args(args)` and explicitly deny strings containing shell operators if shell interpretation is not required.
