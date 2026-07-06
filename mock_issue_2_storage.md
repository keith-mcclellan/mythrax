---
tags: [architecture-review, adversarial]
---
# Finding: Tightly Coupled Storage and Orchestration

## Current Assumption
The Mythrax 2.0 Core Daemon securely manages the Single-Port API Gateway, SurrealKV/RocksDB engines, Model Broker, WAL, and background schedulers all concurrently within a single monolithic Rust process.

## Attack Scenario
A vulnerability or panic triggered in one component—such as an adversarial prompt processed by the Model Broker, or a malicious file parsed by the Obsidian vault watcher—will panic or crash the entire monolithic daemon process.

## Blast Radius
Complete system outage. All concurrent processes, including API serving, background scheduling loops, memory ingestion, and model routing are abruptly terminated.

## Recommended Structural Change
Decouple the architecture into separate, isolated services (e.g., Gateway Service, Storage Service, Inference Service) communicating via gRPC or secure IPC. A crash in the Inference Service processing an adversarial prompt must not bring down the Storage Service or the API Gateway.
