## 2024-08-11 - Block Command Substitution Injection in Arbor test execution

**Vulnerability:** The test command execution engine in `arbor.rs` and `executor.rs` passes user-supplied or AI-generated strings directly to `sh -c` to support functional shell command chaining (`&, |, >, <, ;`). This creates a critical command injection vector, as malicious actors (or adversarial prompts) could embed command substitution syntax (e.g., `$(...)` or backticks) within the test command to execute arbitrary, unconstrained code on the host machine prior to or alongside the intended command execution.

**Learning:** When functional requirements demand the use of a system shell (`sh -c`) to preserve intended operators, naive sanitization (like blocking all shell metacharacters) breaks core functionality. Instead, we must perform surgical restrictions on sub-shell generation primitives. Command substitution is frequently overlooked as an injection vector when standard command separators (`;`) are explicitly allowed by design.

**Prevention:** Implement explicit pre-execution validation to reject test commands containing `$(` or backticks `\``. This restricts the shell's capability to spawn sub-shells and execute embedded arbitrary commands, preserving intended chaining functionality while significantly raising the barrier for command injection.
