## Bug Report: Missing Test Coverage for Critical Public Functions

**File & Line:** `mythrax-core/src/auth.rs`, `mythrax-core/src/embeddings.rs`

**Minimal Reproducible Scenario:** Public functions like `verify_token_constant_time` lack explicit `#[test]` unit tests.

**Severity:** Medium (Coverage Debt).

**Suggested Fix:** Require explicit unit tests for critical paths (Auth, Parsers, Memory Management). Begin by writing `#[test]` blocks for `verify_token_constant_time`.
