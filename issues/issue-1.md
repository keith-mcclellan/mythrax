# Single-Port API Gateway lacks graceful degradation (Single Point of Failure)

**Tags:** `architecture-review`, `adversarial`
**Note:** Never close this issue without a documented architectural decision record (ADR) response.

**Finding** -> Single-Port API Gateway lacks graceful degradation
**Current Assumption** -> The unified gateway handles REST, MCP, and OpenAI API on port 8090/8080 and delegates traffic efficiently under load.
**Attack Scenario** -> An adversary or runaway agent triggers a flood of complex MCP tool requests or malicious completions. The connection pool or tokio workers become exhausted, causing the single gateway to hang.
**Blast Radius** -> Complete system outage. All agent integrations, database reads, model routing, and cognitive synthesis fail simultaneously as they share the same ingress point.
**Recommended Structural Change** -> Decouple the control plane (admin/telemetry) from the data plane (LLM completions/MCP). Introduce rate limiting, load shedding, and dedicated circuit breakers per endpoint to ensure graceful degradation.
