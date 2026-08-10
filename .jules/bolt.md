## 2024-05-19 - N+1 Query in Batch IDF Updates
**Learning:** Found an N+1 query vulnerability in `update_idf_index_batch_db` within `mythrax-core/src/db/crud_operations.rs` where an entire loop of episodes was iterated over, performing individual ID-based queries per iteration. When running batch updates, this scaled database round-trips linearly.
**Action:** Replaced iterative looping queries with single bulk queries (`SELECT * FROM type::record('episode', $ids);`). Avoid running DB queries within iterators/loops. Always favor bulk array/IN-clause operations where memory permits.
