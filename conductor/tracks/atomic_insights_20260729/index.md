# Track: Atomic Insight Itemization & Full-Pipeline Arbor Alignment

## Documents

- [Specification](./spec.md)
- [Implementation Plan](./plan.md)
- [Metadata](./metadata.json)

## Context

This track atomizes insight extraction across the full Mythrax pipeline per the Arbor paper (arXiv:2606.11926v1). Instead of producing single monolithic WikiNodes per cluster, the system will extract multiple individually-vectorized atomic items (patterns, constraints, failure modes, lessons) with content-derived embeddings.

**3 Phases:**
1. Core Atomic Extraction (struct, schema, prompt, dual-path synthesis)
2. Pipeline Propagation (compactor, wisdom graduation, distillation)
3. Raw Markdown Asset Mining (universal `.md` handling, retroactive reprocessing, WisdomRule generation)

## Adversarial CTO Review

The implementation plan received **UNCONDITIONAL APPROVAL** from the Adversarial CTO Reviewer after 2 review iterations addressing 5 specific gaps (single-episode path, outlier compaction, dual centroid removal, DB file paths, cross-scope graduation).
