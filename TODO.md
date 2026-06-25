# Mythrax Performance and Architecture TODOs

This document tracks planned architectural improvements inspired by advanced pipeline design patterns.

## 1. Asynchronous Embedding Execution
Offload CPU-bound ONNX embedding inference to prevent blocking the async runtime (Tokio executor threads).
- [ ] Implement `tokio::task::spawn_blocking` inside `DatabaseBackend` methods that invoke `LocalEmbedder::embed` (specifically `save_episode`, `search`, `save_hypothesis_node`, etc.).
- [ ] Add error handling and context propagation for the spawned tasks.
- [ ] Add integration tests in `tests/` to verify that concurrent database operations do not cause thread starvation or latency spikes.

## 2. Shared Resource Lifecycle Management
Eliminate ad-hoc initialization of the `LocalEmbedder` to reduce memory and CPU overhead.
- [ ] Refactor `mythrax-core/src/cognitive/synthesis.rs` to accept a shared `Arc<LocalEmbedder>` instead of calling `LocalEmbedder::new().ok()` inside loops.
- [ ] Ensure `LocalEmbedder` is initialized once during daemon startup and shared across all active cognitive modules.
- [ ] Update `mythrax-core/src/cognitive/compactor.rs` and other modules to utilize the same shared reference.

## 3. Batch Embedding Support
Leverage parallel execution in ONNX Runtime for multi-document operations.
- [ ] Implement `embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>` in `LocalEmbedder`.
- [ ] Update the tokenization logic in `embeddings.rs` to handle batching and padding.
- [ ] Refactor synthesis and compaction loops to use batch embeddings instead of sequential single-string embedding calls.

## 4. RAG and Conversational Memory Architecture
Adapt modular, type-safe, and structured memory patterns to improve context retrieval and reasoning.
- [ ] Define a modular `MemoryComponent` trait in Rust to represent individual nodes in a memory retrieval pipeline (e.g., retrieval, compaction, symbol injection).
- [ ] Implement a first-class `chat_history` table in SurrealDB to store conversational thread turns.
- [ ] Create an automated sliding window retriever to fetch the `last_k` messages of the active session and inject them cleanly into the prompt context.
- [ ] Refactor MCP route handlers to use strongly-typed argument structs instead of generic `serde_json::Value` objects, adding strict schema validation.
- [ ] Implement a bi-level hybrid prompt builder that unifies immediate conversational history, active code symbols, and long-term semantic memories.

## 5. Additional Pipeline and Ingestion Patterns
Adapt advanced document ingestion, declarative configuration, evaluation, and caching mechanisms.
- [ ] Refactor the "Forge" ingestion pipeline to use granular, semantic document splitting (e.g., 1,000–2,000 token chunks) instead of the monolithic 24,000-token windows, and extend this splitting strategy to long-term episodes and other large artifacts (e.g., handoffs, code files) to optimize vector search precision and reduce local model extraction latency.
- [ ] Implement a declarative YAML configuration system (`mythrax.yaml`) to allow users to configure cognitive thresholds, token budgets, and model endpoints without recompiling the Rust codebase.
- [ ] Develop a local, automated RAG evaluation harness to quantitatively measure retrieval accuracy and compaction faithfulness (hallucination checks) using Phi-3-mini (3.8B, ~2.2 GB) as the local judge model instead of the heavier primary model.
- [ ] Introduce a caching and memoization layer for cognitive tasks (compaction and synthesis) to skip expensive local LLM runs when source episodes or database records have not changed.


