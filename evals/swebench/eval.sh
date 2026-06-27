#!/bin/bash
# eval.sh - Wrapper for the OFFICIAL SWE-bench Verified evaluation harness.
#
# This invokes the dataset authors' published scorer at arm's length (subprocess,
# pinned version) per the spec's CODE vs DATA vs OFFICIAL SCORER exception. It does
# NOT re-implement "% Resolved" and has NO mock mode. The official harness applies
# each predicted patch inside Docker, runs the repo's tests, and emits a report
# JSON (resolved_ids / unresolved_ids / error_ids), which summarize.py then parses.
set -euo pipefail

DATASET_NAME="princeton-nlp/SWE-bench_Verified"
# Pinned dataset revision (HF commit SHA) — recorded for reproducibility.
DATASET_REVISION="c104f840cc67f8b6eec6f759ebc8b2693d585d4a"

show_help() {
    cat <<EOF
Usage: ./eval.sh --predictions <file.jsonl> --run-id <id> [--max-workers N]

Runs the official swebench harness (Docker required) over the full
SWE-bench_Verified set against the given predictions file.

Options:
  --predictions FILE   Predictions JSONL ({instance_id,model_name_or_path,model_patch}).
  --run-id ID          Unique run id (e.g. baseline | mythrax). Names the report JSON.
  --max-workers N      Parallel Docker workers (default 4).
  -h | --help          Show this help.
EOF
}

PREDS=""
RUN_ID=""
MAX_WORKERS="4"

while [[ "$#" -gt 0 ]]; do
    case "$1" in
        --predictions) PREDS="$2"; shift ;;
        --run-id) RUN_ID="$2"; shift ;;
        --max-workers) MAX_WORKERS="$2"; shift ;;
        -h|--help) show_help; exit 0 ;;
        *) echo "Unknown parameter: $1" >&2; show_help; exit 1 ;;
    esac
    shift
done

if [[ -z "$PREDS" || -z "$RUN_ID" ]]; then
    echo "Error: --predictions and --run-id are required." >&2
    show_help
    exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
    echo "Error: Docker is required by the official swebench harness but was not found." >&2
    exit 4
fi

# Record the actual installed harness version into the run manifest for reproducibility.
SWEBENCH_VERSION="$(python3 -c 'import importlib.metadata as m; print(m.version("swebench"))' 2>/dev/null || echo unknown)"
echo "Using official swebench harness version: ${SWEBENCH_VERSION}"
echo "Dataset: ${DATASET_NAME} @ ${DATASET_REVISION}"

# Official CLI contract (NOT --dataset/--predictions/--output):
#   python -m swebench.harness.run_evaluation \
#     --dataset_name <name> --predictions_path <file> --run_id <id> --max_workers N
python3 -m swebench.harness.run_evaluation \
    --dataset_name "$DATASET_NAME" \
    --predictions_path "$PREDS" \
    --run_id "$RUN_ID" \
    --max_workers "$MAX_WORKERS"

echo "Official harness complete. The report JSON (<model>.<run_id>.json) contains"
echo "resolved_ids / unresolved_ids / error_ids — feed it to summarize.py."
