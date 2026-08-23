#!/usr/bin/env python3
import os
import json
import subprocess
import shlex

issues = [
    {
        "title": "Bug: Hardcoded end_line calculation truncates long code blocks in AST extraction",
        "body": "**File and Line:** `mythrax-core/src/cognitive/ast.rs` (lines 69, 110, 149, 188)\n\n**Scenario:** When parsing code to extract symbols (e.g., Rust, Python), the `end_line` is hardcoded as `(idx + 15).min(lines.len())`. If a function or struct spans more than 15 lines, it is abruptly truncated, which risks context window hallucination for the AI agent as it receives incomplete ASTs.\n\n**Severity:** High (causes incomplete symbol processing and AI agent hallucination).\n\n**Suggested Fix:** Implement proper brace or indentation matching to dynamically find the correct end of the scope for the symbol, rather than hardcoding a 15-line limit.",
        "labels": ["bug", "agent-found"]
    },
    {
        "title": "Bug: Mutex poisoning panic path in log_writer.rs",
        "body": "**File and Line:** `mythrax-core/src/cli/log_writer.rs` (lines 34, 100, 115)\n\n**Scenario:** The `SizeRollingFileWriter` calls `self.inner.lock().unwrap()`. If another thread panics while holding this lock, the mutex becomes poisoned. Any subsequent log writes across all threads will hit the `.unwrap()` and panic, causing the daemon to crash globally on a simple log write.\n\n**Severity:** Critical (daemon crash vector).\n\n**Suggested Fix:** Use `.unwrap_or_else(|poisoned| poisoned.into_inner())` to recover the lock despite poisoning, or switch to a lock-free/channel-based logging design.",
        "labels": ["bug", "agent-found"]
    },
    {
        "title": "Bug: BM25 Okapi IDF formula can result in NaN or negative values",
        "body": "**File and Line:** `mythrax-core/src/retrieval/bm25.rs` (line 122)\n\n**Scenario:** The IDF formula `(((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln()` can fail if global IDF stats edge cases cause `df_t` to exceed `self.n`, or due to floating point inaccuracies leading to a negative value inside `.ln()`, resulting in NaN. This completely breaks ranking.\n\n**Severity:** High (breaks retrieval ranking).\n\n**Suggested Fix:** Clamp the value inside the logarithm to a minimum positive value (e.g., `1e-6`) or use a BM25 variant that enforces positive IDFs (like BM25L or applying `f32::max(0.0, ...)` after).",
        "labels": ["bug", "agent-found"]
    },
    {
        "title": "Bug: Erroneous 5x Jaccard similarity amplification in cluster_facts",
        "body": "**File and Line:** `mythrax-core/src/cognitive/pipeline/signals.rs` (line 96)\n\n**Scenario:** In the fallback text similarity logic for `cluster_facts`, the Jaccard similarity is calculated as `(intersection as f32 / union as f32) * 5.0`. A valid Jaccard similarity is in the range `[0, 1]`. Multiplying it by 5.0 inflates the score massively, causing almost any text overlap to exceed typical threshold bounds and generating huge false-positive clusters.\n\n**Severity:** High (breaks clustering pipeline, pollutes contexts).\n\n**Suggested Fix:** Remove the `* 5.0` multiplier so the Jaccard similarity correctly bounded between 0.0 and 1.0.",
        "labels": ["bug", "agent-found"]
    }
]

use_gh = "GITHUB_ACTIONS" in os.environ

if use_gh:
    for issue in issues:
        cmd = ["gh", "issue", "create", "--title", issue["title"], "--body", issue["body"], "--label", ",".join(issue["labels"])]
        print(f"Executing: {' '.join(cmd)}")
        subprocess.run(cmd, check=True)
else:
    os.makedirs("issues", exist_ok=True)
    for i, issue in enumerate(issues):
        filename = f"issues/issue_{i+1}.md"
        with open(filename, "w") as f:
            f.write(f"---\nTitle: {issue['title']}\nLabels: {', '.join(issue['labels'])}\n---\n\n{issue['body']}\n")
        print(f"Created {filename}")
