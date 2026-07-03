---
title: "Bug: Panic on missing local embeddings"
labels: ["bug", "agent-found"]
---

### Vulnerability / Bug Description
In the fallback handling for batch embeddings, `results.into_iter().map(|opt| opt.unwrap()).collect()` is used assuming all items in a batch successfully generated an embedding. If logic changes or a caching issue leads to a missing embedding (a `None` in the `results` Vec), this causes a panic in the central embedding utility.

### File and Line Number
- `mythrax-core/src/embeddings.rs`, lines 298 and 558.

### Minimal Reproducible Scenario
1. Submit an embedding batch where one or more items somehow fail the internal chunk processing (or cache fetch returns `None` and local generation fails silently).
2. The `results` array has missing elements.
3. The `.unwrap()` map triggers a system panic instead of propagating an Error.

### Severity
**Medium**. Directly crashes the vector database indexing pipeline on malformed inputs or local model errors.

### Suggested Fix
Iterate safely through the `results` Vec and `return Err(anyhow::anyhow!("..."))` if a `None` value is encountered, gracefully failing the transaction rather than panicking.
