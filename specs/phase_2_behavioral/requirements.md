# Requirements: Phase 2 Active Learning & Process Standards (v0.9.x)

This document defines the requirements for Phase 2 of the Mythrax 0.9.x features.

---

## Problem

1.  **Conversational Amnesia**: When a developer corrects the agent's actions (e.g. pointing out a missed step, an invalid assumption, or a coding mistake), the agent corrects course in the current turn but has no mechanism to persist this correction long-term. As a result, the same mistake is frequently repeated in future sessions.
2.  **Visual and Aesthetic Bleed**: Visual styling conventions (such as CSS frameworks, design tokens, button layouts, and color palettes) are highly project-specific. If styling rules are generalized and applied globally, it leads to aesthetic contamination across projects (e.g., applying React/Tailwind rules to a minimalist Vanilla CSS or Rust CLI project).
3.  **Compaction-Driven Amnesia**: Standard context compaction summarizes past turns to fit within the context window. However, this summarization often strips out critical active constraints, temporary credentials, or session-specific compiler blockers, causing the agent to lose its "attention anchor" midway through a task.

---

## Outcome

*   **Continuous Correction Learning**: The agent automatically detects conversational corrections, dispatches an async critic to synthesize a long-term `WisdomRule`, writes it to Obsidian and SurrealDB, and applies it immediately in the next turn.
*   **Scoped Styling & Global Processes**: Project-specific aesthetic rules are strictly locked to their local project scope, while high-utility procedural guidelines are synthesized, promoted, and made available globally.
*   **Compaction-Resistant Anchors**: Flagged constraints and session parameters are protected from the compaction engine's summarizer, ensuring they remain active and verbatim in the prompt at all times.

---

## User Value

*   **Self-Correcting Assistant**: The agent gets smarter with every correction and never repeats the same mistake twice.
*   **Perfect Aesthetic Isolation**: The agent automatically adheres to Vanilla CSS, Tailwind, or custom styles without cross-project bleeding.
*   **Bulletproof Task Focus**: The agent retains critical rules and compiler blockers verbatim across long, compacted sessions, eliminating mid-task derailments.

---

## In Scope

*   **Conversational Mistake Learning**:
    *   Implement a trigger hook that scans user inputs for correction phrases (e.g. "you forgot", "incorrect", "that's wrong").
    *   Create an asynchronous background critic task that extracts a structured `WisdomRule` from the dialog history.
    *   Write the extracted rule as a markdown file in `vault/wisdom/dynamic/` and index it in SurrealDB with `utility_score = 50.0` and the active project scope.
*   **Project-Isolated Aesthetics vs. Global Standards**:
    *   Modify the dreaming and compaction loops in `cognitive/synthesis.rs` to classify rules.
    *   Classify rules into `"Aesthetic"` (visuals, layouts, CSS) or `"Procedural"` (testing, git, compiler).
    *   Scope-lock Aesthetic rules to the active project scope.
    *   Promote Procedural rules to the Global Wisdom Tier (`~/.mythrax/global/wisdom/permanent/`).
*   **Implicit Attention Anchors**:
    *   Implement a parser that extracts flagged attention blocks (e.g., starting with `@attention-anchor` or `[ANCHOR: ...]`) from the prompt history.
    *   Update the compaction engine to exclude these anchor blocks from the summarization prompt.
    *   Append the extracted anchor blocks verbatim to the end of the compacted context.

---

## Out of Scope

*   Resolving conflicting dynamic rules in this phase (deferred to Phase 4).
*   Supporting real-time notifications in the developer's IDE when a rule is harvested.
*   Interactive editing of harvested rules within the chat interface.

---

## Inputs

*   **For Mistake Learning**: The user's input prompt, the session message history (dialogue window), and any command error logs.
*   **For Style Isolation**: Completed rules or episodes analyzed during synthesis/compaction.
*   **For Attention Anchors**: The active prompt and context strings containing flagged anchor markers.

---

## Outputs

*   **For Mistake Learning**: A new `.md` file written to `vault/wisdom/dynamic/` and a corresponding `wisdom` record in SurrealDB.
*   **For Style Isolation**: Scope-bounded aesthetic rules and generalized global procedural rules.
*   **For Attention Anchors**: A compacted prompt string where the flagged anchor blocks are appended verbatim at the end of the text.

---

## Constraints

*   **Zero Latency Impact**: The dynamic rule extraction must run asynchronously in the background so it does not block or slow down the agent's turn response.
*   **Anchor Integrity**: Flagged attention anchors must be carried forward **verbatim** without any modification or summarization by the LLM.
*   **Strict Scope Binding**: Aesthetic rules must never be returned in queries where the active scope does not match the rule's bound scope.

---

## Risks and Edge Cases

*   **False Positive Triggers**: The user might say "you forgot" in an unrelated context (e.g., "don't worry if you forgot that API key").
    *   *Remedy*: The critic LLM serves as a validator. If it reviews the dialog and determines no mistake or rule was actually made, it returns an empty response and no rule is created.
*   **Compaction of Anchors**: If an anchor is extremely long, it could still consume a large part of the context window.
    *   *Remedy*: We will recommend a best practice of keeping anchors concise ($<200$ characters) and log a warning if an anchor exceeds 1k characters.

---

## Acceptance Criteria

*   **[ ] AC-2.1 (Correction Hook)**: Scanning a user prompt containing "you forgot to run tests" successfully triggers the mistake detection handler.
*   **[ ] AC-2.2 (Wisdom Extraction)**: When a correction is triggered, the background critic correctly extracts a structured `WisdomRule` JSON containing the target pattern, action to avoid, causal explanation, and remedy. The rule is written to `vault/wisdom/dynamic/` and saved to SurrealDB with `utility: 50.0` and the active scope.
*   **[ ] AC-2.3 (Rule Classification)**: During the synthesis/compaction cycle, the system classifies a CSS styling rule as `"Aesthetic"` and a cargo test rule as `"Procedural"`.
*   **[ ] AC-2.4 (Scope Isolation & Promotion)**: Aesthetic rules are saved with the active project scope (e.g., `"mythrax"`) and are never returned when querying from a different project scope. Procedural rules are successfully promoted to the global permanent tier.
*   **[ ] AC-2.5 (Attention Anchors)**: Running compaction on a prompt containing `@attention-anchor Keep active_session_token = 8899` results in a compacted prompt that contains the exact string `"Keep active_session_token = 8899"` verbatim at the end.
