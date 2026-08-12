---
title: "Architecture Liability: Scaling Tight Coupling"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> SurrealKV and the Model Broker are tightly coupled into the single Core Daemon binary.

**Current Assumption** -> The system will remain primarily a local-first single-node deployment.

**Attack Scenario** -> 18 months from now at 10x scale, separating state (SurrealKV) from compute (Model Broker) becomes impossible without a complete rewrite, meaning the entire daemon must scale vertically instead of horizontally.

**Blast Radius** -> Severe scalability limitations, requiring a massive re-architecture to split storage and inference into independent, horizontally scalable services.

**Recommended Structural Change** -> Decouple SurrealKV and the Model Broker via internal gRPC/messaging boundaries now, allowing them to be deployed as separate services in the future.

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
