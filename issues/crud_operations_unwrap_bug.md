---
title: Panic in crud_operations.rs on malformed temporal relations
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/crud_operations.rs`
**Line:** 517-518

**Description:**
The code uses `.unwrap()` when extracting and parsing `from_str` and `to_str` from `rel` (a JSON object/map). If the relation map is missing these keys or if the values are not strings, this will panic and crash the daemon.

**Minimal Reproducible Scenario:**
Submit a request to the database layer to create temporal relations where the relation map is malformed, specifically missing the `from_str` or `to_str` keys, or providing values that are not strings. The daemon will panic on lines 517 or 518 of `crud_operations.rs`.

**Severity:** High (Crash)

**Suggested Fix:**
Check for the existence of the keys and ensure they are strings using `if let` or `match` blocks, or propagate the error gracefully.

```rust
for rel in relations {
    if let (Some(from_val), Some(to_val)) = (rel.get("from_str"), rel.get("to_str")) {
        if let (Some(from_uuid), Some(to_uuid)) = (from_val.as_str(), to_val.as_str()) {
            let from_thing = parse_record_id(&format!("episode:{}", from_uuid));
            let to_thing = parse_record_id(&format!("episode:{}", to_uuid));

            if let (Ok(from), Ok(to)) = (from_thing, to_thing) {
                // ... relate query ...
            }
        }
    }
}
```
