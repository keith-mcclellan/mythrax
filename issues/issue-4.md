---
title: "Bug: Missing logic for tracking context window and off-by-one errors"
labels: ["bug", "agent-found"]
---

**File & Line:** `.agents/skills/mythrax/SKILL.md` (and related scaffold)

**Description:**
The instructions and scaffold structure do not explicitly bound or prevent off-by-one errors related to context window context gathering. If the agent loads too much data via large files, it will run out of context and potentially fail or get truncated unexpectedly without proper bounds checking.

**Minimal Reproducible Scenario:**
Load a file large enough to exceed the context window, and the agent's logic fails to trim it before sending to the LLM.

**Severity:** Medium (Limits agent applicability for large files).

**Suggested Fix:**
Add token counting bounds checks directly into the agent retrieval or extraction tools to ensure the token count never exceeds `context_limit - output_limit`.
