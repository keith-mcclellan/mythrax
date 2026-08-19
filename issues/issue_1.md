# API Gateway Single Point of Failure

**Finding** -> The Single-Port API Gateway multiplexes REST, MCP, and OpenAI completions into a single router on port 8090, configured via `MYTHRAX_DAEMON_PORT` in `main.rs` and `cli.rs`.
**Current Assumption** -> The unified router will handle all administrative, memory, MCP, and completions proxy endpoints fairly without resource starvation.
**Attack Scenario** -> An adversary or runaway agent floods the 8090 port with heavy model context payloads or search queries. The single port blocks, preventing administrative control (`/v1/daemon/stop` endpoint in `api.rs`) from being successfully processed, which leads to total system lockout.
**Blast Radius** -> Complete system lockout and loss of control over autonomous agents.
**Recommended Structural Change** -> Decouple the administrative control plane (stop commands, config) to an isolated port or process separated from the data plane.


**Labels**: `architecture-review`, `adversarial`

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
