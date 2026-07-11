---
labels: architecture-review, adversarial
---
**Finding**: The Persistent Lock Retry Loop lacks graceful degradation.
**Current Assumption**: A 10-attempt, 500ms sleep retry loop is sufficient to resolve lock contention for SurrealKV and RocksDB engines.
**Attack Scenario**: A malicious actor or misconfigured test rapidly spawns daemon processes, holding locks longer than 5 seconds.
**Blast Radius**: Total system lockout. The daemon fails to start, preventing all agents from accessing memory or inference capabilities.
**Recommended Structural Change**: Implement a non-blocking or distributed lock manager, or fallback to an in-memory ephemeral state when persistent locks are permanently unavailable.
---
labels: architecture-review, adversarial
---
**Finding**: Pre-Compaction Hook Verbatim Ingestion enables latent prompt injection.
**Current Assumption**: Transcripts are safe to parse and ingest verbatim into episodic memory because they originated from a trusted agent session.
**Attack Scenario**: An agent interacts with external, adversarial input (e.g., a poisoned PR comment). This malicious payload is ingested verbatim into episodic memory.
**Blast Radius**: When the "dreaming" compactor runs or another agent queries memory, the adversarial payload is retrieved and executed, leading to unbounded recursive actions or data exfiltration.
**Recommended Structural Change**: Enforce strict boundary sanitization on tool outputs and user text before indexing, and implement prompt-injection classification scoring on ingestion.
---
labels: architecture-review, adversarial
---
**Finding**: The SWE-bench Verified Eval Framework lacks adversarial testing.
**Current Assumption**: Passing standard happy-path coding tasks in `princeton-nlp/SWE-bench_Verified` demonstrates architectural readiness and robustness.
**Attack Scenario**: The system is deployed into a production environment where it encounters edge cases, poisoned repositories, or malformed data formats not present in SWE-bench.
**Blast Radius**: The AI system behaves unpredictably or fails catastrophically under adversarial loads, revealing a fundamental disconnect between eval metrics and real-world resilience.
**Recommended Structural Change**: Integrate adversarial robustness datasets into the eval pipeline (e.g., prompt injection benchmarks, intentionally obfuscated codebases) and require a minimum pass rate before deployment.
---
labels: architecture-review, adversarial
---
**Finding**: Model Broker routing logic is tightly coupled to a specific local HTTP server configuration.
**Current Assumption**: Large hybrid models will always route directly to a local `mlx-lm` HTTP completions server on port 8080.
**Attack Scenario**: A deployment requires shifting to an external VLLM cluster, or scaling horizontally across multiple machines.
**Blast Radius**: Re-architecting the routing logic requires modifying the Model Broker source code directly, preventing independent scaling and testing of inference endpoints.
**Recommended Structural Change**: Decouple routing rules into a dynamic configuration layer, using abstracted endpoint interfaces that support dynamic port assignment and multiple backend types.
---
labels: architecture-review, adversarial
---
**Finding**: Daily DBSCAN clustering of episodic memory will become a scaling bottleneck.
**Current Assumption**: Running epsilon-calibrated DBSCAN clustering over the entire uncompacted episodic memory corpus during a daily "dreaming" cycle is computationally viable.
**Attack Scenario**: As the system scales 10x over 18 months, the volume of episodic memory grows exponentially, causing the daily compaction cycle to exceed 24 hours.
**Blast Radius**: Compaction fails to complete, causing memory usage to spiral, retrieval to slow down, and eventual system out-of-memory or persistent lock contention crashes.
**Recommended Structural Change**: Transition from batch DBSCAN clustering to incremental clustering algorithms (e.g., BIRCH or streaming DBSCAN) and partition memory clustering by project or temporal boundaries.
