#!/usr/bin/env bash
# install_hooks.sh - Unified installer for Project Mythrax v0.1 Alpha hooks

set -euo pipefail

WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
echo "=== Installing Mythrax hooks to: $WORKSPACE_ROOT ==="

# 1. Initialize git if not present
if [ ! -d "$WORKSPACE_ROOT/.git" ]; then
  echo "Initializing git repository..."
  git -C "$WORKSPACE_ROOT" init
  git -C "$WORKSPACE_ROOT" remote add origin https://github.com/keith-mcclellan/mythrax || true
fi

# 2. Write Git hooks
HOOKS_DIR="$WORKSPACE_ROOT/.git/hooks"
mkdir -p "$HOOKS_DIR"

echo "Writing git pre-commit, pre-push, post-checkout, post-merge hooks..."
HOOK_NAMES=("pre-commit" "pre-push" "post-checkout" "post-merge")
for hook in "${HOOK_NAMES[@]}"; do
  HOOK_PATH="$HOOKS_DIR/$hook"
  cat << 'EOF' > "$HOOK_PATH"
#!/bin/sh
# Compliance verification hook installed by Project Mythrax
echo "Running compliance check hook..."
python3 /Users/keith/.gemini/antigravity/scratch/verify_compliance.py "$(pwd)"
EOF
  chmod +x "$HOOK_PATH"
done

echo "Writing git post-commit hook..."
POST_COMMIT_PATH="$HOOKS_DIR/post-commit"
cat << 'EOF' > "$POST_COMMIT_PATH"
#!/bin/sh
# Commit indexing hook installed by Project Mythrax
commit_msg=$(git log -1 --pretty=%B)
scope=$(basename "$(pwd)")
token=$(cat ~/.mythrax/token 2>/dev/null || echo "secret-token")

curl -s -X POST -H "Content-Type: application/json" \
  -H "X-Mythrax-Token: $token" \
  -d "{\"title\": \"Commit: $(git log -1 --pretty=%s)\", \"content\": \"$commit_msg\", \"entities\": [], \"scope\": \"$scope\"}" \
  http://127.0.0.1:8090/v1/episodes >/dev/null 2>&1 &
EOF
chmod +x "$POST_COMMIT_PATH"

# 3. Update global Gemini config.json and mcp_config.json using Python JSON parser
echo "Configuring MCP and permission grants in ~/.gemini/config/..."
python3 -c "
import os
import json

config_dir = os.path.expanduser('~/.gemini/config')
os.makedirs(config_dir, exist_ok=True)

# Update mcp_config.json
mcp_path = os.path.join(config_dir, 'mcp_config.json')
mcp_data = {'mcpServers': {}}
if os.path.exists(mcp_path):
    try:
        with open(mcp_path, 'r', encoding='utf-8') as f:
            mcp_data = json.load(f)
    except Exception:
        pass

mcp_data.setdefault('mcpServers', {})
mcp_data['mcpServers']['mythrax'] = {
    'command': '/Users/keith/Documents/self-improvement-engine/.venv/bin/python',
    'args': ['-m', 'mythrax_mcp.main'],
    'env': {
        'MYTHRAX_API_URL': 'http://127.0.0.1:8090'
    }
}

with open(mcp_path, 'w', encoding='utf-8') as f:
    json.dump(mcp_data, f, indent=2)
print('Successfully updated mcp_config.json')

# Update config.json (Global Permissions)
config_path = os.path.join(config_dir, 'config.json')
config_data = {}
if os.path.exists(config_path):
    try:
        with open(config_path, 'r', encoding='utf-8') as f:
            config_data = json.load(f)
    except Exception:
        pass

user_settings = config_data.setdefault('userSettings', {})
global_grants = user_settings.setdefault('globalPermissionGrants', {})
allow_list = global_grants.setdefault('allow', [])

grant_term = 'mcp(mythrax/*)'
if grant_term not in allow_list:
    allow_list.append(grant_term)

with open(config_path, 'w', encoding='utf-8') as f:
    json.dump(config_data, f, indent=2)
print('Successfully updated config.json with mcp(mythrax/*) permissions')
"

# 4. Write hooks.json
echo "Writing hooks.json to global configuration..."
python3 -c "
import os
import json

config_dir = os.path.expanduser('~/.gemini/config')
hooks_path = os.path.join(config_dir, 'hooks.json')
hooks_data = {}
if os.path.exists(hooks_path):
    try:
        with open(hooks_path, 'r', encoding='utf-8') as f:
            hooks_data = json.load(f)
    except Exception:
        pass

hooks_data['mythrax-compliance'] = {
    'PreInvocation': [
        {
            'type': 'command',
            'command': 'python3 /Users/keith/.gemini/antigravity/scratch/verify_compliance.py'
        }
    ]
}

with open(hooks_path, 'w', encoding='utf-8') as f:
    json.dump(hooks_data, f, indent=2)
print('Successfully updated hooks.json')
"

echo "=== Installation Completed Successfully ==="
