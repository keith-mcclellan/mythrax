Title: Test coverage gaps for public functions
Labels: bug, agent-found

Severity: Medium
Scenario: The following public functions in `mythrax-core` have no corresponding test in the eval or test suite. Unnoticed regressions could be introduced.
Suggested Fix: Add unit tests for the untested public functions.

List of untested functions (partial list):
- mythrax-core/src/vault/ingestion.rs:378: resolve_scope_from_path
- mythrax-core/src/vault/ingestion.rs:465: extract_scope_from_log
- mythrax-core/src/vault/organization.rs:19: from_category_str
- mythrax-core/src/vault/organization.rs:35: slugify_title
- mythrax-core/src/vault/operations.rs:198: stringify_record_id
- mythrax-core/src/vault/watcher.rs:42: is_ignored
- mythrax-core/src/vault/watcher.rs:59: queue_write
- mythrax-core/src/vault/watcher.rs:1216: slugify
- mythrax-core/src/vault/markdown.rs:145: format_arbor_wiki_node_markdown
- mythrax-core/src/vault/distillation.rs:59: is_edit_tool
- mythrax-core/src/vault/distillation.rs:70: has_edit_calls
- mythrax-core/src/vault/distillation.rs:76: has_tool_calls
- mythrax-core/src/vault/distillation.rs:82: chunk_transcript
- mythrax-core/src/vault/distillation.rs:124: enforce_symbol_integrity
