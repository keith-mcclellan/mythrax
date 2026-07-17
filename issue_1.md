# Critical: SQL Injection Vulnerabilities via String Interpolation

## Description
Multiple execution paths in the codebase construct SurrealDB queries using string interpolation (`format!`) with user-controlled or dynamically generated inputs, rather than using parameterized queries (`bind`). This allows arbitrary SQL execution if the input contains malicious queries, completely bypassing application logic and data constraints.

## Locations
- `src/vault/ingestion.rs:47`: `let query_sql = format!("SELECT role, content FROM {};", table_name);`
- `src/mcp_routes/manage_handlers.rs:1480`: `let sql = format!("SELECT scope FROM {};", rec_id.table);`
- `src/cognitive/synthesis.rs:1218`: `let sql_wiki = format!("SELECT *, vector::similarity::cosine(embedding, $emb) AS similarity FROM wiki_node WHERE embedding <|200, {}|> $emb;", hnsw_ef);`
- `src/cognitive/synthesis.rs:1237`: `let sql_ep = format!("SELECT *, vector::similarity::cosine(embedding, $emb) AS similarity FROM episode WHERE node_type = 'procedural' AND embedding <|200, {}|> $emb;", hnsw_ef);`
- `src/mcp_routes/vault_handlers.rs:271-276`: `let delete_queries = format!("DELETE FROM chat_history WHERE session_id = '{}'; ...", sess);`
- `src/db/search_pipeline.rs`: Multiple instances where SQL query strings are dynamically generated via `format!` before execution.

## Remediation
Refactor all identified locations to use parameterized queries (`.bind()`) or `type::table()` for dynamic table names.
For dynamic table names in SurrealDB queries, parameterize them using `$table_name` and binding the parameter, or `type::table($table_name)`. Never use string interpolation for SQL queries.
