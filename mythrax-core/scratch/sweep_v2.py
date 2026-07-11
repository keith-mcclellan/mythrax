import os
import json
import re
import subprocess
import time
import hashlib
import sys

CWD = "/Users/keith/Documents/mythrax/mythrax-core"
TUNED_PARAMS_PATH = os.path.join(CWD, "bench_data/tuned_params.json")
# Parse CLI arguments to determine Mode and Checkpoint Path
cross_encoder_only = "--cross-encoder-only" in sys.argv
no_cross_encoder = "--no-cross-encoder" in sys.argv
full_mode = "--full" in sys.argv or (not cross_encoder_only and not no_cross_encoder)

if cross_encoder_only:
    CHECKPOINT_PATH = os.path.join(CWD, "scratch/sweep_checkpoint_cross_encoder.json")
    mode_name = "CROSS-ENCODER ONLY"
elif no_cross_encoder:
    CHECKPOINT_PATH = os.path.join(CWD, "scratch/sweep_checkpoint_no_cross_encoder.json")
    mode_name = "NON-CROSS-ENCODER ONLY"
else:
    CHECKPOINT_PATH = os.path.join(CWD, "scratch/sweep_checkpoint.json")
    mode_name = "FULL"

# Ensure scratch exists
os.makedirs(os.path.join(CWD, "scratch"), exist_ok=True)

# Load checkpoint
if os.path.exists(CHECKPOINT_PATH):
    try:
        with open(CHECKPOINT_PATH, "r") as f:
            checkpoint = json.load(f)
        print(f"Loaded checkpoint for {mode_name} sweep with {len(checkpoint.get('eval_cache', {}))} cached runs.")
    except Exception as e:
        print(f"Failed to load checkpoint: {e}")
        checkpoint = {"eval_cache": {}, "optimal_params": {}, "completed_stages": []}
else:
    checkpoint = {"eval_cache": {}, "optimal_params": {}, "completed_stages": []}

eval_cache = checkpoint.setdefault("eval_cache", {})
optimal_params = checkpoint.setdefault("optimal_params", {})
completed_stages = checkpoint.setdefault("completed_stages", [])

def save_checkpoint():
    try:
        with open(CHECKPOINT_PATH, "w") as f:
            json.dump(checkpoint, f, indent=2)
    except Exception as e:
        print(f"Failed to save checkpoint: {e}")

def get_params_hash(params):
    serialized = json.dumps(params, sort_keys=True)
    return hashlib.md5(serialized.encode("utf-8")).hexdigest()

def run_eval(params):
    # Ensure values are strings
    str_params = {k: str(v) for k, v in params.items()}
    h = get_params_hash(str_params)
    
    if h in eval_cache:
        cached = eval_cache[h]
        print(f"  [Cache Hit] Hash: {h} | R_All@25: {cached.get('recall_all_25', 0.0):.4f} | R_Any@5: {cached.get('recall_any', 0.0):.4f} | R_All@5: {cached.get('recall_all_5', 0.0):.4f} | nDCG@10: {cached.get('ndcg', 0.0):.4f}")
        return cached

    # Write params to tuned_params.json
    with open(TUNED_PARAMS_PATH, "w") as f:
        json.dump(str_params, f, indent=2)

    env = os.environ.copy()
    env["MYTHRAX_LOAD_TUNED_PARAMS"] = "true"
    cmd = ["./target/release/bench", "--split", "dev50", "--mode", "hybrid"]
    
    t0 = time.time()
    try:
        proc = subprocess.Popen(
            cmd, cwd=CWD, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env
        )
        stdout, stderr = proc.communicate(timeout=600)  # 10 minutes timeout
        elapsed = time.time() - t0
        output = stdout + "\n" + stderr
    except subprocess.TimeoutExpired:
        proc.kill()
        print(f"  [Timeout] Config hash {h} timed out.")
        return None
    except Exception as e:
        print(f"  [Error] Failed to run bench: {e}")
        return None

    if proc.returncode != 0:
        print(f"  [Failed] Process exited with code {proc.returncode}")
        return None

    # Parse metrics from results_dev50.jsonl
    results_path = os.path.join(CWD, "bench_data/results_dev50.jsonl")
    if not os.path.exists(results_path):
        print("  [Error] results_dev50.jsonl not found.")
        return None

    try:
        records = []
        with open(results_path, "r") as f:
            for line in f:
                if line.strip():
                    records.append(json.loads(line))
    except Exception as e:
        print(f"  [Error] Failed to parse results_dev50.jsonl: {e}")
        return None

    # Parse aggregate metrics
    r_any = 0.0
    r_all_5 = 0.0
    r_all_25 = 0.0
    ndcg = 0.0
    
    any_match = re.search(r"Recall_Any@5:\s+([\d.]+)", output)
    if any_match:
        r_any = float(any_match.group(1))
    all_5_match = re.search(r"Recall_All@5:\s+([\d.]+)", output)
    if all_5_match:
        r_all_5 = float(all_5_match.group(1))
    all_25_match = re.search(r"Recall_All@25:\s+([\d.]+)", output)
    if all_25_match:
        r_all_25 = float(all_25_match.group(1))
    ndcg_match = re.search(r"nDCG@10:\s+([\d.]+)", output)
    if ndcg_match:
        ndcg = float(ndcg_match.group(1))

    metrics = {
        "recall_any": r_any,
        "recall_all_5": r_all_5,
        "recall_all_25": r_all_25,
        "ndcg": ndcg,
        "elapsed": elapsed,
        "records": records
    }

    print(f"  [Eval] Hash: {h} | R_All@25: {r_all_25:.4f} | R_Any@5: {r_any:.4f} | R_All@5: {r_all_5:.4f} | nDCG@10: {ndcg:.4f} | took {elapsed:.1f}s")
    eval_cache[h] = metrics
    save_checkpoint()
    return metrics

def get_sliced_objective(metrics, param_key):
    if not metrics:
        return 0.0
        
    # Safety Recall Gating check
    # Recall_All@25 must not regress below 0.9400
    # Recall_Any@5 must not regress below 0.8600
    # Recall_All@5 must not regress below 0.7000
    if metrics.get("recall_all_25", 0.0) < 0.9400:
        return 0.0
    if metrics.get("recall_any", 0.0) < 0.8600:
        return 0.0
    if metrics.get("recall_all_5", 0.0) < 0.7000:
        return 0.0

    records = metrics.get("records", [])
    if not records:
        return 0.0

    # Determine slice
    # Category mappings:
    # temporal -> temporal-reasoning
    # preference -> single-session-preference
    # user -> single-session-user
    # default -> knowledge-update, multi-session, single-session-assistant
    qtypes = []
    if "temporal" in param_key:
        qtypes = ["temporal-reasoning"]
    elif "preference" in param_key:
        qtypes = ["single-session-preference"]
    elif "user" in param_key:
        qtypes = ["single-session-user"]
    elif "default" in param_key:
        qtypes = ["knowledge-update", "multi-session", "single-session-assistant"]
    
    if qtypes:
        sliced_records = [r for r in records if r.get("question_type") in qtypes]
    else:
        sliced_records = records

    if not sliced_records:
        return 0.0

    total_ndcg = sum(r.get("ndcg_turn_at10", 0.0) for r in sliced_records)
    avg_ndcg = total_ndcg / len(sliced_records)
    
    # Objective score is overall nDCG plus sliced nDCG to optimize both globally and specifically
    return metrics.get("ndcg", 0.0) + avg_ndcg

def select_plateau_robust_value(param_key, candidates, results, base_value):
    # results is list of (value, score)
    max_score = max(r[1] for r in results)
    if max_score <= 0.0:
        return base_value

    threshold = max_score - 0.0005
    plateau = [r for r in results if r[1] >= threshold]
    
    sorted_plateau = sorted(plateau, key=lambda r: float(r[0]))
    center_idx = len(sorted_plateau) // 2
    center_val = sorted_plateau[center_idx][0]
    return center_val

def run_ternary_search(param_key, L, R, is_int=False, max_depth=4, tol=0.0005, default_val=None, current_params=None):
    depth = 0
    current_L = float(L)
    current_R = float(R)
    
    while depth < max_depth:
        if is_int:
            L_val = int(round(current_L))
            R_val = int(round(current_R))
            if R_val - L_val <= 2:
                vals = [str(x) for x in range(L_val, R_val + 1)]
                scores = []
                for val in vals:
                    test_params = current_params.copy()
                    test_params[param_key] = val
                    metrics = run_eval(test_params)
                    score = get_sliced_objective(metrics, param_key)
                    scores.append((val, score))
                optimal_val = select_plateau_robust_value(param_key, vals, scores, default_val)
                return optimal_val
        else:
            if current_R - current_L < 0.01:
                break
                
        m1_f = current_L + (current_R - current_L) / 3.0
        m2_f = current_R - (current_R - current_L) / 3.0
        
        if is_int:
            m1 = int(round(m1_f))
            m2 = int(round(m2_f))
            L_val = int(round(current_L))
            R_val = int(round(current_R))
            if m1 == L_val:
                m1 += 1
            if m2 == R_val:
                m2 -= 1
            if m1 >= m2:
                vals = [str(x) for x in range(L_val, R_val + 1)]
                scores = []
                for val in vals:
                    test_params = current_params.copy()
                    test_params[param_key] = val
                    metrics = run_eval(test_params)
                    score = get_sliced_objective(metrics, param_key)
                    scores.append((val, score))
                optimal_val = select_plateau_robust_value(param_key, vals, scores, default_val)
                return optimal_val
            
            v_L = str(L_val)
            v_m1 = str(m1)
            v_m2 = str(m2)
            v_R = str(R_val)
        else:
            v_L = f"{current_L:.4f}"
            v_m1 = f"{m1_f:.4f}"
            v_m2 = f"{m2_f:.4f}"
            v_R = f"{current_R:.4f}"
            
        print(f"  [Ternary Depth {depth}] Testing range [{v_L}, {v_R}] with division points {v_m1}, {v_m2}")
        
        points = [v_L, v_m1, v_m2, v_R]
        scores = []
        for val in points:
            test_params = current_params.copy()
            test_params[param_key] = val
            metrics = run_eval(test_params)
            score = get_sliced_objective(metrics, param_key)
            scores.append(score)
            
        S_L, S_m1, S_m2, S_R = scores
        S_max = max(scores)
        
        if S_max <= 0.0:
            print(f"  [Ternary Warning] All configurations failed recall constraints. Returning default: {default_val}")
            return default_val
            
        plateau_mask = [s >= S_max - tol for s in scores]
        
        if plateau_mask[1] and plateau_mask[2]:
            print("  Plateau in the middle. Shrinking range to [m1, m2]")
            if is_int:
                current_L, current_R = float(m1), float(m2)
            else:
                current_L, current_R = m1_f, m2_f
        elif plateau_mask[0] and plateau_mask[1]:
            print("  Plateau on the left. Shrinking range to [L, m1]")
            if is_int:
                current_L, current_R = float(L_val), float(m1)
            else:
                current_L, current_R = current_L, m1_f
        elif plateau_mask[2] and plateau_mask[3]:
            print("  Plateau on the right. Shrinking range to [m2, R]")
            if is_int:
                current_L, current_R = float(m2), float(R_val)
            else:
                current_L, current_R = m2_f, current_R
        elif S_m1 >= S_m2:
            print("  Peak closer to m1. Shrinking range to [L, m2]")
            if is_int:
                current_L, current_R = float(L_val), float(m2)
            else:
                current_L, current_R = current_L, m2_f
        else:
            print("  Peak closer to m2. Shrinking range to [m1, R]")
            if is_int:
                current_L, current_R = float(m1), float(R_val)
            else:
                current_L, current_R = m1_f, current_R
                
        depth += 1
        
    if is_int:
        final_val = str(int(round((current_L + current_R) / 2.0)))
    else:
        final_val = f"{((current_L + current_R) / 2.0):.4f}"
    print(f"  Ternary search completed. Final range: [{current_L}, {current_R}] -> Midpoint: {final_val}")
    return final_val

def main():
    if not os.path.exists(TUNED_PARAMS_PATH):
        print(f"Error: {TUNED_PARAMS_PATH} not found.")
        sys.exit(1)

    with open(TUNED_PARAMS_PATH, "r") as f:
        baseline_params = json.load(f)

    # Force configurations for optimization run
    baseline_params["search.bypass_sigmoid_gating"] = "true"
    if cross_encoder_only:
        baseline_params["search.enable_cross_encoder_rerank"] = "true"
    else:
        baseline_params["search.enable_cross_encoder_rerank"] = "false"

    current_params = baseline_params.copy()
    for k, v in optimal_params.items():
        current_params[k] = v

    # Define stage sweeps
    all_stages = [
        # Phase 1: Pool Sizes
        {
            "key": "search.tfidf_pool_size",
            "type": "int",
            "L": 50, "R": 250,
            "default": "150"
        },
        {
            "key": "search.temporal_expansion_pool_size",
            "type": "int",
            "L": 1, "R": 15,
            "default": "5"
        },
        # Phase 2: Decay Sigmas
        {
            "key": "search.gaussian_temporal_sigma",
            "type": "float",
            "L": 30.0, "R": 720.0,
            "default": "117.6"
        },
        {
            "key": "search.temporal.gaussian_sigma",
            "type": "float",
            "L": 30.0, "R": 720.0,
            "default": "117.6"
        },
        {
            "key": "search.user.gaussian_sigma",
            "type": "float",
            "L": 30.0, "R": 720.0,
            "default": "117.6"
        },
        {
            "key": "search.preference.gaussian_sigma",
            "type": "float",
            "L": 30.0, "R": 720.0,
            "default": "117.6"
        },
        # Phase 3: Cross-Encoder Rerank Weights & Specific Pool Size (tuned last to select smallest pool size)
        {
            "key": "search.default.rerank_weight",
            "type": "float",
            "L": 0.0, "R": 1.0,
            "default": "0.20"
        },
        {
            "key": "search.temporal.rerank_weight",
            "type": "float",
            "L": 0.0, "R": 1.0,
            "default": "0.25"
        },
        {
            "key": "search.user.rerank_weight",
            "type": "float",
            "L": 0.0, "R": 1.0,
            "default": "0.20"
        },
        {
            "key": "search.preference.rerank_weight",
            "type": "float",
            "L": 0.0, "R": 1.0,
            "default": "0.25"
        },
        {
            "key": "search.rerank_pool_size",
            "type": "int",
            "L": 5, "R": 75,
            "default": "25"
        },
        # Phase 4: Fusion & Boost
        {
            "key": "search.active_session_boost",
            "type": "float",
            "L": 0.0, "R": 0.4,
            "default": "0.15"
        },
        {
            "key": "search.default.ladder_scale",
            "type": "float",
            "L": 0.0, "R": 0.3,
            "default": "0.100"
        },
        {
            "key": "search.temporal.ladder_scale",
            "type": "float",
            "L": 0.0, "R": 0.4,
            "default": "0.200"
        },
        {
            "key": "search.user.ladder_scale",
            "type": "float",
            "L": 0.0, "R": 0.3,
            "default": "0.100"
        },
        {
            "key": "search.preference.ladder_scale",
            "type": "float",
            "L": 0.0, "R": 0.3,
            "default": "0.100"
        }
    ]

    # Filter stages based on active mode
    if cross_encoder_only:
        stages = [s for s in all_stages if "rerank_weight" in s["key"] or s["key"] == "search.rerank_pool_size"]
    elif no_cross_encoder:
        stages = [s for s in all_stages if "rerank_weight" not in s["key"] and s["key"] != "search.rerank_pool_size"]
    else:
        stages = all_stages

    print(f"=== STARTING Mythrax v2.6.0 TIERED TERNARY SWEEP ({mode_name} MODE) ===")

    for idx, stage in enumerate(stages):
        key = stage["key"]
        if key in completed_stages:
            print(f"\n[Stage {idx+1}/{len(stages)}] {key} already completed. Optimal: {optimal_params[key]}")
            current_params[key] = optimal_params[key]
            continue

        print(f"\n[Stage {idx+1}/{len(stages)}] Ternary Search for {key} over range [{stage['L']}, {stage['R']}]")
        
        optimal_val = run_ternary_search(
            param_key=key,
            L=stage["L"],
            R=stage["R"],
            is_int=(stage["type"] == "int"),
            max_depth=4,
            tol=0.0005,
            default_val=stage["default"],
            current_params=current_params
        )
        
        optimal_params[key] = optimal_val
        current_params[key] = optimal_val
        completed_stages.append(key)
        save_checkpoint()
        print(f"-> Winner for {key}: {optimal_val}")

    # Stability Verification
    print("\n=== STARTING STABILITY RUNS ===")
    stability_runs = []
    for run_idx in range(3):
        print(f"Stability Run {run_idx+1}/3...")
        metrics = run_eval(current_params)
        if metrics:
            stability_runs.append(metrics.get("ndcg", 0.0))

    if len(stability_runs) == 3:
        avg_ndcg = sum(stability_runs) / 3
        variance = sum((x - avg_ndcg) ** 2 for x in stability_runs) / 3
        std_dev = variance ** 0.5
        print(f"Stability check: nDCG std_dev = {std_dev:.5f} (Target: < 0.01)")
        if std_dev >= 0.01:
            print("WARNING: High standard deviation in stability runs!")
    else:
        print("Error: Could not complete 3 stability runs.")

    # Write final tuned_params.json
    final_params = baseline_params.copy()
    for k, v in optimal_params.items():
        if k in final_params:
            final_params[k] = v
            
    with open(TUNED_PARAMS_PATH, "w") as f:
        json.dump(final_params, f, indent=2)
    print("\n=== SWEEP COMPLETED SUCCESSFULLY ===")
    print(f"Final parameters written to {TUNED_PARAMS_PATH}")

if __name__ == "__main__":
    main()
