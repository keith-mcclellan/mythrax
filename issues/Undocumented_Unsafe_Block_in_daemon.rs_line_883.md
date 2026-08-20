---
labels: bug, agent-found, security-audit
title: Undocumented Unsafe Block in daemon.rs line 883
---

**File**: mythrax-core/src/daemon.rs:883
**Snippet**:
```rust
let res = unsafe { libc::statfs(c_path.as_ptr(), &mut buf) };
```
**Severity**: High
**Remediation**: Analyze the memory safety risk of this block and document the justification with a `// SAFETY:` comment, or refactor to safe Rust.