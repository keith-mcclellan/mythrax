# Mythrax Codebase Mock, Configuration, Integration & Documentation Audit Report

## Executive Summary

A comprehensive read-only audit of all 49 Rust source files in `mythrax-core/src/` identified **10 mocked/stubbed code instances** in production paths, **100+ hardcoded values** that should be configurable (including 3 critical security-relevant hardcoded auth tokens and 1 already-expired hardcoded date), **25+ external process invocations** via `std::process::Command` (spanning `git`, `sh`, `cargo`, `sysctl`, `ps`, `kill`, and arbitrary command execution), and **12 documentation discrepancies** between the implemented code and the repository documentation. Of particular concern: 5 hardcoded absolute paths referencing `/Users/keith/mythrax-vault` are compiled into the production binary, test-detection logic is embedded in production search code, and the CLI version string is stale at `"1.0.0"` while `Cargo.toml` declares `v2.2.0`.

---

## 1. Mocked & Stubbed Code Findings

### MOCK-001: `audit_tailwind` — No-Op Stub in verify.rs
- **File Path:** [verify.rs#L29-L31](file:///Users/keith/Documents/mythrax/mythrax-core/src/verify.rs#L29-L31)
- **Code Snippet:**
  ```rust
  fn audit_tailwind(_workspace_path: &Path) -> (bool, Vec<String>) {
      (true, Vec::new())
  }
  ```
- **Matching Specification:** Design specification not found. No spec in `specs/` defines a Tailwind audit feature or its expected behavior.
- **Root Cause & Description:** This function is called from the compliance audit path (`verify_compliance`). It always returns a passing result `(true, Vec::new())`, meaning Tailwind violations are never detected. The parameter is prefixed with `_`, confirming it was never implemented. This is a stub that was likely placed as a placeholder during initial compliance audit development and never replaced with real logic.
- **Fix Recommendation:** Either implement a real Tailwind CSS audit (scanning for inline Tailwind classes, checking against a block list of prohibited utility classes), or remove the function entirely and its call site if Tailwind auditing is not a desired feature. If removing, update the `verify_compliance` return structure accordingly.

---

### MOCK-002: `audit_tailwind` — Duplicate No-Op Stub in daemon.rs
- **File Path:** [daemon.rs#L29-L30](file:///Users/keith/Documents/mythrax/mythrax-core/src/daemon.rs#L29-L30)
- **Code Snippet:**
  ```rust
  // Same stub signature as verify.rs — returns (true, Vec::new())
  ```
- **Matching Specification:** Design specification not found.
- **Root Cause & Description:** Duplicate of MOCK-001 in a different module. This suggests the function was copy-pasted rather than extracted into a shared module, compounding the stub problem.
- **Fix Recommendation:** Consolidate into a single implementation in `verify.rs` and remove the duplicate. Then implement real logic per MOCK-001.

---

### MOCK-003: `check_memory_pressure` — Always-False Stub
- **File Path:** [daemon.rs#L526-L528](file:///Users/keith/Documents/mythrax/mythrax-core/src/daemon.rs#L526-L528)
- **Code Snippet:**
  ```rust
  pub fn check_memory_pressure() -> bool {
      false
  }
  ```
- **Matching Specification:** [mythrax-2.0 design](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/design.md) — specifies swap monitoring with `sysctl vm.swapusage`, tier-based thresholds (2.0/3.0/6.0 GB), and `disable_swap_monitor` config flag.
- **Root Cause & Description:** The 2.0 spec defines a real memory pressure check using `sysctl vm.swapusage` on macOS (which IS implemented in `main.rs:337-340`), but this function was left as a stub. The adjacent `check_swap_pressure()` at line 517 IS implemented with real tier-based threshold logic. This stub appears to be a leftover from an earlier iteration.
- **Fix Recommendation:** Either delegate to the existing `check_swap_pressure()` with actual `sysctl` output parsing (already implemented in `main.rs:337-340`), or remove this dead stub if `check_swap_pressure()` fully supersedes it.

---

