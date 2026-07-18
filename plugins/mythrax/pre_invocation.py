import os
import json
import sys
import urllib.request
import urllib.error

def main():
    # Read stdin
    try:
        input_data = sys.stdin.read()
        if not input_data.strip():
            input_data = "{}"
        ctx = json.loads(input_data)
    except Exception as e:
        sys.stderr.write(f"Error parsing stdin: {e}\\n")
        ctx = {}

    session_id = ctx.get("conversationId", "")
    if not session_id:
        session_id = os.environ.get("MYTHRAX_SESSION_ID", "general")

    workspace_paths = ctx.get("workspacePaths", [])
    workspace_path = workspace_paths[0] if workspace_paths else os.environ.get("MYTHRAX_WORKSPACE_ROOT", ".")
    query = ctx.get("nextPrompt", "")

    # Get auth token
    home = os.environ.get("HOME", "")
    token = ""
    token_path = os.path.join(home, ".mythrax", "token")
    if os.path.exists(token_path):
        try:
            with open(token_path, "r") as f:
                token = f.read().strip()
        except Exception:
            pass
    token = os.environ.get("MYTHRAX_TOKEN") or os.environ.get("MYTHRAX_DAEMON_TOKEN") or token

    port = os.environ.get("MYTHRAX_DAEMON_PORT", "8090")
    url = f"http://127.0.0.1:{port}/v1/mcp/call"

    payload = {
        "name": "manage",
        "arguments": {
            "action": "pre_invocation",
            "session_id": session_id,
            "query": query,
            "workspace_path": workspace_path
        }
    }

    headers = {
        "Content-Type": "application/json",
        "X-Mythrax-Token": token
    }

    success = False
    response_text = ""

    try:
        req = urllib.request.Request(
            url,
            data=json.dumps(payload).encode("utf-8"),
            headers=headers,
            method="POST"
        )
        with urllib.request.urlopen(req, timeout=1.5) as response:
            resp_data = response.read().decode("utf-8")
            resp_json = json.loads(resp_data)
            content = resp_json.get("content", [])
            if content and isinstance(content, list):
                response_text = content[0].get("text", "")
            else:
                response_text = resp_json.get("text", "")
            success = True
    except Exception as e:
        sys.stderr.write(f"Warning: Failed to query Mythrax daemon: {e}\\n")

    if not success or not response_text:
        response_text = (
            "### ⛔ Known Failed Approaches\\n"
            "- [Mythrax Pre-Invocation Hook Warning: SurrealDB Daemon offline. Memory retrieval and state synchronization skipped.]\\n\\n"
            "### ⚠️ Known Knowledge Boundaries / Conflicts\\n"
            "- [Mythrax Pre-Invocation Hook Warning: SurrealDB Daemon offline. Memory retrieval and state synchronization skipped.]"
        )

    out_json = {
        "injectSteps": [
            {
                "ephemeralMessage": response_text
            }
        ]
    }
    print(json.dumps(out_json, indent=2))

if __name__ == "__main__":
    main()
