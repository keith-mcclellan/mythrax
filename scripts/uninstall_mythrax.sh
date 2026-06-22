#!/usr/bin/env bash
# uninstall_mythrax.sh - Clean uninstallation of Project Mythrax components

set -euo pipefail

WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Target directories default to WORKSPACE_ROOT if no args passed
TARGET_DIRS=()
if [ "$#" -gt 0 ]; then
  for dir in "$@"; do
    TARGET_DIRS+=("$(cd "$dir" && pwd)")
  done
else
  TARGET_DIRS+=("$WORKSPACE_ROOT")
fi

echo "=== Uninstalling Mythrax from target directories: ==="
for dir in "${TARGET_DIRS[@]}"; do
  echo "  - $dir"
done

# 1. Stop any running mythrax daemon
echo "Stopping any running Mythrax daemon processes..."
pkill -f "target/debug/mythrax daemon start" || true
pkill -f "mythrax-core/target/debug/mythrax" || true
pkill -f "mythrax daemon" || true

# Unload launchd agent if exists
PLIST_PATH="$HOME/Library/LaunchAgents/com.mythrax.daemon.plist"
if [ -f "$PLIST_PATH" ]; then
  echo "Unloading and removing launchd service..."
  launchctl unload "$PLIST_PATH" || true
  rm -f "$PLIST_PATH"
fi

# 2. Clean Git hooks for target directories
HOOK_NAMES=("pre-commit" "pre-push" "post-checkout" "post-merge" "post-commit")
for dir in "${TARGET_DIRS[@]}"; do
  echo "--- Cleaning hooks in: $dir ---"
  HOOKS_DIR="$dir/.git/hooks"
  if [ -d "$HOOKS_DIR" ]; then
    for hook in "${HOOK_NAMES[@]}"; do
      HOOK_PATH="$HOOKS_DIR/$hook"
      if [ -f "$HOOK_PATH" ]; then
        # Check if it was written by mythrax
        if grep -q "Project Mythrax" "$HOOK_PATH"; then
          echo "Removing hook: $HOOK_PATH"
          rm -f "$HOOK_PATH"
        fi
      fi
    done
  fi
done

# 3. Clean global Gemini configurations
echo "Cleaning global Gemini configurations..."
python3 -c "
import os
import json

config_dir = os.path.expanduser('~/.gemini/config')

# 1. Update mcp_config.json
mcp_path = os.path.join(config_dir, 'mcp_config.json')
if os.path.exists(mcp_path):
    try:
        with open(mcp_path, 'r', encoding='utf-8') as f:
            data = json.load(f)
        if 'mcpServers' in data and 'mythrax' in data['mcpServers']:
            del data['mcpServers']['mythrax']
            with open(mcp_path, 'w', encoding='utf-8') as f:
                json.dump(data, f, indent=2)
            print('Successfully removed mythrax from mcp_config.json')
    except Exception as e:
        print(f'Error updating mcp_config.json: {e}')

# 2. Update config.json
config_path = os.path.join(config_dir, 'config.json')
if os.path.exists(config_path):
    try:
        with open(config_path, 'r', encoding='utf-8') as f:
            data = json.load(f)
        allow_list = data.get('userSettings', {}).get('globalPermissionGrants', {}).get('allow', [])
        grant_term = 'mcp(mythrax/*)'
        if grant_term in allow_list:
            allow_list.remove(grant_term)
            with open(config_path, 'w', encoding='utf-8') as f:
                json.dump(data, f, indent=2)
            print('Successfully removed mcp(mythrax/*) permissions from config.json')
    except Exception as e:
        print(f'Error updating config.json: {e}')

# 3. Update hooks.json
hooks_path = os.path.join(config_dir, 'hooks.json')
if os.path.exists(hooks_path):
    try:
        with open(hooks_path, 'r', encoding='utf-8') as f:
            data = json.load(f)
        if 'mythrax-compliance' in data:
            del data['mythrax-compliance']
            with open(hooks_path, 'w', encoding='utf-8') as f:
                json.dump(data, f, indent=2)
            print('Successfully removed compliance hook from hooks.json')
    except Exception as e:
        print(f'Error updating hooks.json: {e}')
"

# 4. Remove local data directory
MYTHRAX_DIR="$HOME/.mythrax"
if [ -d "$MYTHRAX_DIR" ]; then
  echo "Removing local Mythrax directory: $MYTHRAX_DIR"
  rm -rf "$MYTHRAX_DIR"
fi

echo "=== Mythrax Uninstallation Completed Successfully ==="
