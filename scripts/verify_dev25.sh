#!/bin/bash
set -e

# Change directory to mythrax-core folder (where Cargo.toml and bench_data reside)
cd "$(dirname "$0")/../mythrax-core"

# Run dev25 benchmark
echo "=== Running dev25 Benchmark ==="
cargo run --features mlx,bench --release --bin bench -- --split dev25 --mode hybrid

# Verify against baseline
echo "=== Verifying dev25 Regression Gate ==="
python3 -c "
import json, sys

baseline_path = 'bench_data/BASELINE_DEV25.json'
results_path = 'bench_data/results_dev25.jsonl'

try:
    with open(baseline_path) as f:
        baseline = json.load(f)
except Exception as e:
    print(f'Error reading baseline file: {e}')
    sys.exit(1)

try:
    with open(results_path) as f:
        manifest = json.loads(f.readline())
        records = [json.loads(line) for line in f]
except Exception as e:
    print(f'Error reading results file: {e}')
    sys.exit(1)

if not records:
    print('Error: No records found in results file')
    sys.exit(1)

recalls = [r['recall_any_turn_at5'] for r in records]
ndcgs = [r['ndcg_turn_at10'] for r in records]
recall_alls = [r['recall_all_turn_at5'] for r in records]

avg_recall = sum(recalls) / len(recalls)
avg_ndcg = sum(ndcgs) / len(ndcgs)
avg_recall_all = sum(recall_alls) / len(recall_alls)

print(f'Current Recall_Any@5: {avg_recall:.4f} (Baseline: {baseline[\"recall_any_5\"]:.4f})')
print(f'Current Recall_All@5: {avg_recall_all:.4f} (Baseline: {baseline[\"recall_all_5\"]:.4f})')
print(f'Current nDCG@10:       {avg_ndcg:.4f} (Baseline: {baseline[\"ndcg_10\"]:.4f})')

# Allow 0.0001 precision/floating point tolerance
precision_tolerance = -0.0001

if (avg_recall - baseline['recall_any_5']) < precision_tolerance:
    print('REJECT: Recall_Any@5 has regressed!')
    sys.exit(1)
if (avg_ndcg - baseline['ndcg_10']) < precision_tolerance:
    print('REJECT: nDCG@10 has regressed!')
    sys.exit(1)

print('PASS: dev25 benchmark regression check passed.')
"
