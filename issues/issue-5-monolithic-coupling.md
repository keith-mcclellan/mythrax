---
tags: [architecture-review, adversarial]
---
# Architectural Liability: Monolithic Coupling

## Finding
The API Gateway, Model Broker, File System Watcher, and Database Daemon are tightly coupled into a single monolithic Rust process.

## Code Reference
`ARCHITECTURE.md` (Single-Port API Gateway, Three-Tiered Model Broker, and Daemon architecture) and `mythrax-core/src/api.rs`.

## Current Assumption
Running all components in a single process optimizes for latency and simplifies deployment.

## Attack Scenario
A bug in the File System Watcher or a memory leak in the in-process Model Broker causes the entire process to crash.

## Blast Radius
Complete system failure. Because the components cannot be independently deployed or scaled, a failure in one subsystem takes down the entire cognitive architecture, gateway, and database access.

## Recommended Structural Change
Decouple the monolithic daemon into a microservices or actor-based architecture. Separate the API Gateway, Model Broker, and Storage layers so they can be individually scaled, monitored, and restarted without affecting the others.

**Note: Do not close this issue without a documented architectural decision record (ADR) response.**
