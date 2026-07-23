# ⚡ Bolt: [performance improvement] Hoist embedding norm computations

## 💡 What
This PR hoists the Euclidean norm computation out of several inner execution loops in `mythrax-core/src/cognitive/synthesis.rs`. Specifically:
1. In `check_for_contradictions_and_merge`, `norm_u` is computed once outside the `for` loop.
2. In `form_grad_clusters`, the `GradCandidate` struct has been updated to include a `norm` field, pre-computed upon instantiation or cluster construction.
3. The in-memory fallback `for other in &candidates` loops now use `cand.norm` and `other.norm` directly instead of computing the norm per iteration.

## 🎯 Why
Recalculating constants like `norm_u` and `norm_v` inside inner execution loops for high-dimensional arrays (like 1536d embeddings) creates a severe $O(N^2)$ latency bottleneck, especially as the number of nodes or candidates increases.

## 📊 Impact
Reduces redundant computation loops significantly. This turns quadratic norm recalculations into a linear $O(N)$ pre-computation phase and $O(1)$ property accesses inside the loop, severely speeding up local embedding similarities and validations.

## 🔬 Measurement
Run `MYTHRAX_TEST_MOCK=1 cargo test --lib cognitive::synthesis` and observe the lack of regressions. In production scenarios with large candidate counts, the CPU usage should be noticeably lower.
