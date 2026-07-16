---
title: "Bug: Panic when embedding cache fails to populate all elements"
labels: ["bug", "agent-found"]
---

## Description
There is a logic bug/panic path in `mythrax-core/src/embeddings.rs` where the code assumes that all texts successfully generated embeddings.

**File:** `mythrax-core/src/embeddings.rs`
**Lines:** 532, 819

**Severity:** High (Crash / DoS)

**Minimal Reproducible Scenario:**
1. Call `embed_batch` or `embed` with an array of inputs where one input triggers an unexpected failure in the backend MLX or Hugging Face embedder (e.g., token length exceeded for one item, or an internal engine panic that gets caught/swallowed returning an empty sub-batch).
2. The `uncached_embeddings` loop finishes, but leaves `None` at the index of the failed input in `results`.
3. The method blindly calls `results.into_iter().map(|opt| opt.unwrap()).collect();`.
4. The process panics at `opt.unwrap()`, taking down the service.

**Suggested Fix:**
Instead of blindly unwrapping, map the `Option` to a `Result` and propagate the error. For example:
`results.into_iter().map(|opt| opt.ok_or_else(|| anyhow::anyhow!("Failed to generate embedding for all inputs"))).collect::<Result<Vec<_>>>()?`
