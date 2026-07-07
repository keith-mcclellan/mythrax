# Thread-Safe WAL and SQLite/RocksDB Content Contention

**Tags:** `architecture-review`, `adversarial`

**Finding:** The WAL implementation relies on a file-lock retry loop (up to 9/10 attempts) to resolve concurrent DB access contention.
**Current Assumption:** A 500ms sliding window and retry loop with backoff can adequately resolve persistent file lock contention.
**Attack Scenario:** High-concurrency ingestion by multiple parallel agents exhausts the 9-attempt retry limit. The WAL falls behind, and subsequent read/write operations experience cascading timeouts and failures.
**Blast Radius:** Data loss during crashes (uncommitted WAL) and severe API latency rendering the daemon completely unresponsive.
**Recommended Structural Change:** Replace the file-lock retry loop with a dedicated single-writer connection pool or message queue for DB writes to eliminate cross-process file lock contention entirely.
