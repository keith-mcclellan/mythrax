# 🛡️ Sentinel: [HIGH] Fix unbounded recursion risk in temporal expansion graph traversal

**Labels:** `bug`, `agent-found`

🚨 Severity: HIGH
💡 Vulnerability: Sliding Window Caps (e.g., 1,000-element `VecDeque`) and `LIMIT 50` constraints per hop level in `mythrax-core/src/db/crud_operations.rs` can yield 125,000 nodes at depth 3, creating an unbounded recursion risk.
🎯 Impact: Unbounded recursion leading to potential denial-of-service (OOM or CPU exhaustion) via adversarial memory graphs constructed by users.
🔧 Fix: Implement a strict global node limit or depth cap for graph traversals, independent of per-hop limits.
✅ Verification: Traversal of highly connected graphs halts before exhausting system resources, honoring the global cap.

**Minimal Reproducible Scenario:**
Create a highly connected temporal memory graph (e.g., node A relates to 50 nodes, each of which relates to 50 nodes, etc.) up to depth 3 or 4. Query node A with temporal expansion enabled. The server will run out of memory or CPU processing the exponentially growing traversal queue despite the per-hop `LIMIT 50`.

**File and Line Number:**
`mythrax-core/src/db/crud_operations.rs` lines 1692, 1711, 1715, 2703, 2705, 2712, 2714

**Estimated Effort:** Medium
