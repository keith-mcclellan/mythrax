---
title: "Bug: Potential string slicing panic in unescape_id_part"
labels: ["bug", "agent-found"]
---

## Description
In `mythrax-core/src/db/backend.rs` inside the `unescape_id_part` function, string slicing is used manually to strip suffix characters:

```rust
while s.ends_with('⟩') {
    s = &s[..s.len() - '⟩'.len_utf8()];
}
```

## Reproducible Scenario
The current slicing is technically safe in Rust if `s.ends_with()` returned true right beforehand (because the boundary `s.len() - len_utf8` will perfectly align with the character). However, manually calculating byte boundaries and slicing a string is error-prone to refactoring and is generally considered technical debt that could hide off-by-one errors if the checking logic ever decouples from the slicing logic.

More critically, in loop processing, using `s.strip_suffix('⟩')` is much safer and canonical.

## Severity
**Low/Medium** - Works currently but is fragile.

## Suggested Fix
Use the built-in `strip_suffix` and `strip_prefix` string methods which handle unicode boundaries safely:

```rust
while let Some(stripped) = s.strip_prefix('⟨') {
    s = stripped;
}
while let Some(stripped) = s.strip_suffix('⟩') {
    s = stripped;
}
```
