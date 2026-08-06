import os

def create_issue(title, body, labels):
    os.makedirs("issues", exist_ok=True)
    filename = f"issues/{title.replace(' ', '_').replace('/', '_').replace('.', '_').lower()}.md"
    with open(filename, "w") as f:
        f.write(f"---\nLabels: {', '.join(labels)}\n---\n")
        f.write(f"# {title}\n\n")
        f.write(body)

create_issue(
    "Panic vulnerability in search_pipeline.rs on keyword_resp_res.unwrap()",
    "**File**: `mythrax-core/src/db/search_pipeline.rs`\n"
    "**Line**: 1986\n\n"
    "**Scenario**: If the keyword search DB query fails (e.g. malformed query or DB timeout) in hybrid mode, `keyword_resp_res` can be an `Err`. Calling `.unwrap()` will panic, crashing the daemon. This is particularly sensitive to user inputs that could contain malformed query syntax.\n"
    "**Severity**: High\n"
    "**Suggested Fix**: Use proper error handling, e.g., `let mut keyword_candidates = parse_results(keyword_resp_res?, false)?;`.",
    ["bug", "agent-found"]
)

create_issue(
    "Panic vulnerability in crud_operations.rs on temporal relation parse",
    "**File**: `mythrax-core/src/db/crud_operations.rs`\n"
    "**Line**: 517-518\n\n"
    "**Scenario**: When processing `relations` during batch save, the code calls `rel.get(\"from_str\").unwrap().as_str().unwrap()`. If a malformed temporal relation without `from_str` or `to_str` keys is provided via the API, this will panic and crash the daemon.\n"
    "**Severity**: High\n"
    "**Suggested Fix**: Use safe optional access, e.g., `if let (Some(from_val), Some(to_val)) = (rel.get(\"from_str\"), rel.get(\"to_str\")) { if let (Some(from_str), Some(to_str)) = (from_val.as_str(), to_val.as_str()) { ... } }`.",
    ["bug", "agent-found"]
)
