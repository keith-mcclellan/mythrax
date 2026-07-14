# Mythrax Workspace Rules

## Parallel Test Execution
- **Mandate**: Always run test suites in parallel using `cargo nextest run` or the `cargo t` alias.
- **Why**: The default `cargo test` runs test suites sequentially which triggers database lock contentions and significantly slows down the E2E verification loop.
- **Fast Mocking**: Always specify the `MYTHRAX_TEST_MOCK=1` environment variable when running unit and integration tests (e.g., `MYTHRAX_TEST_MOCK=1 cargo nextest run`). Do NOT specify `--features mlx` for mock tests, to avoid heavy Metal compiler/JIT loading and compilation overhead. If JIT compile errors or startup hangs occur on macOS, ensure `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` is exported.

## Core System Goals & Objectives

To fulfill its role as a persistent, autonomous sidecar intelligence companion, Mythrax commits to five fundamental objectives:

1. **Short-Term Context Recall & Compaction Recovery:** Provide immediate short-term retrieval for active agents operating with large context windows. Memory compaction must preserve the granular sequence of raw turns (user inputs, assistant thoughts, tool outputs) so agents can review their immediate past steps and avoid forgetting loops.
2. **Project-Level Memory (Insights):** Build high-cohesion, project-specific knowledge representations (`wiki_node` / clusters) so that multiple agents or sequential sessions on the same codebase share operational constraints and context.
3. **Cross-Project Global Memory (Wisdom):** Maintain a durable, global partition (`wisdom`) for general guidelines, coding practices, user preferences, and architectural rules that apply universally across workspaces (e.g. general design principles).
4. **Forged Knowledge & Skill Integration:** Enable raw reference assets (like PDFs, specs, and papers) and composed agent strategies (e.g. chaining `spec-builder`, `loop-builder`, and reviewers) to be dynamically injected via RAG into active context windows on-demand.
5. **Resource-Efficient Memory Brokerage:** Optimize token footprint and compute overhead using local models (`mlx-community/Qwen3.6-35B-A3B-4bit`) for text embeddings, token budget management, and code generation.

## Mythrax 6-Signal Unified Retrieval (v2.5.2)
- **6 Retrieval Signals**: Combine Vector Similarity, BM25 (FTS) Relevance, Concept Spreading Activation, STM Working Memory Injection, Temporal Neighbor Expansion, and Gaussian Temporal Proximity.
- **Concept Spreading Activation**: Attenuates scores as it traverses `relates_to` edges cross-scope (`anchor_sim * edge_confidence * 0.5`).
- **STM Working Memory Injection**: Query active STM KV pairs, compare query embedding with values, and inject matching entries as high-priority candidates (`tier: "working"`).
- **Temporal Neighbors**: Expand candidates by traversing `followed_by` temporal relationship edges.
- **Gaussian Temporal Proximity decay**: Replace hard time-demotions with \(\exp(-\Delta t^2 / 2\sigma^2)\) scoring, default \(\sigma = 168h\).
- **Active VRAM Model Broker**: Dynamic coordination unloads embedding models before loading reranking/inference models to prevent OOM.
- **Cross-Scope Graduation**: Promotes project-scoped insights and procedural episodes (365-day half-life, 500-node LRU cap) to generalized global wisdom rules upon convergence across multiple projects (cosine \(\ge 0.85\)).

## Thinking and Writing Concision (docs:write-concisely)
- **Mandate**: All agents (including subagents) MUST apply the `/docs:write-concisely` Strunk & White principles to all outputs, including **inner thoughts (thinking blocks)**, planning documents, and formal vault markdown files.
- **Rules**:
  - **Omit needless words**: Be direct, clean, and concise. Eliminate throat-clearing, introductory fillers, and repetitive summaries.
  - **Use active voice**: Make the subject perform the action to keep descriptions vigorous.
  - **Use positive form**: Make definite assertions instead of evasive/negative qualifiers.
  - **Use definite, specific, concrete language**: Avoid vague generalizations.
  - **Keep paragraphs focused**: Stick to one topic per paragraph.

