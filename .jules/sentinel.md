## 2026-06-27 - [Fix Shell Injection Vulnerability in Arbor HTR]
**Vulnerability:** Shell injection vulnerability via raw POSIX shell invocation (`sh -c`) in `arbor.rs` and `executor.rs` for dynamically constructed test commands.
**Learning:** The fallback to `sh -c` execution path allows adversarial manipulation of shell operators when commands are constructed dynamically, compromising agent isolation boundaries.
**Prevention:** Always enforce direct argument execution (`std::process::Command::new(program).args(args)`) and prohibit shell interpretation for dynamic commands.
