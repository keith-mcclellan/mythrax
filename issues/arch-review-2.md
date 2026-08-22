# Adversarial Architecture Review: AST Extraction truncation context hallucination risk

Labels: architecture-review, adversarial

**Finding**: In `mythrax-core/src/cognitive/ast.rs`, the symbol extraction functions hardcode the `end_line` calculation as `(idx + 15).min(lines.len())`, which truncates longer code blocks.

**Current Assumption**: 15 lines is enough to capture context, and truncation will not severely impact downstream tasks.

**Attack Scenario**: An adversary writes a deliberately misleading malicious code block that starts innocuously for 15 lines, and then contains obfuscated harmful code on line 16. The cognitive pipeline indexes the innocent summary, but the full file execution triggers the malware, while the AI agent hallucinates that the code is safe based on the truncated AST memory.

**Blast Radius**: Code context poisoning, potentially leading the agent to execute or approve malicious payloads.

**Recommended Structural Change**: Use a real AST parser (like `tree-sitter`) to extract complete structural boundaries (function bodies, struct blocks) rather than an arbitrary 15-line limit.