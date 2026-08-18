---
labels: architecture-review, adversarial
state: open
---
# Red Team Architecture Brief: Single Point of Failure in Unified Daemon Gateway

**Finding** -> The Mythrax Core Daemon (Port 8090) centralizes all operations (HTTP Gateway, Model Broker, Database, Background Loops) into a single monolithic process.
**Current Assumption** -> The daemon is robust enough to handle simultaneous database I/O, VRAM eviction, and HTTP proxying without crashing or starvation.
**Attack Scenario** -> An adversary or runaway script sends a massive burst of requests, triggering simultaneous model swapping and vector embeddings. The tokio runtime blocks, memory spikes, and the daemon panics.
**Blast Radius** -> Complete system outage. All agents, CLI tools, and MCP clients instantly lose access to memory and inference.
**Recommended Structural Change** -> Decouple the Unified Gateway, Model Broker, and Storage backend into separate processes communicating via gRPC or message queues. Must require an ADR response to close.
