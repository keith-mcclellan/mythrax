# Bug: Unhandled Option/Result panics on keyword search response
**Labels**: bug, agent-found

**File**: `mythrax-core/src/db/search_pipeline.rs`
**Line**: 1986
**Severity**: High

**Scenario**:
A panic will occur in `keyword_resp_res.unwrap()` if the keyword search response fails or returns `None`. An external failure in the search service propagates into a full panic, causing DoS on the querying agent.

**Suggested Fix**:
Handle the error appropriately. If `keyword_resp_res` is an Option/Result, use `if let Some(res) = keyword_resp_res` or `keyword_resp_res?` to return an error properly.