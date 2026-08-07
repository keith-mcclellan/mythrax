import os
import datetime

os.makedirs('issues', exist_ok=True)

issues = [
    {
        "filename": "issue_001_single_port_shared_token.md",
        "title": "Single-Port API Gateway & Shared Static Auth Token single point of failure",
        "labels": "architecture-review, adversarial",
        "finding": "The Single-Port API Gateway (port 8090) validates REST and MCP requests against a shared static auth token via `X-Mythrax-Token` and `Authorization` headers. We also observed `reqwest::Client` contention in memory, as it is reused across endpoints and tool invocations.",
        "current_assumption": "A shared HTTP client and a single unified port guarded by a static token is efficient and secure enough for local or single-tenant deployments.",
        "attack_scenario": "An attacker or compromised dependency extracts the single static `X-Mythrax-Token` (or leaks it via memory). They now have full unpartitioned access to all endpoints, including MCP tools, memory ingestion, and config overrides. Concurrently, a high-volume request flood on port 8090 can exhaust the shared `reqwest::Client` connection pool, locking out all administrative and routing capabilities simultaneously.",
        "blast_radius": "Total daemon compromise and Denial of Service. No graceful degradation path exists because all traffic flows through the single port 8090 and a single shared token.",
        "recommended_structural_change": "1) Implement fine-grained token partitioning (e.g., separate tokens for MCP tools vs. chat routing). 2) Decouple the administrative/management interface (port 8090) from the proxy/completions interface (port 8080) to allow independent QoS and failure domains."
    },
    {
        "filename": "issue_002_swebench_happy_path.md",
        "title": "Eval Framework (evals/swebench) Only Tests Functional Happy Paths",
        "labels": "architecture-review, adversarial",
        "finding": "The evaluation framework in `evals/swebench/` wraps the official SWE-bench harness, which solely verifies functional correctness on known coding issues. It lacks adversarial, robustness, or security boundary testing.",
        "current_assumption": "Passing SWE-bench benchmarks indicates the system is robust and ready for production deployment as an autonomous agent.",
        "attack_scenario": "The agent receives a task with prompt-injected instructions (e.g., hidden in a seemingly benign test case or issue description). Because the eval framework never simulates adversarial inputs or scope boundary breaches, the prompt injection succeeds, causing the agent to execute malicious commands or exfiltrate the `X-Mythrax-Token`.",
        "blast_radius": "Agent vulnerability to real-world prompt injection and scope creep, leading to unintended and potentially destructive actions on the host system.",
        "recommended_structural_change": "Introduce a dedicated adversarial evaluation harness alongside SWE-bench. This harness must simulate prompt injections, bounded recursion exhaustion, and scope boundary violations to stress-test the orchestration layer."
    },
    {
        "filename": "issue_003_unbounded_recursion_temporal_graph.md",
        "title": "Unbounded Recursion Risk in Temporal Graph Traversals (`LIMIT 50` Hop explosion)",
        "labels": "architecture-review, adversarial",
        "finding": "Temporal expansion graph traversals apply `LIMIT 50` constraints per hop level (e.g., in `mythrax-core/src/db/crud_operations.rs`). A depth-3 traversal can yield 50^3 (125,000) nodes.",
        "current_assumption": "Capping queries at `LIMIT 50` per hop is sufficient to prevent unbounded memory growth during cognitive synthesis and temporal traversal.",
        "attack_scenario": "An adversary (or misbehaving agent) creates a highly dense temporal graph by interlinking hundreds of episodic memories. When a depth-3 search traversal is triggered, the system attempts to process 125,000 nodes, exhausting memory and compute resources.",
        "blast_radius": "Denial of Service via memory exhaustion (OOM) or extreme latency bottlenecks during retrieval and clustering, crashing the single daemon instance.",
        "recommended_structural_change": "Implement an absolute global limit on the number of nodes visited during graph traversals (e.g., max 500 nodes total, regardless of depth) and introduce a visited-node cache to prune duplicate paths early."
    },
    {
        "filename": "issue_004_prompt_injection_precompact.md",
        "title": "Prompt Injection Vulnerability in Agent Orchestration (`precompact.rs`)",
        "labels": "architecture-review, adversarial",
        "finding": "Memory indicates that episodic memory hooks (e.g., `mythrax-core/src/hooks/precompact.rs`) store external inputs (user inputs, API tool results) into prompt-visible memory. If control tokens (`<|`, `|>`, backticks) are not strictly sanitized, this is an injection vector.",
        "current_assumption": "External inputs are safe to store and later inject verbatim into the context window for synthesis and compaction.",
        "attack_scenario": "An attacker feeds an episode containing control tokens (e.g., `<|im_start|>system...`) into the system. During streaming DBSCAN pipeline clustering or RAPTOR synthesis, this tainted memory is loaded into the LLM context, hijacking the compaction prompt to alter synthesized wisdom rules or exfiltrate data.",
        "blast_radius": "Corruption of the canonical Obsidian Vault (poisoning long-term memory) and potential execution of malicious tool calls if the agent reads the poisoned wisdom.",
        "recommended_structural_change": "Enforce strict input sanitization at the ingestion boundary (`episode` table insertion) by escaping or stripping control tokens and markdown backticks from all untrusted input sources before storage."
    },
    {
        "filename": "issue_005_vault_watcher_coupling.md",
        "title": "Coupling: Streaming-to-Disk Pipeline and Obsidian Vault Watcher",
        "labels": "architecture-review, adversarial",
        "finding": "The cognitive pipeline streams artifacts to the canonical Obsidian Vault markdown files. Concurrently, a file system watcher monitors the vault (500ms coalescing). If the pipeline writes trigger the watcher to re-ingest, it creates a tight coupling.",
        "current_assumption": "The cognitive pipeline and the vault watcher can coexist peacefully if the pipeline suppresses raw episode `.md` flushes and the watcher relies on 500ms coalescing.",
        "attack_scenario": "A bug in path filtering or a rapidly fluctuating synthesis loop causes the pipeline to rapidly overwrite markdown files. The watcher picks up these changes and re-ingests them into SurrealDB, which triggers another compaction sweep, leading to an infinite write-ingest loop.",
        "blast_radius": "Unbounded disk I/O, database bloat, and CPU exhaustion, rendering the daemon unusable.",
        "recommended_structural_change": "Decouple the write pipeline from the read watcher via distinct staging directories or cryptographic watermarks (e.g., appending a specific metadata tag that the watcher explicitly ignores) to guarantee unidirectional data flow without relying solely on path exclusions."
    },
    {
        "filename": "issue_006_scaling_bottleneck_sqlite.md",
        "title": "18-Month Scaling: SQLite Embedding Cache & Pipeline Ephemeral State Bloat",
        "labels": "architecture-review, adversarial",
        "finding": "The architecture relies on a local SQLite Embedding Cache (`embeddings.db`) and ephemeral DBSCAN states stored in the SurrealDB `pipeline_cluster` table (with RAII cleanup).",
        "current_assumption": "Local SQLite and SurrealDB can handle embedding caching and ephemeral cluster state for single-user workloads efficiently.",
        "attack_scenario": "As the system scales 10x over 18 months (more agents, massive context ingestion), the SQLite embedding cache will suffer from severe I/O lock contention on transaction-bounded batch writes. Concurrently, aborted pipeline runs (e.g., due to panics or OOMs) may leak `pipeline_cluster` records if the RAII `scopeguard::defer!` fails to execute (e.g., SIGKILL), bloating SurrealDB.",
        "blast_radius": "Catastrophic I/O degradation and database corruption/bloat under heavy concurrent workload, requiring a full re-architecture of the storage layer.",
        "recommended_structural_change": "1) Replace the SQLite embedding cache with a dedicated, highly concurrent vector store (e.g., Qdrant or Milvus). 2) Move ephemeral pipeline state out of the persistent database entirely and into a volatile, fast in-memory store (e.g., Redis or a dedicated tokio async state manager)."
    }
]

for issue in issues:
    filepath = os.path.join('issues', issue['filename'])
    with open(filepath, 'w') as f:
        f.write(f"# {issue['title']}\n\n")
        f.write(f"**Labels:** {issue['labels']}\n\n")
        f.write(f"**Finding:** {issue['finding']}\n\n")
        f.write(f"**Current Assumption:** {issue['current_assumption']}\n\n")
        f.write(f"**Attack Scenario:** {issue['attack_scenario']}\n\n")
        f.write(f"**Blast Radius:** {issue['blast_radius']}\n\n")
        f.write(f"**Recommended Structural Change:** {issue['recommended_structural_change']}\n")
