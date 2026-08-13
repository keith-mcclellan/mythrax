---
labels: bug, agent-found
---
## Medium Severity Issue in `mythrax-core/src/db/crud_operations.rs`

**Line Number:** 2918
**Scenario:** If `rows` is empty despite the database query, `pop().unwrap()` will panic.
**Suggested Fix:** Use `let Some(row) = rows.pop() else { return Ok(()); };`
