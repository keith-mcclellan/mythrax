---
title: Test Coverage Gap: Missing tests for core API route endpoints in mythrax-core
labels: bug, agent-found
---

**File & Line:** `mythrax-core/src/api.rs` (Various public route handlers like `handle_search`, `handle_ingest`, etc)

**Minimal Reproducible Scenario:** Review of the test suite and eval harness reveals significant test coverage gaps for the main API endpoints exposed via `mythrax-core`. These public endpoints accept JSON payloads and interact with the database backend but lack unit/integration tests covering standard execution paths, error handling, and payload parsing.

**Severity:** Medium

**Suggested Fix:** Write dedicated integration tests in a new or existing test module (e.g., `api_tests.rs`) covering the happy paths and expected error scenarios for each public handler function in `api.rs`.