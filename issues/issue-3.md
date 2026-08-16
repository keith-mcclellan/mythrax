# Eval framework fails to test adversarial inputs and boundary conditions

**Tags:** `architecture-review`, `adversarial`
**Note:** Never close this issue without a documented architectural decision record (ADR) response.

**Finding** -> Eval framework only tests happy paths, failing to validate adversarial inputs
**Current Assumption** -> The current evaluation framework in `evals/` accurately assesses system reliability, cognitive pipeline correctness, and agent orchestration.
**Attack Scenario** -> Standard path tests pass, but the system is exposed to adversarial inputs such as malformed JSON, cyclic tool dependencies, and infinite recursion during context expansion. An attacker (or faulty agent) triggers an unbounded recursive tool sequence that evades the 1,000-element sliding window cap.
**Blast Radius** -> Resource exhaustion, OOM crashes, and deadlocks in the agent orchestration and token context layers, bringing down the daemon.
**Recommended Structural Change** -> Introduce comprehensive adversarial fuzzing and red-teaming evals specifically targeting prompt injection, malformed data paths, unbounded recursion, and context window overflow limits. Systems that only test happy paths are architecturally dishonest.
