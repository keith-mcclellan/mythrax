#!/bin/bash
# smoke-test.sh - Smoke test for SWE-bench Verified A/B harness

set -e

echo "=== Running SWE-bench Harness Smoke Test ==="

# 1. Run predictions in mock mode
echo "1. Generating mock predictions..."
./run-batch --mock --output smoke_preds.jsonl

# 2. Assert predictions.jsonl schema
echo "2. Verifying predictions.jsonl schema..."
if ! grep -q "instance_id" smoke_preds.jsonl || ! grep -q "model_name_or_path" smoke_preds.jsonl || ! grep -q "model_patch" smoke_preds.jsonl; then
    echo "FAIL: predictions.jsonl is missing required schema fields."
    exit 1
fi
echo "Pass: Schema fields verified."

# 3. Create a mock single-instance evaluation result
echo "3. Generating single-instance evaluation result..."
echo '{"instance_id": "django__django-11111", "status": "resolved"}' > smoke_res.jsonl

# 4. Verify summarize.py single-run output
echo "4. Running summarize.py on single-run results..."
SUM_OUT=$(python3 summarize.py smoke_res.jsonl)
echo "$SUM_OUT"
if [[ "$SUM_OUT" != *"Resolved: 1 (100.00%)"* ]]; then
    echo "FAIL: summarize.py did not emit the expected 'Resolved: X (Y%)' line."
    exit 1
fi
echo "Pass: Single-run summary verified."

# 5. Run mock evaluation to generate A/B baseline and mythrax files (500 instances)
echo "5. Generating 500-instance A/B mock runs..."
./eval.sh --mock

# 6. Verify A/B comparison and diff math
echo "6. Verifying A/B comparison and diff math..."
COMP_OUT=$(python3 summarize.py mock_mythrax.jsonl --compare mock_baseline.jsonl)
echo "$COMP_OUT"

# Verify resolve-rate delta is printed and correct
# Baseline: 150/500 = 30.00%
# Mythrax: 178/500 = 35.60%
# Delta: +28 (+5.60 percentage points)
if [[ "$COMP_OUT" != *"+5.60 percentage points"* ]]; then
    echo "FAIL: A/B comparison did not emit correct resolve-rate delta (+5.60 percentage points)."
    exit 1
fi
if [[ "$COMP_OUT" != *"+28"* ]]; then
    echo "FAIL: A/B comparison did not emit correct resolved count delta (+28)."
    exit 1
fi
echo "Pass: A/B comparison diff math verified."

# Clean up temp files
rm -f smoke_preds.jsonl smoke_res.jsonl

echo "=== SMOKE TEST PASSED SUCCESSFULLY ==="
exit 0
