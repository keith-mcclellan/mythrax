# Finding: SWE-bench Eval Framework Only Tests Happy Paths

**Current Assumption:**
"This harness measures the end-to-end coding performance of the host developer agent... scored by the dataset authors' own official harness ('% Resolved')." (`evals/swebench/README.md`). It is assumed that `% Resolved` is a sufficient metric for system reliability and agent safety.

**Attack Scenario:**
The system overfits to SWE-bench's `% Resolved` metric (which only validates standard bug fixes on well-formed codebases) while silently regressing on its ability to handle malformed, recursive, or actively adversarial code changes. LLM-based systems that only test happy paths are architecturally dishonest, failing to test the agent scope boundaries in the orchestration design.

**Blast Radius:**
Deploying an agent that scores 95% on SWE-bench but fails catastrophically (e.g., unbounded loops, data destruction, unbounded recursion risk) when encountering edge cases or hostile environments in a real-world codebase.

**Recommended Structural Change:**
Extend the eval harness to include a "Chaos/Adversarial Track" that injects malformed JSON, recursive Git worktree structures, or conflicting prompt instructions to explicitly measure graceful degradation, boundary enforcement, and failure recovery, not just resolution rate.