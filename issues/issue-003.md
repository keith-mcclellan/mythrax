---
labels: architecture-review, adversarial
---
# Finding: Tight Coupling Between Cognitive Pipeline and Vault Storage (I/O Bottleneck)

**Current Assumption:** Streaming cognitive artifacts directly to Obsidian Vault markdown files during memory synthesis is an acceptable side-effect of the cognitive pipeline.

**Attack Scenario:** The Cognitive Pipeline and the Vault Storage are tightly coupled. At 10x scale, thousands of concurrent agent sessions trigger micro-edits, causing severe write amplification and lock contention on the filesystem. The async executor stalls due to disk I/O.

**Blast Radius:** Complete pipeline stall; the core daemon becomes unresponsive to new user inputs. These modules cannot be independently deployed, tested, or replaced without modifying both—an architectural liability.

**Recommended Structural Change:** Decouple the cognitive pipeline from vault storage via an event bus or message queue. The cognitive engine should emit generic `InsightGenerated` events, which an independent `VaultSyncer` module consumes and batches to disk.

*Note: Never close this issue without a documented architectural decision record (ADR) response.*
