---
title: "Bug: Panic in BM25 Scoring due to Missing Document Key"
labels: ["bug", "agent-found"]
severity: "High"
---

## Bug Description
In `mythrax-core/src/retrieval/bm25.rs`, the BM25 scoring algorithm iterates over all documents in `self.doc_lengths`. For each document, it fetches the corresponding term frequencies on line 111 using `.unwrap()`:
`let term_freqs = self.doc_term_freqs.get(doc_id).unwrap();`

If a document exists in `doc_lengths` but is missing from `doc_term_freqs` (e.g. if a document is inserted with length 0 and no terms, or if the maps become desynchronized during updates), this call panics, bringing down the entire retrieval subsystem.

## File & Line Number
`mythrax-core/src/retrieval/bm25.rs:111`

## Minimal Reproducible Scenario
1. Initialize the `BM25` struct with a `doc_lengths` map containing an entry `(doc_id="doc1", len=5)`.
2. Omit `doc1` from the `doc_term_freqs` map.
3. Call `.score("test query")`.
4. The system panics when attempting to retrieve `doc1`'s term frequencies.

## Suggested Fix
Replace `.unwrap()` with a safe `.get()` followed by error handling or a default empty map, such as:
`let term_freqs = self.doc_term_freqs.get(doc_id).unwrap_or(&empty_map);`
Or skip scoring for documents missing term frequency data.
