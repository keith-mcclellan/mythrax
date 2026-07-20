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
