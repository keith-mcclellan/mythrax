# Single Point of Failure: Database Persistent Lock Retry Loop

**Finding**: The system uses a "Persistent Lock Retry Loop" (up to 9/10 attempts, 500ms sleep) to manage RocksDB and SurrealKV exclusive file locks during startup and reconnection.

**Current Assumption**: Lock contention is transient (e.g., brief multi-process overlaps or rapid restarts) and will organically resolve within the 4.5 - 5.0 second retry window without requiring manual intervention or lock-breaking logic.

**Attack Scenario**: An adversarial input or unexpected OOM kill causes the daemon to crash forcefully *without* releasing the RocksDB/SurrealKV file lock. When the daemon attempts to auto-restart (or is spawned by a client), it enters the retry loop. Since the previous process died ungracefully, the lock is permanently orphaned and will never be released within the retry window.

**Blast Radius**: **Total Persistent Denial of Service.** The daemon fails to boot entirely. All memory, cognitive capabilities, and model routing are offline. Clients attempting to auto-spawn the daemon will continually time out. Recovery requires manual user intervention to find and delete the orphaned lock files in the hidden `~/.mythrax/db` directory.

**Recommended Structural Change**: Implement a lock-staleness check or a distributed lock manager. If a lock is held, verify if the holding PID is still alive (using OS-level checks like `kill -0` or `proc` FS). If the holding process is dead, safely break the lock and initiate a DB consistency check/WAL replay before proceeding.

Tags: `architecture-review`, `adversarial`
