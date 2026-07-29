# Security Advisory Report

## 1. [Critical] Hardcoded Secret
- **File:** ./mythrax-core/src/api.rs:848
- **Details:** Hardcoded secret found.
- **Remediation:** Use environment variables or a secrets manager.
- **Estimated Effort:** Low

## 2. [Critical] Hardcoded Secret
- **File:** ./mythrax-core/src/mcp_routes/arbor_handlers.rs:392
- **Details:** Hardcoded secret found.
- **Remediation:** Use environment variables or a secrets manager.
- **Estimated Effort:** Low

## 3. [Critical] Hardcoded Secret
- **File:** ./mythrax-core/tests/domain_cognitive.rs:5551
- **Details:** Hardcoded secret found.
- **Remediation:** Use environment variables or a secrets manager.
- **Estimated Effort:** Low

## 4. [Critical] Hardcoded Secret
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:1800
- **Details:** Hardcoded secret found.
- **Remediation:** Use environment variables or a secrets manager.
- **Estimated Effort:** Low

## 5. [Critical] Hardcoded Secret
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:218
- **Details:** Hardcoded secret found.
- **Remediation:** Use environment variables or a secrets manager.
- **Estimated Effort:** Low

## 6. [Critical] Hardcoded Secret
- **File:** ./mythrax-core/tests/domain_hooks_models.rs:302
- **Details:** Hardcoded secret found.
- **Remediation:** Use environment variables or a secrets manager.
- **Estimated Effort:** Low

## 7. [Critical] Hardcoded Secret
- **File:** ./mythrax-core/tests/domain_hooks_models.rs:359
- **Details:** Hardcoded secret found.
- **Remediation:** Use environment variables or a secrets manager.
- **Estimated Effort:** Low

## 8. [Critical] Hardcoded Secret
- **File:** ./mythrax-core/tests/domain_hooks_models.rs:860
- **Details:** Hardcoded secret found.
- **Remediation:** Use environment variables or a secrets manager.
- **Estimated Effort:** Low

## 9. [Critical] Hardcoded Secret
- **File:** ./mythrax-core/tests/domain_hooks_models.rs:980
- **Details:** Hardcoded secret found.
- **Remediation:** Use environment variables or a secrets manager.
- **Estimated Effort:** Low

## 10. [Critical] Hardcoded Secret
- **File:** ./mythrax-core/tests/domain_e2e_harness.rs:649
- **Details:** Hardcoded secret found.
- **Remediation:** Use environment variables or a secrets manager.
- **Estimated Effort:** Low

## 11. [Critical] Secret in Git History
- **File:** Commit: e2ab8f7f64ef0a793a2dd31cc79497a39381a018:0
- **Details:** Potential secret found in git history at commit e2ab8f7f64ef0a793a2dd31cc79497a39381a018
- **Remediation:** Rewrite git history to remove the secret or rotate the credential immediately.
- **Estimated Effort:** High

## 12. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/src/main.rs:101
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 13. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/main.rs:305
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 14. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/src/main.rs:1611
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 15. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/store.rs:466
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 16. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/store.rs:611
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 17. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/store.rs:615
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 18. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/daemon.rs:921
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 19. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/daemon.rs:922
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 20. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/daemon.rs:943
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 21. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/daemon.rs:944
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 22. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/bench/runner.rs:171
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 23. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/bench/runner.rs:1069
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 24. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/mcp_routes/vault_handlers.rs:420
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 25. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/mcp_routes/vault_handlers.rs:425
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 26. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/src/mcp_routes/arbor_handlers.rs:331
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 27. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/src/mcp_routes/arbor_handlers.rs:336
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 28. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/cognitive/harvest.rs:281
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 29. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/cognitive/harvest.rs:321
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 30. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/cognitive/harvest.rs:330
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 31. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/cognitive/harvest.rs:350
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 32. [High] Unsafe Rust Block
- **File:** ./mythrax-core/src/bin/inspect_failed_query.rs:41
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 33. [High] Unsafe Rust Block
- **File:** ./mythrax-core/scratch/runner_234.rs:155
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 34. [High] Unsafe Rust Block
- **File:** ./mythrax-core/scratch/runner_232.rs:155
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 35. [High] Unsafe Rust Block
- **File:** ./mythrax-core/scratch/runner_241.rs:160
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 36. [High] Unsafe Rust Block
- **File:** ./mythrax-core/scratch/runner_241.rs:535
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 37. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:31
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 38. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:270
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 39. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:567
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 40. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:668
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 41. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:804
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 42. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:1017
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 43. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:1096
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 44. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:1312
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 45. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:1438
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 46. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:1541
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 47. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:1684
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 48. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:1895
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 49. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:2246
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 50. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:2300
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 51. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:2596
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 52. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:2627
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 53. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:2697
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 54. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:2739
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 55. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:3066
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 56. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:3224
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 57. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:3697
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 58. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:3717
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 59. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:3869
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 60. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:4256
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 61. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:4489
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 62. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:4865
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 63. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:4917
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 64. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:5031
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 65. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:5220
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 66. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:5373
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 67. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:5438
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 68. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:5539
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 69. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:5671
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 70. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:5679
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 71. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:5724
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 72. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:6034
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 73. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:6370
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 74. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:6438
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 75. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:6458
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 76. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:6488
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 77. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:6502
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 78. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:6514
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 79. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_cognitive.rs:6562
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 80. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:439
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 81. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:541
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 82. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:974
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 83. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:1098
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 84. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:1593
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 85. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:1667
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 86. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:1773
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 87. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:1835
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 88. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:1943
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 89. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:1981
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 90. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:2669
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 91. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:2749
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 92. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:2796
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 93. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:2855
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 94. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_search_retrieval.rs:3169
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 95. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:230
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 96. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:237
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 97. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:241
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 98. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:248
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 99. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:252
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 100. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:257
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 101. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:1935
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 102. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:2000
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 103. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:2050
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 104. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:2193
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 105. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:2305
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 106. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_vault_storage.rs:2403
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 107. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:80
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 108. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:102
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 109. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:106
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 110. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:111
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 111. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:205
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 112. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:293
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 113. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:303
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 114. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:319
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 115. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:326
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 116. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:360
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 117. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:477
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 118. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:1496
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 119. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:1703
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 120. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2013
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 121. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2026
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 122. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2205
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 123. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2214
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 124. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2223
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 125. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2252
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 126. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2342
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 127. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2547
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 128. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2552
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 129. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2556
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 130. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2563
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 131. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2567
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 132. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2572
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 133. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2623
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 134. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2667
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 135. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2741
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 136. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2872
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 137. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:2979
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 138. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:3138
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 139. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:3210
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 140. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:3380
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 141. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:3460
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 142. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:3648
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 143. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:3719
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 144. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:3815
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 145. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:4673
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 146. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:4775
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 147. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_legacy_aggregators.rs:4888
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 148. [High] Unsanitized Input in Command
- **File:** ./mythrax-core/tests/domain_hooks_models.rs:184
- **Details:** Potential unsanitized input passed to std::process::Command.
- **Remediation:** Sanitize external inputs or use strict allowlists before passing to Command.
- **Estimated Effort:** Medium

## 149. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_hooks_models.rs:447
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 150. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_hooks_models.rs:528
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 151. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_hooks_models.rs:656
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 152. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_hooks_models.rs:675
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 153. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_e2e_harness.rs:33
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 154. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_e2e_harness.rs:345
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low

## 155. [High] Unsafe Rust Block
- **File:** ./mythrax-core/tests/domain_e2e_harness.rs:765
- **Details:** Unsafe block missing SAFETY: documentation. Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.
- **Remediation:** Add a SAFETY: comment explaining why the unsafe block is memory-safe.
- **Estimated Effort:** Low
