---
title: "18-Month Scaling Bottlenecks (Top 3 Decisions)"
labels: ["architecture-review", "adversarial"]
---

Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change

**Finding**: Three design decisions will mandate re-architecture at 10x scale: 1) O(1) String Hash lookups in inner loops, 2) Hardcoded symbol limit of (idx + 15) truncating code context, 3) Single-port monolithic API gateway.
**Current Assumption**: Data volumes and context windows will remain within small single-user limits.
**Attack Scenario**: A collaborative workspace ingests thousands of complex documents. The system exhausts memory, truncates critical code context, and hits gateway starvation limits.
**Blast Radius**: Cascading daemon lockups and severe context hallucination.
**Recommended Structural Change**: Re-architect graph algorithms to integer-based arrays, eliminate arbitrary hardcoded line limits, and decompose the gateway for multi-tenant scalability.

**Note:** Never close this issue without a documented architectural decision record (ADR) response.
