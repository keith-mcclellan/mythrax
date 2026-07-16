---
title: "Bug: Silent Logic Failure in Temporal Query Regex Initialization"
labels: ["bug", "agent-found"]
---

# Silent Logic Failure in Temporal Query Regex Initialization

**File:** `mythrax-core/src/db/query_classification.rs`
**Line:** 41

## Description
In `mythrax-core/src/db/query_classification.rs`, the function `split_temporal_query` uses a statically compiled regex `CLEANING_RE` to strip temporal terms from queries. The regex matches `\b(before|preceding|...)\b`.
However, the regex is case-sensitive. Meanwhile, the `classify_query` function converts the query to lowercase before checking for keywords.
This results in a critical silent logic failure: if a user or agent submits a query like `"Meetings Before yesterday"`, `classify_query` correctly identifies the `Temporal` category, but `split_temporal_query` will *fail* to strip the capitalized word `"Before"`. As a result, the "cleaned" query retains temporal terms which can disrupt downstream semantic retrieval and logic branches relying on pure temporal isolation.

## Minimal Reproducible Scenario
1. Input query: `"Find documents Before last week"`
2. `classify_query` converts to lower case and detects `"before"`, marking the category as `Temporal`.
3. Execution proceeds to `split_temporal_query("Find documents Before last week")`.
4. `CLEANING_RE` matches `"last"` and `"week"`, but misses the capitalized `"Before"`.
5. The function returns a cleaned query of `"Find documents Before"`.

## Severity
**Medium**. This produces silent logic errors where false positives/dirty strings are forwarded to semantic pipelines instead of clean query concepts, reducing overall retrieval accuracy and agent success rates without raising any loud errors.

## Suggested Fix
Make the Regex case-insensitive by prepending `(?i)`.
```rust
Regex::new(r"(?i)\b(before|preceding|previously|prior|earlier|ago|last|after|following|subsequently|later|next|recent|recently|latest|newest|today|now|week|weeks|month|months|year|years|day|days|hour|hours|minute|minutes|second|seconds)\b").unwrap()
```
