# Rust Coding Standards

This document establishes the Rust coding standards and guidelines for this project, aligned with the official [Rust Style Guide](https://github.com/rust-lang/rust/tree/HEAD/src/doc/style-guide) defined by the Rust Style Team.

All Rust code in this repository must comply with these guidelines. Compliance is enforced via automated pre-commit and CI validation.

---

## 1. Automated Formatting (`rustfmt`)

All Rust source code must be formatted using `rustfmt`. Manual formatting workarounds are prohibited.

*   **Indentation:** 4 spaces (no tabs).
*   **Line Width:** Maximum 100 characters. Lines exceeding this must be wrapped according to standard `rustfmt` wrapping rules.
*   **Braces Style:** Egyptian style (opening brace on the same line as the declaration/statement, closing brace on its own line matching the start indentation level).
*   **Newlines:** Unix-style line endings (`\n`).

---

## 2. Imports Grouping & Sorting

Imports (`use` declarations) must be organized into distinct, sorted blocks separated by a single empty line. 

Imports within each block must be sorted alphabetically. The blocks must be ordered as follows:

1.  **Standard Library:** Imports from `std`, `core`, and `alloc`.
2.  **External Crates:** Imports from third-party dependencies defined in `Cargo.toml`.
3.  **Local Crates and Modules:** Imports starting with `crate::`, `self::`, `super::`, or local module identifiers.

### Example:
```rust
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::contracts::Episode;
use crate::db::StorageBackend;
```

---

## 3. Naming Conventions

Follow standard Rust naming conventions:

*   **Types, Structs, Enums, Traits:** `UpperCamelCase` (e.g., `StorageBackend`, `EpisodeRaw`).
*   **Functions, Methods, Local Variables, Module Names:** `snake_case` (e.g., `run_graduation_pipeline`, `session_id`).
*   **Constants and Statics:** `SCREAMING_SNAKE_CASE` (e.g., `INIT_SCHEMA`, `IS_INGESTING`).
*   **Type Parameters (Generics):** A single uppercase letter (`T`, `U`) or a descriptive `UpperCamelCase` name starting with `T` (e.g., `TBackend`).

---

## 4. Lint Enforcement & Quality Gates

Code quality must be enforced using Rust's compiler lints and `clippy`.

*   **Clippy Checks:** All code must pass `cargo clippy` without errors or warnings. Critical code modules should consider enforcing strict checks:
    ```rust
    #![deny(clippy::all)]
    #![warn(clippy::pedantic)]
    ```
*   **Dead Code:** Unused code, fields, or functions must not be checked in with `#[allow(dead_code)]` unless explicitly designed as a public API or pending future integration. Integrate components actively to ensure full coverage.
*   **Errors and Panics:** 
    *   Avoid raw `unwrap()` or `expect()` in production paths. Prefer returning a `Result` wrapped with appropriate context using `anyhow` or custom `thiserror` types.
    *   Use `expect()` only in test suites or when an invariant is mathematically guaranteed.

---

## 5. Documentation

*   **Public API:** All public structs, enums, fields, traits, and functions should have documentation comments (`///`).
*   **Internal Comments:** Document *why* complex logic is written in a particular way, rather than *what* the code does. Keep comments synchronized with code changes.

---

## 6. Architecture & Concurrency Directives

All Rust code written for features, tracks, and refactoring tasks must adhere strictly to these architectural guidelines:

*   **Direct Native Async Refactoring (Anti-Bridge Rule):** Subagents and developers MUST NOT create parallel `_async` methods alongside existing sync methods, nor use `futures::executor::block_on` or `tokio::task::block_in_place` fallbacks inside default trait methods. When converting a trait or subsystem to async, update the trait definition directly with `async fn` and refactor all downstream callsites natively.
*   **Top-Level Scoping & Safe RAII Guards:** Operational status guards (e.g., `IS_INGESTING`) and cleanup routines MUST be scoped at the outermost public entry point of a function, covering all match arms, harness types, and execution branches. All temporary database state (e.g., `pipeline_cluster`) or filesystem resources MUST use safe RAII scope guards (implementing `Drop` with `Arc<dyn Trait>` handles) so cleanup is guaranteed on early `?` error returns, panics, and scope drops. Unsafe raw pointer transmutes (`*const dyn Trait`) are strictly forbidden.
*   **Strict Lock Ordering & Contention Prevention:** Never hold a primary lock (e.g., `EMBEDDING_CACHE` or `term_counts_cache`) while acquiring a secondary lock (e.g., `SQLITE_CACHE_CONN` or inner scope locks). Always extract required data into local variables, drop the primary lock completely, and then acquire secondary locks or execute I/O operations.
*   **Algorithmic Complexity & Bulk Operations (No $O(N)$ Hot-Path Scans):** Never perform $O(N)$ linear iteration scans (e.g., `.min_by_key()`) inside hot-path loops or per-element insertions. Use constant-time $O(1)$ data structures (e.g., `lru::LruCache`) or perform bulk pruning (evicting the bottom 10% of items in a single pass when capacity is reached).
*   **Complete Resource Lifecycle & Write-on-Evict Safety:** Any component that loads GPU VRAM weights or allocates heavy in-memory buffers MUST implement a public `evict()` method and register it with the background idle eviction loop (`daemon.rs`). Any cache eviction mechanism (such as `LruCache::push` or `resize`) MUST inspect evicted items and immediately persist dirty entries to disk before dropping them from memory (Write-on-Evict).

