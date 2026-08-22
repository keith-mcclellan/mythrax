# Adversarial Architecture Review: Tight coupling between `daemon.rs` and `db/backend.rs`

Labels: architecture-review, adversarial

**Finding**: There is tight coupling between the API gateway/daemon layer (`daemon.rs`) and the database backend (`db/backend.rs`).

**Current Assumption**: The database backend and API gateway will always be deployed together and operate under the same runtime environment.

**Attack Scenario**: If the database needs to be scaled independently or swapped (e.g., moving from SurrealKV to a distributed cluster), the tight coupling prevents modular replacement. An attacker could exploit an issue in the API gateway to crash the shared runtime, taking down the entire database backend simultaneously.

**Blast Radius**: System-wide outage due to cascading failure; inability to scale components independently.

**Recommended Structural Change**: Introduce a defined interface (trait) for the database backend and inject it into the daemon layer. This allows for independent testing, mocking, and deployment.