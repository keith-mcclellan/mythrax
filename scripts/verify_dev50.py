import json
import sys
import subprocess
import os
import time

# Change directory to mythrax-core
script_dir = os.path.dirname(os.path.abspath(__file__))
os.chdir(os.path.join(script_dir, "../mythrax-core"))

# Run benchmark
print("=== Running dev50 Benchmark ===")
res = subprocess.run([
    "cargo", "run", "--features", "mlx,bench", "--release", "--bin", "bench", 
    "--", "--split", "dev50", "--mode", "hybrid"
])
if res.returncode != 0:
    print("Benchmark binary execution failed!")
    sys.exit(res.returncode)

# Verify metrics
print("=== Verifying dev50 Regression Gate ===")
baseline_path = 'bench_data/BASELINE_DEV50.json'
results_path = 'bench_data/results_dev50.jsonl'

try:
    with open(baseline_path) as f:
        baseline = json.load(f)
except Exception as e:
    print(f'Error reading baseline file: {e}')
    sys.exit(1)

try:
    with open(results_path) as f:
        manifest = json.loads(f.readline())
        records = [json.loads(line) for line in f if line.strip()]
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

passed = True

if (avg_recall - baseline['recall_any_5']) < precision_tolerance:
    print('REJECT: Recall_Any@5 has regressed!')
    passed = False
if (avg_recall_all - baseline['recall_all_5']) < precision_tolerance:
    print('REJECT: Recall_All@5 has regressed!')
    passed = False
if (avg_ndcg - baseline['ndcg_10']) < precision_tolerance:
    print('REJECT: nDCG@10 has regressed!')
    passed = False
if avg_latency > (baseline['avg_latency_ms'] * latency_tolerance_ratio):
    print(f'REJECT: Average latency has regressed beyond 15% limit! ({avg_latency:.2f}ms vs baseline {baseline["avg_latency_ms"]:.2f}ms)')
    passed = False

commit_hash = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
timestamp = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())

# Write history log
try:
    with open('bench_data/dev50_history.jsonl', 'a') as history_file:
        history_file.write(json.dumps({
            "commit": commit_hash,
            "timestamp": timestamp,
            "recall_any_5": avg_recall,
            "recall_all_5": avg_recall_all,
            "ndcg_10": avg_ndcg,
            "avg_latency_ms": avg_latency,
            "status": "PASS" if passed else "REJECT",
            "evidence": results_path
        }) + "\n")
except Exception as e:
    print(f"Warning: Failed to write history file: {e}")

# Write active state
try:
    with open('bench_data/dev50_state.json', 'w') as state_file:
        json.dump({
            "active_commit": commit_hash,
            "status": "PASS" if passed else "REJECT",
            "confirmed_by": f"confirmed:{results_path}",
            "metrics": {
                "recall_any_5": avg_recall,
                "recall_all_5": avg_recall_all,
                "ndcg_10": avg_ndcg,
                "avg_latency_ms": avg_latency
            },
            "delta": {
                "recall_any_5": avg_recall - baseline['recall_any_5'],
                "recall_all_5": avg_recall_all - baseline['recall_all_5'],
                "ndcg_10": avg_ndcg - baseline['ndcg_10'],
                "avg_latency_ms": avg_latency - baseline['avg_latency_ms']
            },
            "updated_at": timestamp
        }, state_file, indent=2)
except Exception as e:
    print(f"Warning: Failed to write state file: {e}")

if not passed:
    sys.exit(1)

print('PASS: dev50 benchmark regression check passed.')
