# Validation

## Acceptance Criteria Review

5: | Criterion | Target | Verification Method | Status |
6: |---|---|---|---|
7: | HNSW Index | Schema update | Verify `INIT_SCHEMA` runs successfully with vector indexes. | [ ] |
8: | Wisdom Embeddings | Automatic embeddings | Check that wisdom rules are saved with vector embeddings. | [ ] |
9: | Watcher wiki Sync | Sync insights & compactions | Verify wiki files are saved in `wiki_node` database table on file creation/modification. | [ ] |
10: | Vector Search | Semantic search querying | Run search with similarity ordering. | [ ] |
11: | score Blending | Similarity-utility ranking | Verify ranking matches $S \times (0.7 + 0.3 \times U)$. | [ ] |
12: | Graph Relations | relate_nodes connections | Run deep insight search and verify related rules/insights are traversed. | [ ] |
13: | Compaction Scopes | Dynamic scope compaction | Compact all active scopes in database. | [ ] |
14: | Watcher Deletions | Delete sync | Delete markdown file and verify corresponding database record is removed. | [ ] |
15: | HTR Wisdom Injection | HTR loop guidelines | Verify HTR ideation is injected with semantically matched Wisdom Rules. | [ ] |
16: | Startup Reprocessing | Background task | Verify backend automatically triggers background reprocessing for missing embeddings. | [ ] |
17: | Code Compilation | 0 compile errors | Run `cargo test` and verify compile success. | [ ] |
18: 
19: ## Final Status
20: - PENDING
