# Architecturally Dishonest Evals (Happy-Path Only)

**Finding** -> The `evals/` framework only implements the SWE-bench Verified coding dataset (`evals/swebench/eval.sh`), evaluating code completion tasks without adversarial checks.
**Current Assumption** -> Code correctness on SWE-bench validates the architecture's fitness and safety.
**Attack Scenario** -> The system passes all SWE-bench tests but fails instantly when presented with a jailbreak payload or prompt injection because no adversarial evals exist in the test suite.
**Blast Radius** -> False confidence in system security; highly vulnerable to attacks in the wild.
**Recommended Structural Change** -> Integrate adversarial evaluation frameworks into `evals/` and enforce a failing CI pipeline if jailbreaks or scope boundary violations succeed.


**Labels**: `architecture-review`, `adversarial`

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
