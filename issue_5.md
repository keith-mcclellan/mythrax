---
title: "🛡️ Red Team Architecture Brief: 18-Month 10x Scaling Liabilities"
labels: ["architecture-review", "adversarial"]
---

## Red Team Architecture Brief

**Finding:**
Projecting the Mythrax 2.0 architecture 18 months forward under a 10x scaling scenario reveals three core design decisions that will inevitably mandate full re-architecture:
1. **Local Embedded Databases with Exclusive Locks:** Relying on `rocksdb://` and `surrealkv://` with blocking file locks.
2. **The 10-Attempt/500ms Retry Loop:** Using a naive sleep-and-retry mechanism to handle concurrent connection contention.
3. **Hardcoded Configurations and Paths:** Embedding production configurations (e.g., auth tokens, 10-minute sweep thresholds, `/Users/keith/mythrax-vault` absolute paths) directly in the Rust binary.

**Current Assumption:**
The system assumes it will only ever operate as a localized, single-tenant, lightly-concurrent sidecar daemon on a single developer's machine. It assumes that configuration will rarely change dynamically and that database contention will be infrequent enough that a simple 5-second backoff window will mask any overlapping lock requests.

**Attack Scenario:**
As the platform scales to host fleets of agents or is deployed in a cloud-native, multi-tenant environment (10x scale), the following occurs:
- Concurrent agent orchestration requests immediately overwhelm the 10x500ms database retry loop, leading to cascading initialization failures and timeouts across the fleet.
- The hardcoded absolute paths (e.g., `/Users/keith/mythrax-vault`) prevent deployment on standard Linux containers or diverse host systems, breaking CI/CD pipelines and production rollouts.
- Embedded auth tokens become an administrative nightmare, making credential rotation impossible without recompiling and redeploying the entire core daemon binary across all nodes.

**Blast Radius:**
System-wide architectural collapse under scale. The current design simply cannot support distributed, high-concurrency, or multi-tenant agent execution. Scaling 10x will result in persistent deadlocks, deployment failures on non-macOS hardware, and severe security compliance violations due to un-rotatable secrets.

**Recommended Structural Change:**
1. **Migrate to a Client/Server Database Model:** Replace embedded RocksDB/SurrealKV with a networked database backend (e.g., PostgreSQL with pgvector, or standalone SurrealDB servers) to handle concurrent connections natively without exclusive local file locks.
2. **Eliminate the Retry Loop:** Implement a proper connection pool (e.g., `bb8` or `deadpool` in Rust) or an asynchronous message queue to handle concurrent database requests gracefully without blocking threads.
3. **Externalize Configuration:** Decouple all hardcoded secrets, paths, and tuning parameters into environment variables or a dynamic configuration service (e.g., HashiCorp Vault), completely removing them from the compiled binary.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*