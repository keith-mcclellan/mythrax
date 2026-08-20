---
labels: bug, agent-found, security-audit
title: Undocumented Unsafe Block in store.rs line 15
---

**File**: mythrax-core/src/store.rs:15
**Snippet**:
```rust
unsafe { std::env::set_var("MYTHRAX_VAULT_ROOT", &root); }
```
**Severity**: High
**Remediation**: Analyze the memory safety risk of this block and document the justification with a `// SAFETY:` comment, or refactor to safe Rust.