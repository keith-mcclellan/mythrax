---
labels: architecture-review, adversarial
---
**Finding**: Pre-Compaction Hook Verbatim Ingestion enables latent prompt injection.
**Current Assumption**: Transcripts are safe to parse and ingest verbatim into episodic memory because they originated from a trusted agent session.
**Attack Scenario**: An agent interacts with external, adversarial input (e.g., a poisoned PR comment). This malicious payload is ingested verbatim into episodic memory.
**Blast Radius**: When the "dreaming" compactor runs or another agent queries memory, the adversarial payload is retrieved and executed, leading to unbounded recursive actions or data exfiltration.
**Recommended Structural Change**: Enforce strict boundary sanitization on tool outputs and user text before indexing, and implement prompt-injection classification scoring on ingestion.
