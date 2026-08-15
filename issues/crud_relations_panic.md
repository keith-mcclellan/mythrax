---
labels: bug, agent-found
---

# Panic Risk in `save_episodes_batch_db` on Malformed Relations

## File and Line
`mythrax-core/src/db/crud_operations.rs:517`

## Minimal Reproducible Scenario
The function `save_episodes_batch_db` processes a `relations: &[serde_json::Value]` slice and extracts `from_str` and `to_str`:
```rust
for rel in relations {
    let from_uuid = rel.get("from_str").unwrap().as_str().unwrap();
    let to_uuid = rel.get("to_str").unwrap().as_str().unwrap();
    // ...
}
```
If an API client or an internal process calls `save_episodes_batch_db` with a JSON relations object that is missing the `"from_str"` key, the `"to_str"` key, or if either key is not a string, `.unwrap()` will panic, causing a Denial of Service for the application or async task. The codebase requires defensive parsing for external JSON data to avoid panics.

## Severity
High (Potential DoS on malformed input)

## Suggested Fix
Use defensive parsing with `and_then` or `if let`:
```rust
for rel in relations {
    if let (Some(from_val), Some(to_val)) = (rel.get("from_str"), rel.get("to_str")) {
        if let (Some(from_uuid), Some(to_uuid)) = (from_val.as_str(), to_val.as_str()) {
            let from_thing = parse_record_id(&format!("episode:{}", from_uuid));
            let to_thing = parse_record_id(&format!("episode:{}", to_uuid));

            if let (Ok(from), Ok(to)) = (from_thing, to_thing) {
                // ...
            }
        }
    }
}
```
