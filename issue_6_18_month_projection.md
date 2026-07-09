# 💥 18-Month Scaling Projection: Top 3 Impending Architectural Rewrites

**Tags:** `architecture-review`, `adversarial`

**Requires ADR response to close.**

**Finding:**
Projecting 18 months forward to a 10x scale (10x larger context, 10x concurrent agents, 10x database size), the current Mythrax architecture will buckle under three specific design decisions made today.

**Current Assumption:**
The architecture assumes single-user, local-first workloads with bounded project scopes and synchronous background compaction cycles.

*What assumption does this break if it's wrong?* It assumes scale only grows linearly and that local SSD I/O and Apple Silicon VRAM are sufficient to handle compounded memory graphs.

**Attack Scenario / Scaling Breakdown:**

1. **The DBSCAN/RAPTOR Compaction Bottleneck:**
   *Current Decision:* Daily "dreaming" cycle runs DBSCAN clustering in the background on the local machine.
   *10x Scale Breakdown:* As episodic memory grows, DBSCAN's $O(n^2)$ complexity will stall the daemon. At 10x scale, the nightly compaction will take longer than 24 hours to complete, meaning the database will infinitely expand until local disk is exhausted.
   *Rewrite:* Migration from local DBSCAN to a continuous, streaming approximate-nearest-neighbor (ANN) graph partitioning algorithm, likely offloaded to a dedicated cloud batch-processing tier.

2. **In-Process RAG Sigmoid Gating (Flow 4):**
   *Current Decision:* Blended search matches vectors via `nomic-embed` in-process and applies a strict Sigmoid gating filter locally.
   *10x Scale Breakdown:* Scanning a 10x larger SurrealDB vector space in-process will block the main tokio reactor thread, creating massive P99 latency spikes for simple agent HTTP requests.
   *Rewrite:* Ripping out SurrealDB in favor of a specialized, distributed vector database (e.g., Milvus, Qdrant) that handles similarity gating natively at the query level, rather than pulling rows into Rust memory to apply a sigmoid function.

3. **Flat JSONL Transcript Mining (Flow 5):**
   *Current Decision:* The pre-compaction hook parses flat JSONL transcripts line-by-line to extract raw text and tool results.
   *10x Scale Breakdown:* As context windows expand to 2M+ tokens, parsing massive, multi-gigabyte JSONL files line-by-line for "trailing turns" will cause catastrophic I/O blocking and extreme RAM spikes.
   *Rewrite:* Abandoning flat JSONL files in favor of a granular event-sourcing stream (e.g., Kafka or a native Rust append-only log) where agents stream structured diffs/deltas, eliminating the need to "mine" large files.

**Recommended Structural Change:**
Begin architecting the interfaces for DBSCAN, Vector Retrieval, and Telemetry Ingestion to be purely asynchronous and interface-driven today, allowing swap-outs to distributed systems without rewriting the core orchestration logic.
