#!/usr/bin/env bash
# maintain_mythrax.sh - Maintenance, recovery, and incremental execution tool for Mythrax
set -euo pipefail

# Resolve project root relative to this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MYTHRAX_BIN="$PROJECT_ROOT/mythrax-core/target/release/mythrax"

# Resolve VAULT_ROOT from ~/.mythrax/config.json
VAULT_ROOT="~/mythrax-vault"
if [ -f ~/.mythrax/config.json ]; then
    CONFIG_VAULT=$(grep -o '"vault_root": *"[^"]*"' ~/.mythrax/config.json | cut -d'"' -f4 || true)
    if [ -n "$CONFIG_VAULT" ]; then
        VAULT_ROOT="$CONFIG_VAULT"
    fi
fi
VAULT_ROOT="${VAULT_ROOT/#\~/$HOME}"

# Verify compiled binary is present
if [ ! -f "$MYTHRAX_BIN" ]; then
    echo "Error: Compiled binary not found at $MYTHRAX_BIN"
    echo "Please build it first: cd $PROJECT_ROOT/mythrax-core && cargo build --release"
    exit 1
fi

show_status() {
    echo "=== Mythrax System Status ==="
    echo "Binary path: $MYTHRAX_BIN"
    echo "Vault root:  $VAULT_ROOT"
    echo ""
    echo "--- Running Processes ---"
    if pgrep -f "mythrax daemon" > /dev/null; then
        echo "✅ Daemon is running (PIDs: $(pgrep -f "mythrax daemon" | tr '\n' ' '))"
    else
        echo "❌ Daemon is NOT running"
    fi
    if pgrep -f "mythrax_mcp" > /dev/null; then
        echo "✅ MCP Server is running (PIDs: $(pgrep -f "mythrax_mcp" | tr '\n' ' '))"
    else
        echo "ℹ️  No active mythrax_mcp processes"
    fi
    echo ""
    echo "--- Database Lock Status ---"
    if [ -f ~/.mythrax/daemon.pid ]; then
        echo "⚠️  Daemon PID file exists: ~/.mythrax/daemon.pid (Value: $(cat ~/.mythrax/daemon.pid || echo "empty"))"
    else
        echo "✅ No stale daemon.pid file found"
    fi
    if [ -d ~/.mythrax/db ] && [ -f ~/.mythrax/db/LOCK ]; then
        echo "ℹ️  RocksDB Lock file present (normal when database is active or was uncleanly closed)"
    fi
    echo ""
    echo "--- CLI Status Check ---"
    if "$MYTHRAX_BIN" status 2>/dev/null; then
        echo "✅ CLI connection check passed"
    else
        echo "❌ CLI connection check failed (is daemon running?)"
    fi
}

stop_processes() {
    echo "Stopping daemon and any running mythrax processes..."
    pkill -f "mythrax daemon" || true
    pkill -f "mythrax_mcp" || true
    pkill -f "target/release/mythrax" || true
    pkill -f "target/debug/mythrax" || true
    sleep 1
    echo "All processes stopped."
}

unlock_db() {
    echo "Wiping stale PID files and checking locks..."
    rm -f ~/.mythrax/daemon.pid
    # RocksDB lock is released when processes are stopped, but we clean it up if safe
    if [ -f ~/.mythrax/db/LOCK ]; then
        echo "Stale RocksDB LOCK file checked (will be automatically grabbed/released by the binary)."
    fi
    echo "Locks cleaned up."
}

run_ingest() {
    echo "=== 1. Incremental Ingestion ==="
    echo "Note: This is fully idempotent and updates records/files without wiping your database."
    HISTORY_SRC="$HOME/.gemini/antigravity/brain"
    if [ -d "$HISTORY_SRC" ]; then
        echo "Ingesting from: $HISTORY_SRC"
        "$MYTHRAX_BIN" vault ingest --source "$HISTORY_SRC" --harness antigravity --scope history
    else
        echo "Error: Default history source folder not found at $HISTORY_SRC"
        exit 1
    fi
}

run_reprocess() {
    echo "=== 2. Vector Reprocessing ==="
    echo "Calculating missing embeddings using local/ONNX models..."
    "$MYTHRAX_BIN" vault reprocess
}

run_verify() {
    echo "=== 3. Vault Verification & Healing ==="
    echo "Verifying database integrity against vault markdown files..."
    "$MYTHRAX_BIN" vault verify --fix
}

run_summarize() {
    echo "=== 4. Synthesis & Compaction ==="
    echo "Running LLM summarization (skips gracefully if LLM is unavailable)..."
    "$MYTHRAX_BIN" vault summarize || echo "Warning: Summarize skipped or failed. Continuing..."
}

start_daemon() {
    echo "=== 5. Starting Daemon ==="
    # Make sure port 8090 is clear
    stop_processes
    unlock_db
    echo "Launching daemon in background on port 8090..."
    "$MYTHRAX_BIN" daemon start --port 8090 &
    DAEMON_PID=$!
    sleep 2
    echo "Daemon running with background PID $DAEMON_PID"
}

resume_all() {
    echo "=== Mythrax Full Recovery Pipeline ==="
    # 1. First make sure processes are stopped to release database locks
    stop_processes
    unlock_db
    
    # 2. Run sequential recovery tasks
    run_ingest
    run_reprocess
    run_verify
    run_summarize
    
    # 3. Start daemon back up
    start_daemon
    echo "=== Recovery Pipeline Completed Successfully ==="
}

show_menu() {
    echo "========================================="
    echo "      Mythrax Maintenance Utility        "
    echo "========================================="
    echo "1) Status check"
    echo "2) Stop all processes"
    echo "3) Clean lock files (Unlock DB)"
    echo "4) Resume/Run ingestion (Idempotent)"
    echo "5) Run vector embedding generation"
    echo "6) Run vault verify (with auto-fix)"
    echo "7) Run dreaming/summarization"
    echo "8) Start daemon in background"
    echo "9) Run FULL pipeline (Resume & Start)"
    echo "q) Quit"
    echo "========================================="
    read -r -p "Enter choice [1-9, q]: " choice
    case "$choice" in
        1) show_status ;;
        2) stop_processes ;;
        3) unlock_db ;;
        4) run_ingest ;;
        5) run_reprocess ;;
        6) run_verify ;;
        7) run_summarize ;;
        8) start_daemon ;;
        9) resume_all ;;
        q|Q) exit 0 ;;
        *) echo "Invalid option." ;;
    esac
}

# Main command dispatcher
COMMAND=${1:-""}

case "$COMMAND" in
    status)    show_status ;;
    stop)      stop_processes ;;
    unlock)    unlock_db ;;
    ingest)    run_ingest ;;
    reprocess) run_reprocess ;;
    verify)    run_verify ;;
    summarize) run_summarize ;;
    start)     start_daemon ;;
    resume-all) resume_all ;;
    help|--help|-h)
        echo "Usage: $0 [status|stop|unlock|ingest|reprocess|verify|summarize|start|resume-all|help]"
        exit 0
        ;;
    "")
        # Run menu interactively
        while true; do
            show_menu
            echo ""
            read -r -p "Press Enter to continue..."
            echo ""
        done
        ;;
    *)
        echo "Unknown command: $COMMAND"
        echo "Run '$0 help' for usage instructions."
        exit 1
        ;;
esac
