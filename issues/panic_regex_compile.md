---
title: Panic on Regex Compilation in manage_handlers
labels: bug, agent-found
---
# Bug Report

**Severity:** HIGH
**File:** `mythrax-core/src/mcp_routes/manage_handlers.rs`
**Line:** 140, 155

## Minimal Reproducible Scenario
The lines `let re = regex::Regex::new(r"(?m)^status:\s*.*$").unwrap();` and `let re_fail = regex::Regex::new(r"(?m)^fail_reason:\s*.*$").unwrap();` recompile static regexes on every function invocation. If a syntax error is introduced into the regex string during refactoring, or if regex engine internals fail on initialization for an unexpected reason, this will panic the thread processing the request. While the regex string is static and currently valid, the unwrap is technically reachable via internal compilation panics or future typos.

## Suggested Fix
Use `once_cell::sync::Lazy` or `std::sync::OnceLock` to compile the regexes once at startup and handle the initialization error gracefully, or at least avoid recompiling per request.
