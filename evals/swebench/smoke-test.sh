#!/bin/bash
# smoke-test.sh - Pipeline verification for the SWE-bench A/B harness.
#
# This exercises summarize.py against the OFFICIAL harness report-JSON format
# (resolved_ids / unresolved_ids / error_ids) using small, clearly-labeled test
# fixtures. It contains NO fabricated "results" and asserts NO pre-baked win
# (the old +5.60pp / +28 assertions are gone). It also verifies that run-batch
# and eval.sh have no mock mode.
set -euo pipefail

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "=== SWE-bench Harness Smoke Test (no fabricated results) ==="

# 1. No mock mode may exist in the executable harness (run-batch / eval.sh).
echo "1. Asserting no mock mode remains in run-batch/eval.sh..."
if grep -qi -- "--mock\|mock_baseline\|mock_mythrax" run-batch eval.sh; then
    echo "FAIL: a mock mode / mock data reference still exists." >&2
    exit 1
fi
echo "Pass: no mock mode."

# 2. summarize.py parses the OFFICIAL report format (single run).
echo "2. summarize.py single-run over official report format..."
cat > "$TMP/run_a.json" <<'JSON'
{"total_instances": 3, "resolved_ids": ["astropy__astropy-1"], "unresolved_ids": ["sympy__sympy-2"], "error_ids": ["django__django-3"]}
JSON
OUT_A="$(python3 summarize.py "$TMP/run_a.json")"
echo "$OUT_A"
echo "$OUT_A" | grep -q "Resolved: 1 (33.33%)" || { echo "FAIL: single-run resolve line wrong." >&2; exit 1; }
echo "Pass: official report parsed."

# 3. A/B diff math over two official-format fixtures (test_summarize_diff_math).
echo "3. summarize.py --compare diff math..."
cat > "$TMP/baseline.json" <<'JSON'
{"total_instances": 3, "resolved_ids": ["astropy__astropy-1"], "unresolved_ids": ["sympy__sympy-2", "django__django-3"], "error_ids": []}
JSON
cat > "$TMP/mythrax.json" <<'JSON'
{"total_instances": 3, "resolved_ids": ["astropy__astropy-1", "sympy__sympy-2"], "unresolved_ids": ["django__django-3"], "error_ids": []}
JSON
# baseline resolved 1/3 = 33.33%; mythrax resolved 2/3 = 66.67%; delta = +1 (+33.33 pp)
CMP="$(python3 summarize.py "$TMP/mythrax.json" --compare "$TMP/baseline.json")"
echo "$CMP"
echo "$CMP" | grep -q "+1 (+33.33 percentage points)" || { echo "FAIL: resolve delta math wrong." >&2; exit 1; }
echo "$CMP" | grep -q "sympy__sympy-2 | unresolved | resolved | Improved" || { echo "FAIL: per-instance change row missing." >&2; exit 1; }
echo "Pass: diff math + per-instance change table verified."

# 4. eval.sh uses the official CLI contract.
echo "4. eval.sh official CLI contract..."
grep -q "run_evaluation" eval.sh \
  && grep -q -- "--predictions_path" eval.sh \
  && grep -q -- "--run_id" eval.sh \
  && grep -q -- "--dataset_name" eval.sh \
  || { echo "FAIL: eval.sh does not match the official harness CLI." >&2; exit 1; }
echo "Pass: official CLI contract present."

echo "=== SMOKE TEST PASSED (no fabricated results) ==="
