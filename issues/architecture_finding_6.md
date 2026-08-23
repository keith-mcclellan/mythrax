---
title: "API Gateway Coupling: Single-Port Design Limits Scalability and Security"
labels: ["architecture-review", "adversarial"]
---

## Finding
All REST, MCP, and completions proxy endpoints are consolidated onto a single port (8090).

## Current Assumption
A single-port gateway simplifies deployment and routing for a local or single-node architecture.

## Attack Scenario
As the system scales (e.g., 10x traffic or distributed deployment), the single port becomes a bottleneck. An attacker can launch a volumetric DDoS attack on the single port, impacting all services simultaneously (memory, inference, administration).

## Blast Radius
System-wide denial of service. Inability to isolate or independently scale critical services.

## Recommended Structural Change
Decouple the API gateway into separate, microservice-oriented endpoints (e.g., control plane, data plane, inference proxy) that can be independently scaled, secured, and deployed across different ports or nodes.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
