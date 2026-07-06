---
tags: [architecture-review, adversarial]
---
# Finding: Agent Orchestration and Memory Ingestion Vulnerabilities

## Current Assumption
The pre-compaction hook parses transcripts verbatim without dropping details to ingest memory, and agent capabilities are bounded. It assumes the ingested inputs are benign and not adversarial.

## Attack Scenario
An attacker injects an adversarial string into the environment (e.g. into the codebase, a pulled dependency, or a file read by the vault watcher). The agent reads this adversarial string, and the output is logged to the transcript. During compaction, the hook ingests this maliciously crafted payload into the long-term memory graph. If the payload is crafted to mimic agent command syntax or structural markers, future memory retrievals will pass the adversarial prompt into the context window, causing the agent to execute unauthorized instructions (prompt injection) or triggering an unbounded recursive loop of parsing and execution.

## Blast Radius
Persistent corruption of the agent's long-term memory graph. Adversarial payloads persist indefinitely, causing the agent to act maliciously or enter infinite loops in future interactions without further user input.

## Recommended Structural Change
Enforce strict input sanitization and schema validation when parsing transcripts and ingesting into the vault. Establish firm scope boundaries for the agent orchestration, and implement sandboxed or bounded execution contexts for memory compaction loops to mitigate recursion. Include detection for adversarial prompts during ingestion.
