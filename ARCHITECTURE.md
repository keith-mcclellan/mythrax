# Mythrax 1.0 Architecture

This document outlines the technical architecture, data flow, and security models of Mythrax 1.0. The system is designed as a high-performance, secure, and memory-efficient semantic search and retrieval platform, leveraging a centralized daemon architecture to optimize resource utilization and ensure data integrity.

## 1. Client-Server Architecture

Mythrax employs a lightweight, stateless client-server topology. The CLI and any external MCP (Model Context Protocol) servers act as thin HTTP clients, while the heavy lifting is offloaded to a persistent, central daemon process.

### Communication Protocol
All interactions between clients and the backend are handled via standard HTTP/HTTPS requests. The default communication port is **8090**, which can be overridden via the environment variable `MYTHRAX_DAEMON_PORT`.

### Auto-Spawn Sequence
To ensure a seamless user experience, the client implements an intelligent auto-spawn mechanism. When a client command is executed, the following sequence occurs:

1.  **Daemon Check**: The client checks for the existence of a running daemon process by attempting to connect to the configured port (default 8090).
2.  **Background Spawn**: If the daemon is not responding, the client automatically triggers `mythrax daemon start` in the background.
3.  **PID Management**: The client writes the Process ID (PID) of the newly spawned daemon to `~/.mythrax/daemon.pid` to facilitate lifecycle management.
4.  **Port Polling**: The client enters a polling loop, checking the daemon port for up to **5 seconds**.
5.  **Request Forwarding**: Once the daemon is confirmed to be listening, the client forwards the original request to the daemon.

This design ensures that users never need to manually manage the daemon process, while preventing race conditions where a client sends a request before the server is ready to accept it.

## 2. RocksDB Single-Writer Integrity

Data integrity and concurrency are managed through a strict single-writer architecture enforced by RocksDB's file locking mechanisms.

### The Locking Problem
RocksDB requires an exclusive file lock on the database directory to prevent data corruption. In a multi-process environment, this typically leads to "lock contention" errors, where multiple processes attempt to open the same database simultaneously, causing crashes or failures.

### The Mythrax Solution
Mythrax resolves this by designating the **Daemon Process** as the sole writer and reader of the RocksDB instance.

*   **Exclusive Access**: The daemon holds the exclusive file lock on the RocksDB data directory.
*   **Client Read-Only**: Clients do not attempt to open the database directly. Instead, they send HTTP requests to the daemon, which performs the database operations.
*   **Crash Prevention**: By centralizing the lock, we eliminate the possibility of concurrent process crashes due to lock contention. This topology allows for robust, concurrent client access without compromising the integrity of the underlying storage engine.

## 3. Token-Based Security

Security is enforced at the HTTP layer using a custom header-based authentication mechanism.

### Authentication Flow
Every incoming HTTP request must include the `X-Mythrax-Token` header. The daemon validates this token using a constant-time comparison function to prevent timing attacks.

1.  **Token Retrieval**: The daemon reads the expected token from `~/.mythrax/token`.
2.  **Verification**: The function `crate::auth::verify_token_constant_time` compares the provided header value against the stored token.
3.  **Fallback Mode**: In headless or test environments, if no token file exists, the system falls back to a default hardcoded value: `"secret-token"`.

### Security Properties
*   **Constant-Time Verification**: The use of constant-time comparison ensures that the execution time of the verification does not depend on the number of matching characters, mitigating timing side-channel attacks.
*   **Stateless Validation**: The daemon does not maintain session state, allowing for horizontal scaling of client connections without complex session management.

## 4. ONNX Embedding Centrization

A key performance optimization in Mythrax 1.0 is the centralization of the ONNX embedding runtime.

### Resource Optimization
The embedding model (`nomic-embed-text-v1.5.onnx`) is loaded and managed exclusively within the daemon process.

*   **Memory Footprint**: By centralizing the model, clients do not need to load the embedding model into their own memory space. This reduces the active memory footprint of each client process by approximately **50%**.
*   **Performance**: The daemon can maintain the model in memory, avoiding the latency of reloading the model for every client request. This is particularly beneficial for high-concurrency scenarios where multiple clients may be querying the system simultaneously.

### Architecture Benefit
This separation of concerns allows clients to be extremely lightweight, focusing solely on request serialization and response handling, while the daemon handles the computationally expensive embedding generation.

## 5. SurrealDB Schema & Graph Relations

Mythrax utilizes SurrealDB as its primary data store, leveraging its native graph capabilities to manage complex relationships between entities.

### Core Schemas
The system defines four primary node types:

1.  **`episode`**: Represents a discrete unit of content or interaction.
2.  **`wiki_node`**: Represents structured knowledge entries or facts.
3.  **`wisdom_rule`**: Encapsulates logical rules or heuristics for decision-making.
4.  **`handoff`**: Represents a transfer of context or responsibility between agents or processes.

### Graph Relations
Entities are linked via directed graph edges, enabling complex traversal and query capabilities.

*   **`relates_to`**: This is the primary edge type, connecting nodes to establish semantic and logical relationships. For example, an `episode` may `relates_to` a specific `wiki_node` for context, or a `wisdom_rule` may `relates_to` an `episode` to define its handling logic.
*   **Bidirectional Semantic Search Citations**: The system supports bidirectional linking for citations. When a semantic search query retrieves a result, the system creates a link back to the source, ensuring that citations are not just one-way references but part of a navigable graph. This allows for reverse lookups, where users can trace back which queries or episodes referenced a specific piece of knowledge.

### Data Integrity
The graph structure ensures that relationships are first-class citizens in the database, allowing for efficient traversal of complex dependency trees without the need for expensive join operations typical in relational databases.
