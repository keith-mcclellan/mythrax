#!/usr/bin/env python3
import os
import re
import subprocess
import shlex

# Check if running in GitHub Actions CI
is_ci = os.environ.get("GITHUB_ACTIONS") == "true"

def open_issue(title, body):
    if is_ci:
        subprocess.run(["gh", "issue", "create", "--title", title, "--body", body, "--label", "bug,agent-found"], check=True)
    else:
        os.makedirs("issues", exist_ok=True)
        filename = f"issues/{shlex.quote(title).replace(' ', '_').replace('/', '_')}.md"
        with open(filename, "w") as f:
            f.write(f"## {title}\n\n{body}\n")
        print(f"Mock issue created at {filename}")

findings = []

# 1. AST End Line Truncation (Logic Bug / Off-by-one)
findings.append({
    "title": "Bug Report: AST Symbol Extraction Hardcoded End Line Truncation",
    "body": "**File & Line:** `mythrax-core/src/cognitive/ast.rs`, lines 69, 110, 149, 188 (e.g., `let end_line = (idx + 15).min(lines.len());`)\n\n**Minimal Reproducible Scenario:** Run symbol extraction on a >15 line Rust/Python/TS/Go function. Agent context windows truncate the rest of the function.\n\n**Severity:** High (Agent hallucination risk).\n\n**Suggested Fix:** Implement a proper bracket/brace matching parser to accurately determine the `end_line` of the symbol block."
})

# 2. Log Writer Panic Path (Panic Path)
findings.append({
    "title": "Bug Report: `log_writer.rs` Mutex Poisoning Panic Path",
    "body": "**File & Line:** `mythrax-core/src/cli/log_writer.rs`, lines 34, 100, 115 (e.g., `let mut inner = self.inner.lock().unwrap();`)\n\n**Minimal Reproducible Scenario:** If another thread panics while holding the `inner` lock in `RollingFileLogger`, the Mutex becomes poisoned. When any subsequent log message is written, `self.inner.lock().unwrap()` panics, crashing the daemon.\n\n**Severity:** High (Crash in production daemon).\n\n**Suggested Fix:** Use `.unwrap_or_else(|e| e.into_inner())` to recover the lock or replace `std::sync::Mutex` with `parking_lot::Mutex`."
})

# 3. Cluster Facts Similarity Multiplier (Logic Bug)
findings.append({
    "title": "Bug Report: Agent Clustering Similarity Score Anomaly",
    "body": "**File & Line:** `mythrax-core/src/cognitive/pipeline/signals.rs`, line 96 (`(intersection as f32 / union as f32) * 5.0`)\n\n**Minimal Reproducible Scenario:** Trigger a pipeline run where two facts do not have valid embeddings. The code calculates Jaccard similarity (0.0 to 1.0) but multiplies it by `5.0`. A 20% word overlap receives a score of `1.0`, guaranteeing false-positive clustering.\n\n**Severity:** High (Agent Memory Corruption).\n\n**Suggested Fix:** Remove the `* 5.0` multiplier so Jaccard similarity correctly returns a value between 0.0 and 1.0."
})

# 4. BM25 Negative IDF (Logic Bug / NaN Risk)
findings.append({
    "title": "Bug Report: Okapi BM25 Negative IDF NaN Risk",
    "body": "**File & Line:** `mythrax-core/src/retrieval/bm25.rs`, line 122 (`let idf = (((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln();`)\n\n**Minimal Reproducible Scenario:** Edge cases in global statistics (e.g., if `df_t` slightly exceeds `self.n`) can result in negative IDF scores or zero arguments to `.ln()`, leading to `NaN` scores, breaking agent retrieval.\n\n**Severity:** Medium (Retrieval Degradation).\n\n**Suggested Fix:** Apply `.max(0.0)` after calculation: `let idf = ((((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln()).max(0.0);`"
})

# 5. Missing Test Coverage
findings.append({
    "title": "Bug Report: Missing Test Coverage for Critical Public Functions",
    "body": "**File & Line:** `mythrax-core/src/auth.rs`, `mythrax-core/src/embeddings.rs`\n\n**Minimal Reproducible Scenario:** Public functions like `verify_token_constant_time` lack explicit `#[test]` unit tests.\n\n**Severity:** Medium (Coverage Debt).\n\n**Suggested Fix:** Require explicit unit tests for critical paths (Auth, Parsers, Memory Management). Begin by writing `#[test]` blocks for `verify_token_constant_time`."
})

for finding in findings:
    open_issue(finding["title"], finding["body"])
