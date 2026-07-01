# Finding: Daemon Panic on Missing LLM `source_skills`

**Finding:** `mythrax-core/src/cognitive/meta_skill.rs` unconditionally unwrap fields from LLM-generated JSON, such as `sug["source_skills"]` and `sug["similarity"]`.

**Current Assumption:** The LLM will strictly adhere to the requested schema.

**Attack Scenario:** An adversarial or unexpected runtime input causes the LLM to output malformed JSON or miss fields.

**Blast Radius:** The intelligence daemon panics and crashes, creating a denial of service for the memory background pipeline.

**Recommended Structural Change:** Adopt strict type-checking on incoming JSON using `serde_json` struct deserialization instead of dynamic `.unwrap()` traversal on `Value` maps.

Labels: `bug`, `agent-found`, `architecture-review`, `adversarial`
