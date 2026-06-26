#!/bin/bash
# eval.sh - Wrapper for SWE-bench Verified evaluation

show_help() {
    echo "Usage: ./eval.sh [options]"
    echo "Options:"
    echo "  --mock               Run in mock evaluation mode (generates mock baseline and mythrax JSONL files for 500 instances)."
    echo "  --predictions FILE   Path to predictions JSONL file."
    echo "  --output FILE        Path to output results JSONL file."
}

MOCK=false
PREDS=""
OUT=""

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --mock) MOCK=true ;;
        --predictions) PREDS="$2"; shift ;;
        --output) OUT="$2"; shift ;;
        -h|--help) show_help; exit 0 ;;
        *) echo "Unknown parameter: $1"; show_help; exit 1 ;;
    esac
    shift
done

if [ "$MOCK" = true ]; then
    echo "Running in mock evaluation mode..."
    
    # Generate mock baseline
    echo "Generating mock_baseline.jsonl..."
    rm -f mock_baseline.jsonl
    for i in $(seq 1 500); do
        inst_id="django__django-$((10000 + i))"
        # 30% resolved (150 instances), 6% error (30 instances), rest unresolved (320 instances)
        if [ $i -le 150 ]; then
            status="resolved"
        elif [ $i -le 180 ]; then
            status="error"
        else
            status="unresolved"
        fi
        echo "{\"instance_id\": \"$inst_id\", \"status\": \"$status\"}" >> mock_baseline.jsonl
    done
    
    # Generate mock mythrax (improved: 35.6% resolved, 5% error)
    echo "Generating mock_mythrax.jsonl..."
    rm -f mock_mythrax.jsonl
    for i in $(seq 1 500); do
        inst_id="django__django-$((10000 + i))"
        # We improve: some baseline unresolved/errors become resolved
        # Baseline resolved (1..150) remain resolved
        # Baseline errors (151..180): 151..155 become resolved, 156..180 remain error (except 156..160 become unresolved)
        # Baseline unresolved (181..500): 181..203 become resolved
        if [ $i -le 150 ]; then
            status="resolved"
        elif [ $i -le 155 ]; then
            status="resolved" # Improved from error
        elif [ $i -le 160 ]; then
            status="unresolved" # Improved from error
        elif [ $i -le 180 ]; then
            status="error"
        elif [ $i -le 203 ]; then
            status="resolved" # Improved from unresolved
        else
            status="unresolved"
        fi
        echo "{\"instance_id\": \"$inst_id\", \"status\": \"$status\"}" >> mock_mythrax.jsonl
    done
    
    echo "Mock evaluation complete. Generated mock_baseline.jsonl and mock_mythrax.jsonl."
    exit 0
fi

# Real execution
if [ -z "$PREDS" ] || [ -z "$OUT" ]; then
    echo "Error: --predictions and --output are required for real runs."
    exit 1
fi

echo "Executing official SWE-bench evaluation harness..."
python3 -m swebench.harness.run_evaluation \
    --dataset princeton-nlp/SWE-bench_Verified \
    --predictions "$PREDS" \
    --output "$OUT"
