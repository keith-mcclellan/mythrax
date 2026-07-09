# 💥 Architectural Liability: Extreme Coupling of Daemon and MLX Engine

**Tags:** `architecture-review`, `adversarial`

**Requires ADR response to close.**

**Finding:**
There is deep, inseparable coupling between the persistent Mythrax Core Daemon and the `mlx` / Model Broker inference tier. They cannot be independently deployed, scaled, or tested without modifying both.

**Current Assumption:**
*Architecture.md* specifies a "Hybrid In-Process vs External Routing" where lightweight dense models (like Nomics embeddings) are "loaded natively into the Rust process memory and run in-process using the Metal GPU backend."

*What assumption does this break if it's wrong?* It assumes that a stateful, long-running database/memory daemon should share the exact same process space and crash domain as highly volatile, GPU-memory-intensive, FFI-bound C++/Metal MLX routines.

**Attack Scenario:**
A memory compaction job triggers the in-process Nomics embedding model (`mlx_rs`). Due to a zero-day in the MLX Metal bindings, a memory leak, or a temporary VRAM spike, the embedding process triggers a segmentation fault or a strict macOS kernel OOM kill. Because the database WAL actor, the memory ingest watcher, and the API gateway share the same process, the entire daemon crashes instantly. Writes are lost before hitting the WAL.

**Blast Radius:**
A failure in the stateless inference tier instantly kills the stateful data tier. The daemon cannot be deployed on a cheap CPU server and remotely call a GPU cluster, because the embedding engine is hard-coupled via compiler feature flags (`#[cfg(feature = "mlx")]`) scattered directly through `mythrax-core/src/llm/mod.rs`.

**Recommended Structural Change:**
Decouple the Core Daemon from the Inference Engine.
1. Extract all `mlx` dependencies and GPU logic out of the main `mythrax-core` daemon.
2. Move the Model Broker to a separate, standalone `mythrax-inference-worker` binary.
3. Communicate between the Core Daemon and the Inference Worker entirely via gRPC or HTTP (already used for `mlx-lm`). This allows the daemon to remain stable even if the MLX worker crashes, and allows independent scaling.
