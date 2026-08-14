---
title: Panic risk in embeddings finalization logic
labels: ["bug", "agent-found"]
---
## Bug Description
**File and Line Number:** `mythrax-core/src/embeddings.rs:722` and `1095`

**Minimal Reproducible Scenario:** In `embed_batch` and `embed_queries`, the logic maps over an array of `Option<Vec<f32>>` and calls `unwrap()` unconditionally (`results.into_iter().map(|opt| opt.unwrap()).collect()`). If `uncached_texts` computation fails to populate `results` correctly, or if `embed_sub_batch` returns fewer embeddings than requested (a realistic possibility with external ML models, context window limits, or API instability), `results[orig_idx]` will remain `None`. The unconditional `unwrap()` will panic the entire service when hitting a `None` value.

**Severity:** High (Service crash on unexpected model/API output size)

**Suggested Fix:** Replace `unwrap()` with a safe check, e.g., mapping and collecting into `Result<Vec<Vec<f32>>>` or skipping `None`s. `results.into_iter().collect::<Option<Vec<_>>>().ok_or_else(|| anyhow!("Missing embeddings in batch"))?`.
