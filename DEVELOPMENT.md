# DEVELOPMENT.md

## Mythrax 1.0: Developer Guide & Architecture Reference

This document serves as the authoritative reference for developers contributing to the Mythrax codebase. It details the architectural decisions behind the tool consolidation strategy and provides a step-by-step guide for extending the system with new capabilities.

---

## 1. Tool Consolidation Architecture

### The Problem: Context Schema Bloat
In previous iterations of local-first AI agents, the standard approach was to expose every distinct operation as a separate MCP (Model Context Protocol) tool. For example, a memory system might expose `add_memory`, `get_memory`, `delete_memory`, `search_memory`, `list_tags`, etc.

This approach creates significant overhead:
1.  **Context Window Waste**: Each tool definition includes a name, description, and argument schema. For 32 legacy tools, this consumed ~15-20% of the context window for a small local LLM (e.g., Llama-3-8B or Mistral-7B).
2.  **Token Overhead**: The JSON-RPC payload for each tool call is verbose.
3.  **Complexity**: Managing 32 distinct endpoints increases the cognitive load for both the developer and the LLM.

### The Solution: Action-Enum-Based Tools
Mythrax 1.0 collapses these 32 legacy tools into **9 high-efficiency, action-enum-based tools**.

#### Core Concept
Instead of exposing 32 tools, we expose 9 "super-tools." Each tool accepts an `action` enum parameter that dictates the specific behavior.

**Example:**
*   **Legacy**: `add_memory`, `get_memory`, `delete_memory` (3 tools)
*   **Mythrax**: `memory_tool` with `action: "add" | "get" | "delete"` (1 tool)

#### Benefits
1.  **>60% Reduction in Schema Bloat**: By reusing the same tool definition structure, we reduce the number of tool definitions sent to the LLM.
2.  **Lightweight Context**: The LLM only needs to learn 9 tool signatures instead of 32.
3.  **Extensibility**: Adding a new action (e.g., `list_sessions`) does not require adding a new tool definition, only a new enum variant and handler.

#### Architecture Diagram
```
[LLM] --> [MCP Client] --> [Mythrax Daemon]
                                  |
                                  v
                          [Router: call_mcp_tool]
                                  |
                  +---------------+---------------+
                  |               |               |
            [memory_tool]   [session_tool]   [file_tool]
                  |               |               |
                  v               v               v
            [handle_memory] [handle_session] [handle_file]
                  |               |               |
                  v               v               v
            [StorageBackend]  [StorageBackend] [StorageBackend]
                  |               |               |
                  v               v               v
            [SurrealDB]     [SurrealDB]     [SurrealDB]
```

---

## 2. Step-by-Step Guide: Adding/Modifying Tools

This guide walks you through adding a new action, such as `list_sessions`, to the `session_tool`.

### Prerequisites
-   Rust toolchain (1.75+)
-   SurrealDB instance running
-   Understanding of the existing `mythrax-core` structure

### Step 1: Define the Action Enum

First, we must define the new action in the schema definition. This ensures the LLM knows this action is valid.

**File**: `mythrax-core/src/mcp_routes.rs`

Locate the `get_mcp_tools_schema()` function. You will see a list of tool definitions. Find the `session_tool` definition and update its `arguments` schema to include the new action in the `action` enum.

```rust
// mythrax-core/src/mcp_routes.rs

pub fn get_mcp_tools_schema() -> Vec<Tool> {
    vec![
        // ... other tools ...
        Tool::new(
            "session_tool",
            "Manage user sessions, including listing, creating, and deleting sessions.",
            json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": [
                            "create_session",
                            "get_session",
                            "delete_session",
                            "list_sessions" // <--- ADD NEW ACTION HERE
                        ],
                        "description": "The specific action to perform on sessions."
                    },
                    "session_id": {
                        "type": "string",
                        "description": "The unique identifier of the session (required for get, delete, list)."
                    },
                    "query": {
                        "type": "string",
                        "description": "Search query for filtering sessions (optional, used with list_sessions)."
                    }
                },
                "required": ["action"]
            }),
        ),
        // ... other tools ...
    ]
}
```

**Key Point**: The `enum` list must be exhaustive for the LLM to understand valid options.

### Step 2: Add Routing Logic in `call_mcp_tool`

Next, we need to route the `list_sessions` action to a dedicated handler function.

**File**: `mythrax-core/src/mcp_routes.rs`

Locate the `call_mcp_tool` function. This function matches the `tool_name` and `action` to the appropriate handler.

```rust
// mythrax-core/src/mcp_routes.rs

pub async fn call_mcp_tool(
    tool_name: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    match tool_name {
        "memory_tool" => handle_memory(args).await,
        "session_tool" => handle_session(args).await, // <--- ROUTE TO HANDLER
        "file_tool" => handle_file(args).await,
        // ... other tools ...
        _ => Err(AppError::ToolNotFound(tool_name.to_string())),
    }
}

// Add the new handler function
async fn handle_session(args: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let action = args["action"].as_str().ok_or_else(|| AppError::InvalidArgument("action".to_string()))?;

    match action {
        "create_session" => handle_create_session(args).await,
        "get_session" => handle_get_session(args).await,
        "delete_session" => handle_delete_session(args).await,
        "list_sessions" => handle_list_sessions(args).await, // <--- ADD NEW MATCH ARM
        _ => Err(AppError::InvalidArgument(format!("Unknown action: {}", action))),
    }
}
```

### Step 3: Implement Backend Delegation

Now, implement the `handle_list_sessions` function. This function will interact with the `StorageBackend` trait and execute the appropriate SurrealDB query.

**File**: `mythrax-core/src/handlers/session.rs` (or similar, depending on your module structure)

```rust
// mythrax-core/src/handlers/session.rs

use crate::storage::StorageBackend;
use crate::error::AppError;
use serde_json::Value;

pub async fn handle_list_sessions(args: Value) -> Result<Value, AppError> {
    let session_id = args["session_id"].as_str();
    let query = args["query"].as_str();

    // Get the storage backend from the app state (assuming you have access to it)
    // In a real implementation, you might pass the backend as a parameter or use a global state
    let backend = get_storage_backend()?; // Placeholder for actual state retrieval

    match session_id {
        Some(id) => {
            // If session_id is provided, list sessions within that session
            backend.list_sessions_by_parent(id, query).await
        }
        None => {
            // If no session_id, list all top-level sessions
            backend.list_all_sessions(query).await
        }
    }
}
```

**File**: `mythrax-core/src/storage/mod.rs` (Define the trait methods)

```rust
// mythrax-core/src/storage/mod.rs

pub trait StorageBackend: Send + Sync {
    // ... other methods ...
    async fn list_all_sessions(&self, query: Option<&str>) -> Result<Value, AppError>;
    async fn list_sessions_by_parent(&self, parent_id: &str, query: Option<&str>) -> Result<Value, AppError>;
}
```

**File**: `mythrax-core/src/storage/surreal_backend.rs` (Implement the trait)

```rust
// mythrax-core/src/storage/surreal_backend.rs

impl StorageBackend for SurrealBackend {
    // ... other implementations ...

    async fn list_all_sessions(&self, query: Option<&str>) -> Result<Value, AppError> {
        let mut query_builder = self.db.query::<Value>("SELECT * FROM session");
        
        if let Some(q) = query {
            // Add WHERE clause for filtering
            query_builder = query_builder.where_clause(format!("content LIKE '%{}%'", q));
        }

        let result = query_builder.send().await?;
        Ok(result)
    }

    async fn list_sessions_by_parent(&self, parent_id: &str, query: Option<&str>) -> Result<Value, AppError> {
        let mut query_builder = self.db.query::<Value>("SELECT * FROM session WHERE parent_id = $parent_id");
        query_builder = query_builder.param("parent_id", parent_id);

        if let Some(q) = query {
            // Add WHERE clause for filtering
            query_builder = query_builder.where_clause(format!("content LIKE '%{}%'", q));
        }

        let result = query_builder.send().await?;
        Ok(result)
    }
}
```

### Step 4: Update the CLI Commands

To make this new capability accessible via the command line, we need to update the CLI subcommand enums and routing.

**File**: `mythrax-cli/src/cli.rs`

Add a new subcommand variant for listing sessions.

```rust
// mythrax-cli/src/cli.rs

use clap::{Parser, Subcommand};

#[derive(Subcommand)]
pub enum Commands {
    // ... other commands ...
    /// List all sessions or sessions within a specific session
    ListSessions {
        /// Optional session ID to filter by parent
        #[arg(long)]
        session_id: Option<String>,
        /// Optional search query
        #[arg(long)]
        query: Option<String>,
    },
}
```

**File**: `mythrax-cli/src/main.rs`

Route the new CLI command to an HTTP request to the daemon's API.

```rust
// mythrax-cli/src/main.rs

use mythrax_cli::cli::Commands;
use mythrax_core::client::MythraxClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        // ... other commands ...
        Commands::ListSessions { session_id, query } => {
            let mut client = MythraxClient::new("http://localhost:8080");
            
            // Construct the args for the session_tool
            let mut args = serde_json::Map::new();
            args.insert("action".to_string(), serde_json::Value::String("list_sessions".to_string()));
            
            if let Some(id) = session_id {
                args.insert("session_id".to_string(), serde_json::Value::String(id));
            }
            
            if let Some(q) = query {
                args.insert("query".to_string(), serde_json::Value::String(q));
            }

            let result = client.call_tool("session_tool", args.into()).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}
```

### Step 5: Document in `SKILL.md`

Finally, update the agent's skill file to ensure the LLM knows when and how to use the new action.

**File**: `.agents/skills/mythrax/SKILL.md`

```markdown
# Mythrax Skill

## Session Management

### `session_tool`

Use this tool to manage user sessions. The `action` parameter determines the behavior.

#### Actions

-   `create_session`: Create a new session. Requires `session_id` (optional, if not provided, one will be generated).
-   `get_session`: Retrieve a specific session by `session_id`.
-   `delete_session`: Delete a session by `session_id`.
-   `list_sessions`: List sessions. 
    -   If `session_id` is provided, list sub-sessions within that session.
    -   If `query` is provided, filter sessions by content.

#### Example Usage

To list all sessions:
```json
{
  "tool": "session_tool",
  "arguments": {
    "action": "list_sessions"
  }
}
```

To list sessions within a specific session with a search query:
```json
{
  "tool": "session_tool",
  "arguments": {
    "action": "list_sessions",
    "session_id": "sess_123",
    "query": "important"
  }
}
```
```

**Key Point**: The `SKILL.md` file is critical. It is the primary source of truth for the LLM. If the LLM does not know about the new action, it will not use it, regardless of how well it is implemented in the backend.

---

## 3. Testing Guidelines

### Running Tests in Parallel
To prevent database lock contentions and leverage multi-threaded execution, always run the test suite using **nextest**:
```bash
# Run all tests in parallel
cargo nextest run --features mlx

# Or use the workspace alias
cargo t --features mlx
```

### Mocking Large Model Downloads in Test Environments
To prevent tests from attempting to download multi-gigabyte Hugging Face `.gguf` weights or allocating GPU VRAM during unit/integration testing, use the `MYTHRAX_TEST_MOCK=1` environment variable:
```bash
MYTHRAX_TEST_MOCK=1 cargo t --features mlx
```

### Build & Compilation (Metal / GPU Acceleration)
When building `mythrax` with the `mlx` feature enabled, the build script compiles the Metal FFI kernels using the Apple Metal compiler tools (`metal`). These tools are only present in the full Xcode App developer directory (typically under `/Applications/Xcode.app`), not in the standalone macOS command line tools (`/Library/Developer/CommandLineTools`).

If cargo build fails with `xcrun: error: unable to find utility "metal"`, specify the `DEVELOPER_DIR` environment variable pointing to the full Xcode installation:
```bash
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo build --release --features mlx
```

---

## Summary

By following these steps, you have:
1.  Defined the new action in the schema.
2.  Routed the action to a handler.
3.  Implemented the backend logic.
4.  Exposed the functionality via CLI.
5.  Documented the usage for the LLM.

This process ensures that Mythrax remains lightweight, extensible, and easy to maintain.
