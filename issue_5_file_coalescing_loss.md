# 💥 SPOF & Data Loss: 500ms File Watcher Coalescing Window

**Tags:** `architecture-review`, `adversarial`

**Requires ADR response to close.**

**Finding:**
The 500ms Obsidian vault watcher coalescing window introduces a silent data-loss vulnerability and race condition for rapid automated file edits.

**Current Assumption:**
*Architecture.md* assumes that "events are coalesced over a 500ms sliding window before being committed to the database" to prevent high-frequency write cascades. It assumes edits within 500ms are redundant or part of the same human typing sequence.

*What assumption does this break if it's wrong?* It assumes file modification speeds are bound by human typing. In an AI agent environment, a tool execution (like a Python script writing output, followed by an agent writing a thought, followed by a linter correcting the file) can easily generate 3 discrete, semantically distinct file states within 100ms.

**Attack Scenario:**
An agent runs a script that writes temporary results to a file, reads them, and immediately overwrites the file with a summarized conclusion—all in 150ms. The Obsidian watcher coalesces these events, dropping the intermediate state. If the agent later needs to recall the exact intermediate results (which were crucial for debugging), they are entirely absent from the episodic memory in SurrealDB.

**Blast Radius:**
Silent, irrecoverable loss of high-speed intermediate agent states. This undermines the core goal of "Short-Term Context Recall & Compaction Recovery," causing agents to hallucinate past steps because the data was coalesced away.

**Recommended Structural Change:**
Do not rely on generic file-watcher coalescing for system-critical telemetry.
1. Implement a structured, push-based API where agents explicitly emit `StateTransition` events directly to the WAL, rather than relying on polling/watching flat files.
2. If file watching is strictly necessary, track filesystem inode generations or cryptographic hashes rather than temporal windows to ensure all distinct states are captured, regardless of speed.
