# Panic Path: Unsafe `.unwrap()` on matched.id when superseding rule

**Labels:** `bug`, `agent-found`
**Severity:** Medium

## Vulnerability Details
**File:** `mythrax-core/src/cognitive/synthesis.rs`
**Line:** 1637

In memory dreaming compaction, if an old rule is superseded by a new rule, the code strips the prefix from the ID using `matched.id.as_ref().unwrap()`. If `matched.id` is `None` (which is technically possible since `id` is an `Option<String>` in `WisdomRule`), this will panic.

**Code Snippet:**
```rust
let old_uuid = matched.id.as_ref().unwrap().strip_prefix("wisdom:").unwrap_or(matched.id.as_ref().unwrap());
```

## Minimal Reproducible Scenario
1. Force the `memory_os` compactor into the synthesis phase for wisdom rules.
2. Inject a `WisdomRule` struct directly into the database or processing pipeline where the `id` field is explicitly set to `None`.
3. Provide a new rule that triggers a successful semantic match and supersedes the old rule (similarity > 0.85).
4. The code reaches line 1637 and attempts to unwrap the `None` ID, resulting in a panic.

## Suggested Fix
Ensure `matched.id` is safe to unwrap by using `if let Some(id) = &matched.id`.
