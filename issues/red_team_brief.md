# Red Team Architecture Brief

## Single Shared Static Auth Token (API Gateway)
Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change
**Finding**: API Gateway and MCP rely on a single shared static token (X-Mythrax-Token).
**Current Assumption**: The local environment is trusted, and agents will not leak credentials.
**Attack Scenario**: Prompt injection in a processed document instructs the agent to echo the static token to an external attacker-controlled server.
**Blast Radius**: Complete compromise of the daemon and all agent orchestration, allowing arbitrary command execution on the host machine.
**Recommended Structural Change**: Implement scoped, short-lived JWTs or mutual TLS (mTLS), and strictly enforce agent network egress scope boundaries.

## Mutex Poisoning in Size-Rolling Logger
Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change
**Finding**: log_writer.rs uses self.inner.lock().unwrap().
**Current Assumption**: Thread panics will not occur while holding the logger lock.
**Attack Scenario**: A malformed payload causes a thread to panic during a log write. The mutex is poisoned.
**Blast Radius**: Complete daemon crash (DoS) on the next log write attempt, with no graceful degradation path.
**Recommended Structural Change**: Use a lock-free asynchronous logging channel (e.g., tracing-appender) to decouple logging from request threads.

## Evident Adversarial Eval Gaps in SWE-bench
Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change
**Finding**: The evals/swebench/ framework tests only happy paths for coding tasks.
**Current Assumption**: Standard coding benchmarks adequately validate agent safety and behavioral constraints.
**Attack Scenario**: An adversary submits a PR containing hidden prompt injections. The eval framework passes, but the agent executes out-of-scope commands in production.
**Blast Radius**: Architecturally dishonest validation leading to unbounded recursion or host compromise.
**Recommended Structural Change**: Integrate adversarial eval datasets that actively test resilience against prompt injection and scope boundary enforcement.

## High Coupling in Graph Iteration
Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change
**Finding**: Iterative graph operations rely on string cloning and HashMap lookups, tightly coupling algorithmic logic to string representations.
**Current Assumption**: The memory graph will remain small enough that string hashing overhead is negligible.
**Attack Scenario**: The system scales 10x, accumulating hundreds of thousands of nodes. Iterations become CPU-bound and block the async executor.
**Blast Radius**: Daemon latency spikes and API gateway starvation.
**Recommended Structural Change**: Map node identifiers to contiguous integers and use flat vectors (CSR format) for tight inner loops.

## 18-Month Scaling Bottlenecks (Top 3 Decisions)
Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change
**Finding**: Three design decisions will mandate re-architecture at 10x scale: 1) O(1) String Hash lookups in inner loops, 2) Hardcoded symbol limit of (idx + 15) truncating code context, 3) Single-port monolithic API gateway.
**Current Assumption**: Data volumes and context windows will remain within small single-user limits.
**Attack Scenario**: A collaborative workspace ingests thousands of complex documents. The system exhausts memory, truncates critical code context, and hits gateway starvation limits.
**Blast Radius**: Cascading daemon lockups and severe context hallucination.
**Recommended Structural Change**: Re-architect graph algorithms to integer-based arrays, eliminate arbitrary hardcoded line limits, and decompose the gateway for multi-tenant scalability.

## Unbounded False-Positive Clustering
Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change
**Finding**: signals.rs clustering fallback erroneously multiplies Jaccard similarity by 5.0.
**Current Assumption**: Token intersection requires a massive artificial boost to be useful.
**Attack Scenario**: Generic tokens overlap slightly across disparate memories; the 5.0 multiplier inflates similarity to 1.0.
**Blast Radius**: The graph collapses into a single monolithic cluster, blowing up the LLM context window during cognitive synthesis.
**Recommended Structural Change**: Remove the hardcoded 5.0 multiplier and calibrate DBSCAN epsilon dynamically.

## NaN Generation in BM25 Scoring
Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change
**Finding**: retrieval/bm25.rs Okapi BM25 scoring can return NaN (Log(0)) or negative IDF values.
**Current Assumption**: Global document count will always exceed document frequency.
**Attack Scenario**: Edge case ingestion causes df_t to equal or exceed self.n.
**Blast Radius**: Search scores become NaN, destroying the hybrid search index and blinding the agent.
**Recommended Structural Change**: Clamp minimum IDF values to a positive epsilon.
