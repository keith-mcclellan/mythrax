---
title: Integer underflow vulnerability in BM25 IDF update logic
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/crud_operations.rs`
**Line:** 1698

**Severity:** High (Database state corruption)

**Issue:**
The query `UPDATE idf_index SET document_frequency -= 1 WHERE term = $term AND scope = $scope;` blindly decrements the `document_frequency`. If a term's frequency falls out of sync or is already 0, this will cause an integer underflow or negative document frequency in the SurrealDB `idf_index`, leading to corrupted BM25 `idf` calculations (e.g. `NaN` or negative `ln` arguments) and permanently breaking retrieval.

**Minimal Reproducible Scenario:**
If an episode is updated or deleted twice, or if a term was not properly tracked during an initial batch ingestion but is present during deletion, the `document_frequency` will be decremented to a negative value.

**Suggested Fix:**
Add a condition to ensure `document_frequency` does not drop below 0.
For example: `UPDATE idf_index SET document_frequency = math::max(0, document_frequency - 1) WHERE term = $term AND scope = $scope;` (or whatever syntax SurrealDB uses for safe decrement/bounds checking, like adding a `WHERE document_frequency > 0`). Also, it's highly recommended to cleanup rows where `document_frequency == 0` to prevent database bloat.
