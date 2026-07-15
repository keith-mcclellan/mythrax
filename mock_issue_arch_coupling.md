# Finding: Core Daemon is Tightly Coupled to Model Broker FFI

**Current Assumption:**
"Lightweight dense models... are loaded natively into the Rust process memory and run in-process using the Metal GPU backend." (ARCHITECTURE.md). It is assumed that running inference in-process provides latency benefits without compromising stability.

**Attack Scenario:**
A crash, panic, or memory violation in the MLX/ORT execution occurring natively in the Rust process immediately crashes the Rust host daemon. This brings down the API Gateway (port 8090) and the database watcher simultaneously. The daemon and broker cannot be independently deployed, scaled, or replaced without modifying both.

**Blast Radius:**
Inference failure causes a complete memory database and gateway outage.

**Recommended Structural Change:**
Move the in-process MLX/ORT engine to a separate gRPC sidecar process. The Daemon should communicate with the local broker over IPC/gRPC, matching the external 8080 proxy pattern, to isolate faults and allow independent restarts and scaling.