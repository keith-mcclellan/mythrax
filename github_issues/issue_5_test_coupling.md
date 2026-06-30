---
title: "Architecture Review: Test-Detection Coupling in Production Search Code"
labels: ["architecture-review", "adversarial"]
---

### Finding
Test-Detection Coupling in Production Search Code

### Current Assumption
Injecting `MYTHRAX_SIGMOID_GATED_SEARCH_TEST` environment variable checks into the production `search()` function in `db/backend.rs` is a valid way to test Sigmoid gating without a complex mock framework.

### Attack Scenario
The production retrieval engine is tightly coupled to test infrastructure. An attacker or accidental misconfiguration sets the test environment variable in a production deployment. The search function immediately bypasses the vector index and returns hardcoded similarity values (0.85, 0.50, 1.0) for every query.

### Blast Radius
Complete destruction of memory relevance. Agents receive hardcoded, meaningless context for all semantic queries, destroying the cognitive engine's capability without raising any errors.

### Recommended Structural Change
Decouple testing logic from production binary paths. Remove test-detection code from `db/backend.rs` entirely. Implement dependency injection for similarity scoring or rely strictly on compile-time gating (`#[cfg(test)]`).

> **Note:** Do not close this issue without a documented Architectural Decision Record (ADR) response.