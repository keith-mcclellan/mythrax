---
tags: architecture-review, adversarial
---

# Single-Port API Gateway Single Point of Failure

**Finding**: Single-Port API Gateway (Port 8090) routing for all endpoints and Client Mode fallback.

**Current Assumption**: A unified router on a single port can reliably handle all REST, MCP, and Model proxy traffic without contention or fatal failure.

**Attack Scenario**: An attacker floods the Model Proxy with heavy completion requests or slowloris connections, exhausting the Tokio runtime's worker threads or file descriptors.

**Blast Radius**: The entire daemon locks up. REST, MCP, and database fallback routing fail entirely, leaving the system with no graceful degradation path and paralyzing all agents.

**Recommended Structural Change**: Isolate the API Gateway into separate listeners or processes for Control Plane (REST/MCP) and Data Plane (Model Proxy). Implement load shedding, independent thread pools, and a clear fallback path for MCP if the model proxy degrades.
