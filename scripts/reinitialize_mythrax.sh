#!/usr/bin/env bash
# reinitialize_mythrax.sh - Automates database/vault cleanup and re-ingestion
set -euo pipefail

echo "=== Mythrax Reinitialization Protocol ==="

# 1. Stop active daemon and kill running processes to release file locks
echo "Stopping active daemon and any running mythrax processes..."
pkill -f "mythrax daemon" || true
pkill -f "mythrax_mcp" || true
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
    for folder in episodes wiki wisdom general archive; do
        if [ -d "$VAULT_ROOT/$folder" ]; then
            echo "Moving folder to backup: $folder"
            mv "$VAULT_ROOT/$folder" "$BACKUP_DIR/"
        fi
    done
else
    echo "Vault root directory $VAULT_ROOT not found. Skipping vault backup."
fi

# 4. Wipe active RocksDB database
echo "Wiping RocksDB database cache..."
rm -rf ~/.mythrax/db

# 5. Build in release mode
echo "Building mythrax binary in release mode..."
cargo build --release --manifest-path mythrax-core/Cargo.toml

# Resolve binary path
MYTHRAX_BIN="./mythrax-core/target/release/mythrax"
if [ ! -f "$MYTHRAX_BIN" ]; then
    echo "Error: Compiled binary not found at $MYTHRAX_BIN"
    exit 1
fi

# 6. Run init with antigravity harness (bootstraps database, folders, and runs configuration & logs discovery)
echo "Bootstrapping fresh system and config..."
"$MYTHRAX_BIN" init antigravity

# 7. Start daemon in background
echo "Starting daemon..."
"$MYTHRAX_BIN" daemon start --port 8090 &
DAEMON_PID=$!
echo "Daemon started with background PID $DAEMON_PID"

# Give the daemon a moment to boot
sleep 2

# 8. Generate initial summaries
echo "Generating initial wiki compactions and synthesis..."
"$MYTHRAX_BIN" vault summarize

# 9. Run integrity verification to assert valid vector embeddings
echo "Verifying vault integrity..."
"$MYTHRAX_BIN" vault verify

echo "=== Reinitialization completed successfully ==="

