# Product Guidelines

## Developer Experience (DX) Principles
- **Zero-CLI Autonomy:** The daemon should run invisibly and robustly in the background, minimizing manual developer intervention.
- **Fail-Safe & Graceful Degradation:** Features (like LLM synthesis) must fall back gracefully (e.g., local to cloud) and never crash the core memory retrieval loop.
- **Transparency via Telemetry:** Rely on structured `tracing` (info/warn/error) for logging instead of `println!`, ensuring observability without terminal noise.

## Voice and Tone (API & Error Messaging)
- **Direct and Actionable:** Error messages must pinpoint the failure and provide the immediate remedy (e.g., "GPU OOM detected: Falling back to cloud broker").
- **Concise:** Avoid throat-clearing in documentation and API responses. Deliver the payload immediately.
- **Authoritative but Safe:** Execute background tasks (like compaction and pruning) assertively, but never destructively alter human-verified wisdom without versioning or provenance.
