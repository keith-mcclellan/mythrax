# 🛡️ Sentinel: [CRITICAL] Fix Shell Injection in Arbor HTR Evaluator

## 🚨 Severity
CRITICAL

## 💡 Vulnerability
The `TestCommandEvaluator` in `mythrax-core/src/cognitive/arbor.rs` executed test commands using a raw POSIX shell invocation (`sh -c`) when it detected shell operators like `&`, `|`, `>`, `<`, or `;`. This allowed untrusted commands (e.g., LLM-generated inputs) to break agent isolation boundaries through shell injection.

## 🎯 Impact
If exploited, an attacker or a compromised AI agent could execute arbitrary system commands on the host machine or within the sandbox, leading to complete system compromise, data exfiltration, or denial of service.

## 🔧 Fix
Replaced the `sh -c` logic with strict argument parsing using the `Command::new()` and `args()` API. This ensures that the test command is executed directly with its arguments safely escaped, preventing any shell injection capabilities. Added an in-code comment outlining the security risk for future reference.

## ✅ Verification
- Analyzed the source file `mythrax-core/src/cognitive/arbor.rs` to confirm the removal of `Command::new("sh").arg("-c")`.
- Validated that `Command::new(&args[0])` is securely invoked.
- Ran `cargo check` inside `mythrax-core` to ensure the library compiles without errors after the fix.
