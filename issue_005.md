---
title: "Architecture Review: Severe Coupling of Storage and Inference Domains"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:** There is high structural coupling between the storage backend (`db/backend.rs`) and the machine learning inference layer (`llm/` and `embeddings::LocalEmbedder`). The database struct (`SurrealBackend`) directly handles generating embeddings (`self.embed_batch`, `self.embed`) during transaction processing.

**Current Assumption:** The system assumes that local embedding generation is fast and reliable enough to happen synchronously during database writes, and that monolithic deployment of DB and ML models on the same node is the permanent architecture.

**Attack Scenario:** A surge of episode ingestions or file watcher coalescing events triggers a massive batch of embedding requests. Because embedding generation is directly tied to the database transaction lifecycle (`run_write!`), the database lock is held while waiting for the GPU to process the embeddings (or for the `METAL_EMBEDDING_SEMAPHORE` to clear). An attacker can intentionally trigger file modifications to exhaust the embedding queue, bringing the entire database (and thus the daemon) to a complete halt.

**Blast Radius:** Complete system deadlock. Neither storage nor retrieval can function because the database is blocked waiting on inference resources. The tight coupling prevents deploying the database and the embedding models on separate hardware, crippling horizontal scalability.

**Recommended Structural Change:**
- Decouple `StorageBackend` from `LocalEmbedder`.
- Storage should only accept pre-computed vectors. Implement an asynchronous ingestion pipeline (a message queue or event bus) that handles text extraction and embedding generation before persisting to the database.
- Ensure the database layer is entirely agnostic to how vectors are generated, allowing the ML broker to be independently deployed or scaled.

*ADR required to close this issue.*