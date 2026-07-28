---
title: Logic Bug: Potential panic due to missing values during token count calculation
labels: bug, agent-found
---

**File & Line:** `mythrax-core/src/db/backend.rs` (Various parsing of JSON bodies and length calculations in `count_text_tokens` and token budget limits)

**Minimal Reproducible Scenario:** In some edge cases within agent metric/score calculations, token counts can underflow when calculating budget remaining if `title_tokens >= remaining_budget` logic is bypassed or modified improperly, leading to a silent failure or underflow panic.

**Severity:** Medium

**Suggested Fix:** Explicitly use `saturating_sub` for unsigned integer arithmetic involving tokens and budgets.