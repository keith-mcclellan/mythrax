---
labels: architecture-review, adversarial
state: open
---
# Red Team Architecture Brief: Tight Coupling Between Storage and Cognitive Pipeline

**Finding** -> The streaming-to-disk pipeline and the AST Symbol Indexing are tightly coupled with `SurrealDB` and `Obsidian Vault` structures.
**Current Assumption** -> Mythrax will always run locally with Obsidian as the primary UI and SurrealDB as the primary datastore.
**Attack Scenario** -> A 10x scale enterprise deployment requires a cloud-hosted Vector DB and headless operation without an Obsidian filesystem.
**Blast Radius** -> A massive re-architecture project to decouple the hardcoded SurrealDB queries, Obsidian file watchers, and `MOC.md` sync logic.
**Recommended Structural Change** -> Abstract the storage layer and vector indexing behind standardized interfaces (Traits). Extract Obsidian syncing into an optional external plugin rather than core daemon logic. Must require an ADR response to close.
