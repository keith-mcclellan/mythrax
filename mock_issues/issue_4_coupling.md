---
labels: architecture-review, adversarial
---
**Finding**: Model Broker routing logic is tightly coupled to a specific local HTTP server configuration.
**Current Assumption**: Large hybrid models will always route directly to a local `mlx-lm` HTTP completions server on port 8080.
**Attack Scenario**: A deployment requires shifting to an external VLLM cluster, or scaling horizontally across multiple machines.
**Blast Radius**: Re-architecting the routing logic requires modifying the Model Broker source code directly, preventing independent scaling and testing of inference endpoints.
**Recommended Structural Change**: Decouple routing rules into a dynamic configuration layer, using abstracted endpoint interfaces that support dynamic port assignment and multiple backend types.
