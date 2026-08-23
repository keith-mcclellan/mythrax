#!/usr/bin/env python3
import os
import subprocess
import shlex

issues = [
    {
        "title": "Bug: Hardcoded 15-line limit in AST extraction truncates code context",
        "body": "**File and Line Number**: `mythrax-core/src/cognitive/ast.rs`, lines 69, 110\n\n**Minimal Reproducible Scenario**: \nWhen an AI agent parses a Rust codebase and calls `extract_rust_symbols` to inspect a function spanning more than 15 lines, the extracted code block truncates arbitrarily at `idx + 15`. The agent receives an incomplete function body, leading to hallucinations about the missing code.\n\n**Severity**: High\n\n**Suggested Fix**: \nReplace the hardcoded `let end_line = (idx + 15).min(lines.len());` with a proper AST traversal or a stack-based bracket/brace counting algorithm to precisely determine the function's end line.\n\n**Labels**: `bug`, `agent-found`"
    },
    {
        "title": "Bug: Mutex poisoning panic path in LogWriter crashes daemon",
        "body": "**File and Line Number**: `mythrax-core/src/cli/log_writer.rs`, lines 34, 100, 115\n\n**Minimal Reproducible Scenario**: \nIf a thread panics while holding the `inner` lock during a log write operation, the Mutex becomes poisoned. Any subsequent thread calling `self.inner.lock().unwrap()` (e.g., during `flush()` or `write_all_and_roll()`) will instantly panic, cascading the failure and crashing the entire Mythrax daemon.\n\n**Severity**: High\n\n**Suggested Fix**: \nReplace `.lock().unwrap()` with `.lock().unwrap_or_else(|poisoned| poisoned.into_inner())` to recover the lock state and continue logging, or gracefully bubble up the error instead of panicking.\n\n**Labels**: `bug`, `agent-found`"
    },
    {
        "title": "Bug: Negative IDF causing NaN in BM25 scoring",
        "body": "**File and Line Number**: `mythrax-core/src/retrieval/bm25.rs`, line 122\n\n**Minimal Reproducible Scenario**: \nIn edge cases (such as eventually consistent distributed indices or test data), if the term document frequency `df_t` exceeds the total document count `self.n`, the formula `(self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)` can become `<-1.0`. Adding `1.0` yields a negative number. Calling `.ln()` on a negative number returns `NaN`, silently corrupting downstream agent retrieval scores.\n\n**Severity**: Medium\n\n**Suggested Fix**: \nClamp the inner term to a minimum positive epsilon before taking the logarithm, e.g., `(((...).max(0.0001)) + 1.0).ln()`, or clamp `df_t` to be `<= self.n`.\n\n**Labels**: `bug`, `agent-found`"
    },
    {
        "title": "Bug: Incorrect 5.0 multiplier in Jaccard similarity causes false-positive clustering",
        "body": "**File and Line Number**: `mythrax-core/src/cognitive/pipeline/signals.rs`, line 96\n\n**Minimal Reproducible Scenario**: \nTwo independent agent memory facts share only 20% of their vocabulary (Jaccard similarity = 0.2). The fallback clustering logic multiplies this by 5.0: `(intersection as f32 / union as f32) * 5.0`. The resulting score is 1.0 (100%), which exceeds `config.cluster_similarity` and falsely groups unrelated facts into the same context window.\n\n**Severity**: High\n\n**Suggested Fix**: \nRemove the `* 5.0` multiplier on line 96 so the Jaccard similarity properly outputs a normalized `[0.0, 1.0]` bounded value.\n\n**Labels**: `bug`, `agent-found`"
    },
    {
        "title": "Bug: Test coverage gap in pipeline signals public functions",
        "body": "**File and Line Number**: `mythrax-core/src/cognitive/pipeline/signals.rs`, lines 6, 27, 34, 41, 118\n\n**Minimal Reproducible Scenario**: \nThe functions `slugify_title`, `derive_slug`, `resolve_rule_path`, `cluster_facts`, and `get_active_stm_anchors` are public and critical for agent memory organization, yet a codebase grep reveals zero corresponding tests in `evals/` or the file itself. Refactoring these functions risks silent regressions that won't be caught by CI.\n\n**Severity**: Medium\n\n**Suggested Fix**: \nAdd a `#[cfg(test)] mod tests { ... }` block to `mythrax-core/src/cognitive/pipeline/signals.rs` (or equivalent integration tests in `evals/`) covering positive, negative, and edge cases for these functions.\n\n**Labels**: `bug`, `agent-found`"
    }
]

if os.environ.get("GITHUB_ACTIONS"):
    for issue in issues:
        # Avoid shell=True to prevent command injection and parsing issues
        subprocess.run(["gh", "issue", "create", "--title", issue["title"], "--body", issue["body"], "--label", "bug,agent-found"])
else:
    os.makedirs("issues", exist_ok=True)
    for i, issue in enumerate(issues):
        filename = f"issues/bug_{i}.md"
        with open(filename, "w") as f:
            f.write(f"# {issue['title']}\n\n{issue['body']}")
        print(f"Created {filename}")
