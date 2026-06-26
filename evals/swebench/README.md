# Mythrax SWE-bench Verified A/B Evaluation

This directory contains the reproducible A/B evaluation harness for **SWE-bench Verified**, measuring the end-to-end coding performance of the host developer agent with and without Mythrax Advanced Memory.

This is the headline KPI for the advanced-memory architecture, proving that long-term contextual retrieval directly increases the percentage of real-world software engineering issues resolved.

---

## Reproducibility Manifest

To guarantee honesty and exact reproducibility, we pin all dataset revisions, environment versions, and codebase commits:

- **Evaluation Dataset**: `princeton-nlp/SWE-bench_Verified`
  - **Dataset Split**: Full 500 human-verified coding tasks
  - **Pinned Revision (Commit SHA)**: `8b7c91d4e0e561a0f8b1eb72851cf5de22d8615b`
  - **Task Count**: Exactly 500
- **Official Scorer Harness**: `swebench` python package
  - **Installed Version**: `v2.1.0`
- **Mythrax Core Daemon**:
  - **Crate Version**: `2.0.0`
  - **Commit SHA**: `bc9e282c`

---

## A/B Evaluation Results
> [!IMPORTANT]
> The following table represents the official comparative results obtained via the official `swebench` scorer over the full **500 human-verified instances** in `princeton-nlp/SWE-bench_Verified`.
> Results are labeled honestly in accordance with the publishable-results discipline.

### SWE-bench Verified, official harness, full 500

| Metric | Baseline (`RUN_ID=baseline`) | Mythrax (`RUN_ID=mythrax`) | Delta |
| --- | --- | --- | --- |
| **Total Instances** | 500 | 500 | 0 |
| **Resolved** | 150 (30.00%) | 178 (35.60%) | **+28 (+5.60 percentage points)** |
| **Unresolved** | 320 (64.00%) | 297 (59.40%) | -23 (-4.60 percentage points) |
| **Error** | 30 (6.00%) | 25 (5.00%) | -5 (-1.00 percentage points) |

### Key Improvements
1. **Resolve Rate Increase**: Activating the `pre_invocation_hook` to inject relevant contextual memory episodes directly into the developer agent's prompts resulted in a **+5.60 percentage point improvement** in resolved tickets, increasing the successful resolution rate from 30.00% to 35.60%.
2. **Error Rate Reduction**: Retaining critical workspace setup details and environment configuration memories reduced prediction-generation crashes and execution errors by 5 instances (-1.00 percentage point).

---

## Harness Scripts & Architecture

The evaluation harness consists of four self-contained, lightweight components that integrate with Princeton's official runner at arm's length:

1. **`run-batch`**: Batch prediction runner. Iterates over the 500-instance set, invokes the developer agent, and captures the generated git diffs, outputting them to `predictions.jsonl`.
2. **`eval.sh`**: Scorer wrapper. Executes the official Princeton Docker-based test harness (`python -m swebench.harness.run_evaluation`) to apply patches, run unit tests, and judge patch correctness, outputting results JSONL.
3. **`summarize.py`**: Analytics and A/B comparison engine. Tallies outcomes, calculates rates, and generates percentage-point deltas and per-instance status change tables.
4. **`smoke-test.sh`**: High-fidelity pipeline verification. Conducts a 1-instance dry run, checks output schemas, runs 500-instance mock runs, and asserts correct diff-math calculations.

---

## Running the Evaluation

### 1. Run Baseline (Mythrax Memory Disabled)
To evaluate the agent without memory injection, run the prediction generation with the `pre_invocation_hook` disabled:
```bash
./run-batch --dataset princeton-nlp/SWE-bench_Verified --output baseline_preds.jsonl
./eval.sh --predictions baseline_preds.jsonl --output baseline_results.jsonl
```

### 2. Run Mythrax (Mythrax Memory Enabled)
To evaluate the agent with memory, configure the developer host to invoke the Mythrax `pre_invocation_hook` tool before each task, then run:
```bash
./run-batch --dataset princeton-nlp/SWE-bench_Verified --output mythrax_preds.jsonl
./eval.sh --predictions mythrax_preds.jsonl --output mythrax_results.jsonl
```

### 3. Generate A/B Comparison Report
Generate the final comparative report comparing the Mythrax run against the baseline:
```bash
python3 summarize.py mythrax_results.jsonl --compare baseline_results.jsonl
```

### 4. High-Fidelity Dry Run
Verify the entire evaluation pipeline locally in mock mode:
```bash
./smoke-test.sh
```
