---
title: Panic unwrapping DB record ID
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/synthesis.rs`
**Line:** 1644

**Scenario:** When retrieving `matched.id` as a reference, unwrapping it directly risks a panic if the ID is missing/None in the returned struct.

**Severity:** High

**Suggested Fix:** Use `.map(|s| s.strip_prefix("wisdom:").unwrap_or(s)).unwrap_or("")` instead of direct `.unwrap()`.
