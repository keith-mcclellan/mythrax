import json
import sys
import subprocess
import os

# Change directory to mythrax-core
script_dir = os.path.dirname(os.path.abspath(__file__))
os.chdir(os.path.join(script_dir, "../mythrax-core"))

# Run benchmark
print("=== Running dev25 Benchmark ===")
res = subprocess.run([
    "cargo", "run", "--features", "mlx,bench", "--release", "--bin", "bench", 
    "--", "--split", "dev25", "--mode", "hybrid"
])
if res.returncode != 0:
    print("Benchmark binary execution failed!")
    sys.exit(res.returncode)

# Verify metrics
print("=== Verifying dev25 Regression Gate ===")
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
latencies = [r['query_latency_ms'] for r in records]

avg_recall = sum(recalls) / len(recalls)
avg_ndcg = sum(ndcgs) / len(ndcgs)
avg_recall_all = sum(recall_alls) / len(recall_alls)
avg_latency = sum(latencies) / len(latencies)

print(f'Current Recall_Any@5: {avg_recall:.4f} (Baseline: {baseline["recall_any_5"]:.4f})')
print(f'Current Recall_All@5: {avg_recall_all:.4f} (Baseline: {baseline["recall_all_5"]:.4f})')
print(f'Current nDCG@10:       {avg_ndcg:.4f} (Baseline: {baseline["ndcg_10"]:.4f})')
print(f'Current Avg Latency:   {avg_latency:.2f}ms (Baseline: {baseline["avg_latency_ms"]:.2f}ms)')

# Allow 0.0001 precision/floating point tolerance
precision_tolerance = -0.0001
latency_tolerance_ratio = 1.15 # Max 15% latency degradation

if (avg_recall - baseline['recall_any_5']) < precision_tolerance:
    print('REJECT: Recall_Any@5 has regressed!')
    sys.exit(1)
if (avg_ndcg - baseline['ndcg_10']) < precision_tolerance:
    print('REJECT: nDCG@10 has regressed!')
    sys.exit(1)
if avg_latency > (baseline['avg_latency_ms'] * latency_tolerance_ratio):
    print(f'REJECT: Average latency has regressed beyond 15% limit! ({avg_latency:.2f}ms vs baseline {baseline["avg_latency_ms"]:.2f}ms)')
    sys.exit(1)

print('PASS: dev25 benchmark regression check passed.')
