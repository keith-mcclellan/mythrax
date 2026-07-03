---
title: "Bug: Critical panic risk in `meta_skill.rs` due to unhandled LLM JSON output"
labels: ["bug", "agent-found"]
---

### Vulnerability / Bug Description
The agent scaffolding in `mythrax-core/src/cognitive/meta_skill.rs` assumes that LLM-generated JSON always contains specific structures without validation. Specifically, on line 309, it does `sug["source_skills"].as_array().unwrap();`, which will cause a thread panic if the LLM produces unexpected fields or non-array types. Other adjacent fields (`suggested_target_name`, `similarity`, `reason`) suffer from the same issue.

### File and Line Number
- `mythrax-core/src/cognitive/meta_skill.rs`, lines 309-316.

### Minimal Reproducible Scenario
1. Trigger the `merge_skills` function, resulting in the system calling the LLM to provide merge candidates.
2. An adversarial or simply hallucinated response from the LLM returns a JSON object like `{"suggested_target_name": "x"}` missing the `source_skills` field, or mapping it to a string.
3. The parsing code attempts `.as_array().unwrap()`, leading to an immediate unhandled panic and daemon crash.

### Severity
**High**. This is a brittle parsing path that deals directly with probabilistic external data, meaning the crash can be triggered frequently during normal operation.

### Suggested Fix
The fix has been implemented in the current PR. The solution involves replacing the `.unwrap()` calls with safe `.ok_or_else` combinators and `.unwrap_or("unknown")` fallback values.
