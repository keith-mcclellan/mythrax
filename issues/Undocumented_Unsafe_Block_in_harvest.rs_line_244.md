---
labels: bug, agent-found, security-audit
title: Undocumented Unsafe Block in harvest.rs line 244
---

**File**: mythrax-core/src/cognitive/harvest.rs:244
**Snippet**:
```rust
unsafe {
```
**Severity**: High
**Remediation**: Analyze the memory safety risk of this block and document the justification with a `// SAFETY:` comment, or refactor to safe Rust.