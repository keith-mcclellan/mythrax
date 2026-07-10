# Architectural Liability: Tight Coupling Between Model Broker and Hardware Eviction

**Finding**: The Dynamic Model Broker (documented in `ARCHITECTURE.md` Section 3) tightly couples inference routing logic with hardware-specific Metal/ONNX execution and "VRAM Eviction & Sequential Swapping" management.

**Current Assumption**: The daemon is tightly bound to local Apple Silicon or specific consumer GPUs, so embedding hardware state management directly into the inference router simplifies the architecture and ensures models are aggressively evicted to prevent Out-Of-Memory (OOM) crashes.

**Attack Scenario**: The system attempts to run in a containerized, cloud, or non-Metal environment where VRAM is either managed dynamically by an external orchestrator or where eviction commands are invalid. Because the VRAM management logic is coupled directly to the inference path, the broker fails to route models properly or panics during the eviction cycle. As seen in `mock_audit_report.md` (MOCK-006), the system attempts to mask these failures by fabricating a "fallback-cpu-model", hiding the underlying coupling failure from operators.

**Blast Radius**: **Inability to Deploy Independently.** The Model Broker cannot be tested or deployed without a mock Metal context, and the eviction logic cannot be replaced with a different resource manager without modifying the core routing logic. This prevents migrating the inference engine to a dedicated microservice.

**Recommended Structural Change**: Introduce a strict `ResourceAllocator` trait interface. Decouple the `ModelBroker` (which should only handle routing decisions based on model tiers) from the `HardwareManager` (which handles VRAM eviction, Metal FFI cache clearing, and split semaphores).

Tags: `architecture-review`, `adversarial`
