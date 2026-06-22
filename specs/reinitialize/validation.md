# Validation - Mythrax Project Reinitialization & Harness Configuration

## Acceptance Criteria Review

| Criterion | Target | Verification Method | Status |
|---|---|---|---|
| RocksDB Support | Persistent DB Cache | Save episode, restart daemon, query episode. | [ ] |
| Nomis Embeddings | Automatically calculated | Check `embedding` field in db is not null. | [ ] |
| Clean Init | Bootstrap clean system | Run `mythrax init antigravity`, check folder structures. | [ ] |
| Dynamic Config | Multiple harnesses | Run `mythrax config claude`, verify `~/.claude.json`. | [ ] |
| Auto History Ingest | Seed on config | Run `mythrax config antigravity`, confirm database records. | [ ] |
| CLI / MCP Tools | Feature-complete lifecycle | Call `bulk_ingest` and `verify` via CLI/MCP. | [ ] |
| Embedding Reprocessing | Generate missing vectors | Save episode without models, restore models, run reprocess, check embedding not null. | [ ] |
| Rust Compilation | 0 errors, 44 passing tests | Run `cargo test`. | [ ] |
| Reinitialization | Clean vault & database | Check folder sizes and record counts. | [ ] |

## Edge Cases
1. **Model Files Missing**: If model/tokenizer is not present, database should initialize without error but set `embedding = None`.
2. **Harness Not Found**: Running `mythrax config invalid_harness` should return a clean error indicating supported harness names.
3. **Empty Source Directory**: Ingesting from an empty directory should return successfully with 0 items ingested and no panic.

## Failure Modes
- ONNX runtime thread initialization error (handled by falling back to non-embedded mode).
- SQLite db file locked (for Cursor/Hermes harness ingestion, handled by opening in read-only mode).

## Final Status
- PENDING | PASS | FAIL
