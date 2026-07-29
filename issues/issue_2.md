---
title: Panic on missing BM25 doc_id in doc_term_freqs
labels: bug, agent-found
severity: Medium
---

**File/Line:** `mythrax-core/src/retrieval/bm25.rs` : 111

**Minimal Reproducible Scenario:**
`self.doc_term_freqs.get(doc_id).unwrap()` will panic if a document is present in `doc_lengths` but missing from `doc_term_freqs`. While they should conceptually be in sync, any desync due to concurrent updates or corrupted state will cause a hard panic during search retrieval.

**Suggested Fix:**
Replace `.unwrap()` with proper safe fallback logic:
```rust
let term_freqs = match self.doc_term_freqs.get(doc_id) {
    Some(freqs) => freqs,
    None => continue,
};
```
