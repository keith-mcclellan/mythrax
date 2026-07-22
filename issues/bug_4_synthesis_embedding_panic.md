---
labels: bug, agent-found
---

# Panic on `.unwrap()` when extracting embeddings in `synthesis.rs`

## Description
When grouping directional nodes, the code iterates over directions and blindly extracts their embeddings using `directions[i].embedding.as_ref().unwrap()`. If a node's embedding generation failed previously and the `embedding` field is `None`, this `.unwrap()` will panic and crash the compaction task.

## Location
- File: `mythrax-core/src/cognitive/synthesis.rs`
- Lines: 2286 and 2292

## Minimal Reproducible Scenario
1. Ingest or generate a `DirectionNode` (or `WikiNode` parsed as a direction) where embedding generation fails (e.g., due to model timeout).
2. Start the background compaction loop over the scope containing this direction.
3. The system begins iterating to cluster similar directions.
4. Line 2286 or 2292 is reached, and `.unwrap()` is called on `None`.
5. The thread panics.

## Severity
High - Missing embeddings can realistically occur in unstable environments or local generation loops, causing unhandled daemon crashes.

## Suggested Fix
Gracefully handle nodes without embeddings:
```rust
let emb_i = match directions[i].embedding.as_ref() {
    Some(e) => e,
    None => continue,
};
```
And similarly for `emb_j`.