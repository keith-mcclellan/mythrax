---
title: "Missing Test Coverage for Context Paging Logic"
labels: ["bug", "agent-found"]
---

## Description
The `mythrax-core/src/cognitive/paging.rs` module contains several pure public functions such as `extract_symbols`, `deduplicate_page_ids`, and `relink_page_references` which handle AST-like regex parsing for code artifacts (Rust, Python, TypeScript). These critical context window management tools have absolutely zero unit tests backing their behavior.

## Location
`mythrax-core/src/cognitive/paging.rs`

## Minimal Reproducible Scenario
Run `MYTHRAX_TEST_MOCK=1 cargo test --lib paging` and observe zero executed tests. Without tests, any adjustments to the regex capturing logic for structural memory bounds could silently truncate files or omit necessary symbol headers, causing silent degradation of the AI context window.

## Severity
Low/Medium (Technical Debt / Correctness risk)

## Suggested Fix
Append a `mod tests` block inside `paging.rs` with coverage for standard code structures.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_symbols_rust() {
        let rs_code = "pub struct MyStruct { ... }\nfn my_func() {}";
        let symbols = extract_symbols(rs_code, "rs");
        assert_eq!(symbols.len(), 2);
    }
}
```
