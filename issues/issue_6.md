# Bug: Unhandled unwrap on Session ID causing panic
**Labels**: bug, agent-found

**File**: `mythrax-core/src/db/search_pipeline.rs`
**Line**: 2244
**Severity**: High

**Scenario**:
A panic occurs at `let sess = c.session_id.as_ref().unwrap();`. Even though the closure checks `c.session_id.is_none() || { ... }`, if `.is_none()` is false, `unwrap()` is called. This is safe logic in standard short-circuiting, however, `unwrap()` is generally frowned upon. A worse offender is `parse_results(keyword_resp_res.unwrap(), false)?` on line 1986 which has no such protection and will panic if `keyword_resp_res` is `None`.

**Suggested Fix**:
Use `if let Some(sess) = c.session_id.as_ref()` for the closure to avoid `unwrap()`. Handle `keyword_resp_res` safely by matching or replacing `unwrap()`.