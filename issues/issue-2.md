---
title: "Adversarial Architecture Review: Tight Coupling of In-Process Inference and Database"
labels: architecture-review, adversarial
---

**Finding:** The MLX In-Process Engine loads models natively into the daemon's process memory and is tightly coupled with the persistent storage engine (SurrealDB/SQLite).
**Current Assumption:** The MLX C++ bindings and Metal GPU operations are safe, and the daemon can gracefully recover from VRAM exhaustion or evaluation errors.
**Attack Scenario:** An adversarial workload or massive context ingestion triggers an out-of-bounds memory access, OOM, or unhandled panic in the MLX C++ bindings.
**Blast Radius:** The entire daemon crashes immediately. Active SurrealDB/SQLite transactions may be interrupted (despite RAII scope guards, a hard C++ panic bypasses Rust unwinding), causing cognitive pipeline state corruption and dropping active connections for all clients. Single point of failure with no graceful degradation.
**Recommended Structural Change:** Decouple inference from storage. Move the In-Process MLX Engine to a separate isolated process communicating over gRPC or IPC. The core daemon must only handle routing and DB transactions, ensuring inference crashes do not take down the control plane.

Never close without a documented architectural decision record (ADR) response.
