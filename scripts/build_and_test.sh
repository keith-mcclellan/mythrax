#!/usr/bin/env bash
set -eo pipefail

PHASE="${1:-all}"

# Load environment scripts
if [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
fi

if [ -d ".venv" ]; then
    source .venv/bin/activate
fi

run_phase_a() {
    echo "=== Running Phase A: Core Setup & Storage Layer ==="
    cargo test --manifest-path mythrax-core/Cargo.toml
}

run_phase_b() {
    echo "=== Running Phase B: Embedding Engine & Markdown Store ==="
    cargo test --manifest-path mythrax-core/Cargo.toml
}

run_phase_c() {
    echo "=== Running Phase C: File-Watcher & WAL Recovery ==="
    cargo test --manifest-path mythrax-core/Cargo.toml
}

run_phase_d() {
    echo "=== Running Phase D: Core CLI ==="
    cargo run --manifest-path mythrax-core/Cargo.toml --bin mythrax -- --help
}

run_phase_e() {
    echo "=== Running Phase E: Python Client & Safety Gate ==="
    pytest mythrax-forge/tests/test_client.py mythrax-forge/tests/test_safety_gate.py
}

run_phase_f() {
    echo "=== Running Phase F: Sandbox Executor & Critic ==="
    pytest mythrax-forge/tests/test_executor.py mythrax-forge/tests/test_critic.py
}

run_phase_g() {
    echo "=== Running Phase G: Compaction & Dreaming ==="
    pytest mythrax-forge/tests/test_synthesis.py mythrax-forge/tests/test_compactor.py
}

run_phase_h() {
    echo "=== Running Phase H: E2E Integration ==="
    pytest tests/e2e/
}

case "$PHASE" in
    phase_a) run_phase_a ;;
    phase_b) run_phase_b ;;
    phase_c) run_phase_c ;;
    phase_d) run_phase_d ;;
    phase_e) run_phase_e ;;
    phase_f) run_phase_f ;;
    phase_g) run_phase_g ;;
    phase_h|e2e) run_phase_h ;;
    all)
        run_phase_a
        run_phase_b
        run_phase_c
        run_phase_d
        run_phase_e
        run_phase_f
        run_phase_g
        run_phase_h
        ;;
    *)
        echo "Unknown phase: $PHASE"
        exit 1
        ;;
esac
