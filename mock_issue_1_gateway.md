# Architectural Liability: Single-Port API Gateway Monolith

**Finding**: The system relies on a Single-Port API Gateway (Port 8090) that multiplexes all REST, MCP, and external API requests into a single routing bottleneck.

**Current Assumption**: Consolidating all requests to a single port simplifies client auto-spawning and network configuration without severely impacting throughput, as local LLM inference is expected to be the primary bottleneck.

**Attack Scenario**: A malicious or looping agent spams high-frequency, low-latency MCP tool calls or large file ingestion requests. Because administrative controls, memory access, and MCP routing all share the same thread pool and port, the high volume of MCP traffic exhausts the gateway's connection limits or thread capacity.

**Blast Radius**: **Total System Lockout.** If the single port becomes unresponsive, external tools cannot reach the model, the user cannot send administrative commands to stop the looping agent, and other concurrent agents are completely blocked. There is no graceful degradation or dedicated administrative channel.

**Recommended Structural Change**: Decouple the control plane from the data plane. Split the gateway into at least two ports: an Administrative/Control Port (for configuration, stopping agents, health checks) and a Data/MCP Port (for high-volume memory/tool routing).

Tags: `architecture-review`, `adversarial`
