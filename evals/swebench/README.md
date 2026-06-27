# Mythrax SWE-bench Verified A/B Evaluation — Scaffolding

> [!WARNING]
> **No results recorded yet.** This directory is runnable scaffolding for an A/B
> evaluation on SWE-bench Verified. No real run has been executed in this
> environment (it requires Docker + a configured Mythrax daemon + the official
> `swebench` harness). **No number in this directory was produced by a scorer.**
> To generate results, run `./run-batch` for each arm and then `./eval.sh`, then
> `python3 summarize.py`. Until then there is intentionally no results table here.

This harness measures the end-to-end coding performance of the host developer
agent **with** vs. **without** Mythrax Advanced Memory, scored by the dataset
authors' own official harness ("% Resolved"). It is the intended headline KPI for
the advanced-memory program — but only once a real run exists.

---

## Reproducibility Manifest (pins)

- **Evaluation Dataset**: `princeton-nlp/SWE-bench_Verified`
  - **Pinned Revision (HF commit SHA)**: `c104f840cc67f8b6eec6f759ebc8b2693d585d4a`
  - **Instance Count**: 500 human-verified (asserted by `run-batch` before proceeding)
- **Official Scorer Harness**: `swebench` python package
  - **Installed Version**: _recorded at run time by `eval.sh`_ (`python -c 'import importlib.metadata,...'`).
    Not pinned here because no run has been executed; `eval.sh` stamps the actual
    installed version into its output.
- **Mythrax Core Daemon**:
  - **Branch**: `feature/v2.1.0-advanced-memory`
  - **Commit SHA**: recorded by each run's manifest from `git rev-parse HEAD`
    (this branch is several commits ahead of the 2.0.0 release `bc9e282c`; do not
    cite `bc9e282c` as the evaluated commit).

---

## Harness Scripts

1. **`run-batch`** — drives the Mythrax-backed developer agent over the pinned
   500-instance set and writes `predictions.jsonl` in the **official schema**
   `{instance_id, model_name_or_path, model_patch}`. No mock mode; never fabricates
   a patch. `--use-memory` toggles the Mythrax `pre_invocation_hook` context
   injection (the A/B switch: omit for the `baseline` arm, pass for the `mythrax` arm).
2. **`eval.sh`** — wraps the **official** scorer with its real CLI:
   `python -m swebench.harness.run_evaluation --dataset_name … --predictions_path … --run_id … --max_workers …`
   (Docker required). It does not re-implement "% Resolved".
3. **`summarize.py`** — parses the official harness **report JSON**
   (`resolved_ids` / `unresolved_ids` / `error_ids`), not a pre-baked status field;
   `--compare` produces the resolve-rate delta and per-instance status-change table.
4. **`smoke-test.sh`** — verifies the pipeline against official-format fixtures and
   asserts no mock mode remains. It asserts **no** specific win/delta.

---

## Running the Evaluation

### 1. Baseline (Mythrax memory disabled)
```bash
./run-batch --model-name baseline --output baseline_preds.jsonl
./eval.sh --predictions baseline_preds.jsonl --run-id baseline
```

### 2. Mythrax (memory enabled)
```bash
./run-batch --model-name mythrax --use-memory --output mythrax_preds.jsonl
./eval.sh --predictions mythrax_preds.jsonl --run-id mythrax
```

### 3. A/B comparison report
The official harness writes a report JSON per run (e.g. `mythrax.<run_id>.json`).
Compare them:
```bash
python3 summarize.py <mythrax_report>.json --compare <baseline_report>.json
```
Results, once generated, must be labeled exactly:
**"SWE-bench Verified, official harness v<X>, full 500."**

### 4. Pipeline smoke test (no Docker/daemon needed)
```bash
./smoke-test.sh
```
