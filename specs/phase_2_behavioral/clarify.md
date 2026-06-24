# Clarify: Phase 2 Active Learning & Process Standards (v0.9.x)

This document initiates Phase 1 (Clarification) of the spec-driven development process for **Phase 2: Active Learning & Process Standards** of Project Mythrax, covering:
1.  **2.1 Conversational Mistake Learning & Auto-Wisdom Extraction**
2.  **2.2 Project-Isolated Aesthetics & Global Standards Synthesis**
3.  **2.3 Implicit Attention Anchors (Constraint Pinning)**

---

## Restated Request

Implement active learning, visual style isolation, and constraint protection capabilities for Mythrax:
*   **Conversational Mistake Learning**: Parse user inputs for correction indicators (e.g. "you forgot", "incorrect", "that's wrong"). When detected, analyze the dialog history and compile an automated, structured `WisdomRule` using the local LLM critic. Save the rule to Obsidian (`vault/wisdom/dynamic/`) and SurrealDB with a high default utility score (`utility = 50.0`) and the active project scope.
*   **Project-Isolated Aesthetics vs. Global Standards**: During memory dreaming and compaction routines (`cognitive/synthesis.rs`), classify rules into **Aesthetic** (CSS, layouts, UI tokens) or **Procedural** (TDD, compilation, git). Bind Aesthetic rules strictly to their local project scope so they never bleed into other projects. Generalize Procedural rules and promote them to the Global Wisdom Tier (`~/.mythrax/global/wisdom/permanent/`).
*   **Implicit Attention Anchors (Constraint Pinning)**: Support designating specific constraints, rules, or state parameters as **Attention Anchors** (e.g., active session rules or compile blockers). Ensure that the compaction engine programmatically bypasses summarizing these anchors, carrying them forward verbatim at the end of the compacted context to prevent information loss.

---

## Known Facts

### 1. Codebase Architecture
*   **Wisdom Schema**: Defined in `mythrax-core/src/contracts.rs` as the `WisdomRule` struct and stored in the `wisdom` table.
*   **Ingestion & Organization**: `mythrax-core/src/vault/ingestion.rs` handles parsing markdown files from the vault into database entities. `mythrax-core/src/cognitive/harvest.rs` runs DBSCAN-based skill clustering and uses the LLM to generate wisdom rules.
*   **Compactor Engine**: `mythrax-core/src/cognitive/compactor.rs` and `synthesis.rs` manage context summaries, dreaming runs, and sliding-window compression.
*   **MCP Server**: `mythrax-core/src/mcp.rs` exposes tools to the agent, including `search_memories`, `search_wisdom`, and `save_episode`.

### 2. Available Hardware/Model Constraints
*   **Local Coder Agent**: Operating on `mlx-community/Qwen3.6-35B-A3B-4bit` under `mcp-openai` at port 8080.
*   **Resources**: Strictly capped at 1 local subagent, with prompts $<1\text{k}$ tokens and swap memory monitored to stay under 4GB.

---

## Assumptions

1.  **Correction Indicator Regex**: A simple, case-insensitive substring or regex check on the user's raw input prompt can reliably trigger the mistake-learning critic without needing a heavy LLM classification step on every turn.
2.  **Critic Dialog Context**: When a correction is triggered, we can fetch the recent message log from the current session (e.g. the last 2-4 turns of prompt/response history plus any stderr from failed commands) and pass this focused subset to the local LLM critic to extract the wisdom rule.
3.  **Aesthetic vs. Procedural Classification**: The LLM critic or synthesis engine can reliably classify a rule by prompting it to return a category field (`"aesthetic"` or `"procedural"`) in its structured JSON output.
4.  **Anchor Flagging Syntax**: Developers or agents can flag attention anchors in markdown files or active prompts using a simple prefix syntax like `[ANCHOR: ...]` or `@attention-anchor`. The compactor can then scan the context for these blocks and extract them before running the summarization prompt.

---

## Ambiguities

1.  **Asynchronous Criticism vs. Synchronous Interception**: Does the mistake-learning critic run synchronously and block the current turn, or does it run asynchronously in the background?
    *   *Resolution*: The extraction runs asynchronously in a background task (using Tokio's `tokio::spawn`) to prevent introducing latency to the developer's chat loop. Once the rule is written to the vault and saved to SurrealDB, it becomes active for the next turn.
2.  **Determining the Active Project Scope for Dynamic Rules**: When a mistake rule is extracted, how do we know which project scope to tag it with?
    *   *Resolution*: We use the same auto-scoping utility developed in Phase 1. It senses the current path/working directory and maps it to the corresponding scope name.
3.  **Decay of Dynamic Rules**: Do dynamic rules learned from corrections decay like other memories?
    *   *Resolution*: Yes, they start with a high utility score (`50.0`), but if they are not retrieved or verified in subsequent turns, their utility will decay in Phase 4.

---

## Tradeoffs

*   **Dynamic Rule Extraction Cost**: Dispatching an LLM call on every correction incurs a small local model invocation cost. However, because it only triggers on specific semantic correction phrases, it is highly targeted and will run rarely (only when the user corrects the agent), keeping local CPU/GPU utilization low.
*   **Regex Prompt Parsing vs. LLM Guardrails**: Parsing prompts with regex/substrings is extremely fast ($<1\text{ms}$) but might suffer from false positives (e.g. "don't worry, it wasn't a mistake"). This is acceptable because the LLM critic itself serves as a guardrail—if the critic analyzes the dialog and finds no actual error or rule to extract, it will simply return an empty array, resulting in no new files or database pollution.

---

## Blocking Questions

*   **None**. All boundaries are clear. We will proceed to Phase 2 (Requirements) to draft the concrete testable behaviors.
