#!/usr/bin/env bash
# reinitialize_mythrax.sh - Automates database/vault cleanup and re-ingestion
set -euo pipefail

# Resolve project root relative to this script so it runs correctly from any working directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Mythrax Reinitialization Protocol ==="
echo "Project root: $PROJECT_ROOT"

# 1. Stop active daemon and kill running processes to release file locks
echo "Stopping active daemon and any running mythrax processes..."
pkill -f "mythrax daemon" || true
pkill -f "mythrax_mcp" || true
pkill -f "target/release/mythrax" || true
pkill -f "target/debug/mythrax" || true
sleep 1

# 2. Retrieve vault root from ~/.mythrax/config.json, default to ~/mythrax-vault
VAULT_ROOT="~/mythrax-vault"
if [ -f ~/.mythrax/config.json ]; then
    CONFIG_VAULT=$(grep -o '"vault_root": *"[^"]*"' ~/.mythrax/config.json | cut -d'"' -f4 || true)
    if [ -n "$CONFIG_VAULT" ]; then
        VAULT_ROOT="$CONFIG_VAULT"
    fi
fi
VAULT_ROOT="${VAULT_ROOT/#\~/$HOME}"

# 3. Clean up the active Obsidian vault folders
echo "Active Obsidian Vault resolved to: $VAULT_ROOT"
if [ -d "$VAULT_ROOT" ]; then
    TIMESTAMP=$(date +%s)
    BACKUP_DIR="$VAULT_ROOT/.trash/backup_$TIMESTAMP"
    echo "Backing up existing vault directories to: $BACKUP_DIR"
    mkdir -p "$BACKUP_DIR"
    for folder in episodes wiki wisdom general archive .handoffs; do
        if [ -d "$VAULT_ROOT/$folder" ]; then
            echo "Moving folder to backup: $folder"
            mv "$VAULT_ROOT/$folder" "$BACKUP_DIR/"
        fi
    done
else
    echo "Vault root directory $VAULT_ROOT not found. Skipping vault backup."
fi

# 4. Wipe active RocksDB database and stale locks
echo "Wiping RocksDB database cache and stale lock files..."
rm -rf ~/.mythrax/db
rm -f ~/.mythrax/daemon.pid

# 5. Verify compiled binary is present
echo "Verifying compiled mythrax binary exists..."

# Resolve binary path
MYTHRAX_BIN="$PROJECT_ROOT/mythrax-core/target/release/mythrax"
if [ ! -f "$MYTHRAX_BIN" ]; then
    echo "Error: Compiled binary not found at $MYTHRAX_BIN"
    exit 1
fi

# 6. Run init with antigravity harness (bootstraps database, folders, config & log discovery)
# Note: This will auto-discover and ingest ~/.gemini/antigravity/brain/ transcripts.
# For large corpora this can take 15-30 min. Progress is silent — it is working.
echo "Bootstrapping fresh system and config..."
echo "NOTE: Historical transcript ingestion may take several minutes depending on corpus size."
"$MYTHRAX_BIN" init antigravity

# 7. Start daemon in background
echo "Starting daemon..."
"$MYTHRAX_BIN" daemon start --port 8090 &
DAEMON_PID=$!
echo "Daemon started with background PID $DAEMON_PID"

# Give the daemon a moment to boot
sleep 2

# 8. Reprocess any episodes missing vector embeddings (idempotent, safe to run post-ingest)
echo "Reprocessing any episodes with missing vector embeddings..."
"$MYTHRAX_BIN" vault reprocess

# 9. Run integrity verification with self-healing
echo "Verifying vault integrity (with auto-fix)..."
"$MYTHRAX_BIN" vault verify --fix

# 10. Attempt dreaming/compaction synthesis (requires LLM — skips gracefully if unavailable)
echo "Generating initial wiki compactions and synthesis (skips if no LLM configured)..."
"$MYTHRAX_BIN" vault summarize

echo "=== Reinitialization completed successfully ==="
echo ""
echo "Daemon running on port 8090 (PID: $DAEMON_PID)"
echo "To run dreaming/compaction later: mythrax vault summarize"
echo "To configure LLM: mythrax config llm --provider cloud --cloud-provider gemini"
