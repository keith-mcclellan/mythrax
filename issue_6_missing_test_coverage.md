---
title: "Bug: Test coverage gaps for public functions in mythrax-core"
labels: ["bug", "agent-found"]
---

## Description
During a routine codebase audit, several public functions in `mythrax-core` were found to have no corresponding unit or integration tests. A robust testing suite is critical to prevent regressions, particularly for public API boundaries.

## Missing Test Coverage
The following public functions lack dedicated test coverage:

**`mythrax-core/src/bench/metrics.rs`**
- `pub fn evaluate_retrieval(...)`
- `pub fn ndcg(...)`
- `pub fn session_id_from_corpus_id(...)`

**`mythrax-core/src/vault/operations.rs`**
- `pub fn stringify_record_id(...)`

**`mythrax-core/src/vault/watcher.rs`**
- `pub fn start_watching(...)`
- `pub fn slugify(...)`
- `pub fn format_episode_markdown(...)`
- `pub fn format_wisdom_markdown(...)`

**`mythrax-core/src/auth.rs`**
- `pub fn load_token(...)`
- `pub fn verify_token_constant_time(...)`

**`mythrax-core/src/db/backend.rs`**
- `pub fn record_key_to_string(...)`
- `pub fn parse_record_id(...)`
- `pub fn format_record_id(...)`
- `pub fn parse_temporal_cues(...)`
- `pub fn calculate_temporal_decay(...)`
- `pub fn sentence_cosine_similarity(...)`

**`mythrax-core/src/llm/mlx_weights.rs`**
- `pub fn get_linear(...)`
- `pub fn get_rms_norm(...)`
- `pub fn get_layer_norm(...)`
- `pub fn get_quantized_linear(...)`
- `pub fn get_embedding(...)`

## Severity
**Medium** - While not immediate bugs, lack of coverage severely increases the risk of regressions.

## Suggested Fix
Implement unit tests in the respective `tests` modules (or `mod tests`) for each identified function. Ensure edge cases (like empty inputs, boundary values, or missing fields) are tested appropriately.
