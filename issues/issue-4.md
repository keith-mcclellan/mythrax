# Tight coupling between SurrealDB AST ingestion and Code AST Parser

**Tags:** `architecture-review`, `adversarial`
**Note:** Never close this issue without a documented architectural decision record (ADR) response.

**Finding** -> Tight coupling between SurrealDB AST ingestion and Code AST Parser
**Current Assumption** -> Extracted function signatures and AST symbols can be tightly integrated and directly written to the SurrealDB `code_symbol` table with an HNSW index.
**Attack Scenario** -> A vulnerability or requirement change in the AST parsing logic, or a failure in SurrealDB's HNSW index implementation, necessitates an independent rollback or replacement of one component. Because they cannot be independently deployed or tested, changing the parser breaks the ingestion pipeline.
**Blast Radius** -> Migration failures cause database corruption and index desynchronization, halting all code navigation, context retrieval, and cognitive scheduling features across the platform.
**Recommended Structural Change** -> Introduce an event-driven abstraction layer (e.g., an event bus or write-ahead log) between the AST parser and the database. This allows both modules to be independently tested, deployed, and replaced, mitigating this architectural liability.
