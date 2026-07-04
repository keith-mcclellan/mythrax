---
title: "Bug: Panic path when empty prompt causes input_ids underflow in LLM pipeline"
labels: ["bug", "agent-found"]
---

## Description
In `mythrax-core/src/llm/mod.rs`, around line 627 and 634, there is a potential panic due to underflow or out-of-bounds indexing if `input_ids` is empty.

```rust
let last_token = input_ids[input_ids.len() - 1] as i32;
// ...
(input_ids.len() - 1) as i32
```

## Reproducible Scenario
If an empty prompt string or a string that the tokenizer parses into an empty sequence is passed to the generation pipeline, `input_ids.len()` will be 0.
1. Submitting a blank string to the completion/generation endpoints.
2. The tokenizer returns an empty `tokens` array.
3. The loop iterations start. On `index = 1` or if `index = 0` handles the zero length unexpectedly, the code will eventually hit `input_ids.len() - 1` resulting in an underflow (causing a panic in debug mode) or an out-of-bounds array access (causing a panic in both debug and release modes).

## Severity
**High**

## Suggested Fix
Check if `input_ids` is empty immediately after tokenization, and return an error or early return an empty string if so, before initializing the MLX model buffers and beginning the autoregressive loop.

```rust
if input_ids.is_empty() {
    return Ok(String::new());
}
```
