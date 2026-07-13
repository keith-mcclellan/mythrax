# DEVELOPMENT.md

## Mythrax 3.0: Developer Guide & Architecture Reference

This document serves as the authoritative reference for developers contributing to the Mythrax codebase. It details the architectural decisions behind the consolidated tool strategy and provides a guide for extending the system with new capabilities.

---

## 1. Tool Consolidation Architecture

### The Problem: Context Schema Bloat & Router Overhead
In previous iterations of local-first AI agents, exposing every distinct operation as a separate MCP (Model Context Protocol) tool resulted in significant overhead:
1. **Context Window Waste**: Exposing dozens of distinct tools consumed ~15-20% of the context window of small local LLMs (e.g., Qwen 0.5B / 1.5B or Llama-3-8B).
2. **Schema Complexity**: Managing many endpoints increased the cognitive load and led to incorrect tool-calling choices by the LLM.

### The Solution: The 4-Tool Consolidated Standard
Mythrax collapses all operations into exactly **4 high-efficiency, action-enum-based tools**:

1. **`read`**: All query, search, retrieval, and fetch operations.
2. **`write`**: All mutation, creation, replacement, and config update operations.
3. **`manage`**: All lifecycle management, audits, vault synchronization, and cognitive loops (Compaction, HTR, dreaming).
4. **`agent`**: Standard interface for local model task orchestration.

#### Benefits
* **>75% Reduction in Schema Bloat**: Reduces schema definitions sent to the LLM to just 4 signatures.
* **Lightweight Context**: The LLM only needs to remember 4 tool signatures, passing the appropriate `action` string parameter to control behavior.
* **Extensibility**: Adding a new feature does not require registering a new tool, only adding a new enum variant and handler match arm in Rust.

#### Architecture Flow
```
[LLM] --> [MCP Client] --> [Mythrax Daemon]
                                  |
                                  v
                           [Router: call_mcp_tool]
                                  |
            +---------------------+---------------------+
            |                     |                     |
         [read]                [write]               [manage]
            |                     |                     |
      [handle_read]         [handle_write]        [handle_manage]
            |                     |                     |
            +---------------------+---------------------+
                                  |
                                  v
                           [StorageBackend]
                                  |
                           [SurrealDB / Mem]
```

---

## 2. Step-by-Step Guide: Adding/Modifying Actions

This guide walks you through adding a new action, such as a custom `list_archives` query, to the `read` tool.

### Step 1: Define the Action Enum Variant
Open `mythrax-core/src/mcp_routes.rs` and locate the `get_mcp_tools_schema()` function. Find the `read` tool definition and add the new action string to the `enum` array for the `action` parameter.

```rust
// mythrax-core/src/mcp_routes.rs

pub fn get_mcp_tools_schema() -> Value {
    json!({
        "tools": [
            {
                "name": "read",
                "description": "Consolidated tool for all reading and querying...",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { 
                            "type": "string", 
                            "enum": [
                                "view", "search", "rules", "nodes", "root", 
                                "query_symbolic", "search_index", "timeline", 
                                "get_full", "get", 
                                "list_archives" // <--- ADD NEW ACTION HERE
                            ] 
                        },
                        // ... other arguments ...
                    },
                    "required": ["action"]
                }
            },
            // ...
        ]
    })
}
```

### Step 2: Update the Router Routing Arm
Open the corresponding sub-handler file in `mythrax-core/src/mcp_routes/` (e.g., `read_handlers.rs` for read actions, `write_handlers.rs` for write actions). Locate the action handler function corresponding to the tool (in this case, `handle_read`). Map the action string to your handler logic:

```rust
// mythrax-core/src/mcp_routes/read_handlers.rs

async fn handle_read(state: &ApiState, mut args: Value) -> Result<Value> {
    let action = args.get("action").and_then(|v| v.as_str()).context("Missing action")?.to_string();
    
    match action.as_str() {
        "view" => handle_view_action(state, args).await,
        "search" => handle_search_action(state, args).await,
        // ...
        "list_archives" => handle_list_archives_action(state, args).await, // <--- ROUTE HERE
        _ => Err(anyhow::anyhow!("Unknown read action: {}", action)),
    }
}
```

### Step 3: Implement Handler & Backend Logic
Implement your handler function in `read_handlers.rs` or delegate to `StorageBackend` methods.

```rust
// mythrax-core/src/mcp_routes/read_handlers.rs

async fn handle_list_archives_action(state: &ApiState, args: Value) -> Result<Value> {
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let results = state.backend.get_archived_memories(limit).await?;
    Ok(json!({ "archives": results }))
}
```

---

## 3. Testing Guidelines

### Running Test Suites in Parallel
To prevent database lock contentions and significantly speed up the validation loop, run all tests in parallel using `nextest` (with the `MYTHRAX_TEST_MOCK=1` environment variable to bypass heavy Hugging Face downloads and GPU VRAM allocations):

```bash
# Run all unit and integration tests
MYTHRAX_TEST_MOCK=1 cargo nextest run --features mlx
```

### Mocking in Tests
When writing unit/integration tests, make sure to mock MLX embeddings to avoid calling local GPU memory pools:
```rust
std::env::set_var("MYTHRAX_TEST_MOCK", "1");
```

### Metal FFI GPU Compilation Dependencies (Apple Silicon)
If building `mythrax` with MLX GPU acceleration features, the Xcode FFI compilation utilities (`metal` compiler) are required.
If cargo build fails with `xcrun: error: unable to find utility "metal"`, specify the Xcode developer tool path:

```bash
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo build --release --features mlx
```

---
