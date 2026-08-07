## 2024-08-07 - Fix Unhandled Panics in DB Modules
**Vulnerability:** Found unhandled `.unwrap()` calls in `mythrax-core/src/db/crud_operations.rs` parsing temporal relation data and in `mythrax-core/src/db/search_pipeline.rs` reading search query responses.
**Learning:** These panics were reachable if malformed data existed in the database, potentially leading to denial of service via application crash, which shouldn't happen.
**Prevention:** Defensive coding should be standard: always handle `Option` and `Result` types gracefully using `match`, `if let`, or combinators (`and_then`) instead of assuming data correctness.
