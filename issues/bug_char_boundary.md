# Bug: String truncation causes panic due to slicing inside multi-byte character boundary

**Labels:** bug, agent-found

**File/Line:** `mythrax-core/src/mcp_routes/read_handlers.rs`, line 277 and `mythrax-core/src/mcp_routes/manage_handlers.rs`, lines 101, 684

**Minimal Reproducible Scenario:**
In `read_handlers.rs` and `manage_handlers.rs`, a dynamically computed index (`truncate_idx` or `STM_VALUE_MAX_CHARS` which is 32000) is used to truncate a string slice directly. If this index falls inside a multi-byte UTF-8 character (like an emoji or foreign language character), Rust will immediately panic, taking down the request handler thread or process.

**Severity:** High

**Suggested Fix:**
Implement a safety loop that decrements the truncation index until it falls on a valid character boundary before executing the slice/truncate operation.
Example:
```rust
while truncate_idx > 0 && !content_slice.is_char_boundary(truncate_idx) {
    truncate_idx -= 1;
}
```