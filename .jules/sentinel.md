## 2026-06-27 - Shell Injection in Command Execution

**Vulnerability:** The code used a fallback to `sh -c` to execute dynamically constructed test commands (in `arbor.rs` and `executor.rs`) when it detected shell operators like `&`, `|`, `<`, `>`. This created a severe shell injection vulnerability as the command strings contained unsanitized user or LLM input.
**Learning:** Avoid using raw POSIX shell invocations (`sh -c`) for dynamically constructed test commands. It completely breaks the isolation boundaries.
**Prevention:** Always use direct argument parsing and execution with `std::process::Command::new(program).args(args)` instead of passing an entire string to `sh -c`. Avoid relying on shell operators inside dynamically generated commands.
