# Finding: Config Injection / Crash via settings schemas

**Finding:** `mythrax-core/src/main.rs` merges configuration payloads into Antigravity settings using `.as_object_mut().unwrap()`.

**Current Assumption:** User configuration files or dynamically fetched permissions have identical base structures.

**Attack Scenario:** A user or script mistakenly injects an array instead of a base dictionary at root config paths, or removes keys.

**Blast Radius:** The application crashes during bootstrap due to panics on `as_object_mut()`, preventing successful server starts.

**Recommended Structural Change:** Traverse trees safely using `if let` and return descriptive `Result` paths.

Labels: `bug`, `agent-found`, `architecture-review`, `adversarial`
