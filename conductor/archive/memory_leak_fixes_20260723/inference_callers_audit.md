# Inference Callers Audit (Phase 5)

## 1. Async Context Callers (Safe to use `.await`)
- `src/llm/mod.rs:658` (`engine.generate(prompt, system_instruction).await?`)

## 2. Sync Context Callers (Requires Bubble-Up / `block_in_place`)
- No explicit blocking `generate`, `complete`, or `rerank` callers were identified in `llm/mod.rs` (other callers might be scattered, but within `llm/` module logic, only async call found).

## 3. Trait Definitions & Implementations (Requires Signature Change)
- `src/llm/mod.rs:1179` (`fn generate(`)
- `src/llm/mod.rs:1251` (`fn generate(`)
