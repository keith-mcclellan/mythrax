---
title: "Bug: Off-by-one errors / arbitrary context window truncation in AST extraction"
labels: bug, agent-found
---
## Issue
File: `mythrax-core/src/cognitive/ast.rs`
Lines: 69, 110, 149, 188

## Minimal Reproducible Scenario
An AST symbol extraction (e.g., for `extract_rust_symbols`) on a large block of code (e.g. 50 lines) will only extract `idx + 15` lines instead of the actual full length of the symbol. This logic error truncates the context window and causes agents to miss critical downstream logic in the block, resulting in hallucinations.

## Severity
High

## Suggested Fix
Replace the naive `(idx + 15).min(lines.len())` static offset with proper block parsing (e.g. brace matching `{...}`) to dynamically determine the correct end line of the parsed function/struct.
