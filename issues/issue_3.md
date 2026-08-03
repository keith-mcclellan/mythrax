# 🛡️ Sentinel: [HIGH] Fix O(N) loop bottleneck in embedding similarity

**Labels:** `bug`, `agent-found`

🚨 Severity: HIGH
💡 Vulnerability: Recalculating Euclidean norms inside inner execution loops for high-dimensional arrays during clustering.
🎯 Impact: Causes severe O(N^2) latency bottlenecks during embedding similarity calculations, significantly degrading performance at scale.
🔧 Fix: Hoist heavy vector operations outside nested loops and lazily cache them (e.g., precomputing `embedding_norm` on structs like `GradCandidate`).
✅ Verification: Profiling confirms norm calculations are performed once per candidate rather than N times, restoring O(N) performance.

**Minimal Reproducible Scenario:**
Trigger a large clustering operation with thousands of high-dimensional embeddings (e.g. 1536d) in the cognitive pipeline. The system will stall or become unresponsive due to repeated norm recalculations inside nested comparison loops.

**File and Line Number:**
`mythrax-core/src/cognitive/pipeline.rs` (or related clustering file in `mythrax-core/src/cognitive/`)

**Estimated Effort:** High
