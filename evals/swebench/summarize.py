#!/usr/bin/env python3
import json
import sys
import argparse

def parse_run_file(path):
    results = {}
    with open(path, 'r') as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                data = json.loads(line)
                instance_id = data.get("instance_id")
                status = data.get("status", "unresolved") # resolved, unresolved, error
                if instance_id:
                    results[instance_id] = status
            except Exception as e:
                print(f"Warning: failed to parse line: {line}. Error: {e}", file=sys.stderr)
    return results

def main():
    parser = argparse.ArgumentParser(description="Summarize and compare SWE-bench evaluation runs.")
    parser.add_argument("run_file", help="Path to the current run JSONL file.")
    parser.add_argument("--compare", help="Path to the baseline run JSONL file for comparison.")
    args = parser.parse_args()

    current_results = parse_run_file(args.run_file)
    if not current_results:
        print("Error: No valid results found in current run file.", file=sys.stderr)
        sys.exit(1)

    total_current = len(current_results)
    resolved_current = sum(1 for s in current_results.values() if s == "resolved")
    unresolved_current = sum(1 for s in current_results.values() if s == "unresolved")
    error_current = sum(1 for s in current_results.values() if s == "error")

    rate_current = (resolved_current / total_current) * 100 if total_current > 0 else 0.0

    if not args.compare:
        # Single run summary
        print("# SWE-bench Evaluation Summary\n")
        print(f"- **Total Instances**: {total_current}")
        print(f"- **Resolved**: {resolved_current} ({rate_current:.2f}%)")
        print(f"- **Unresolved**: {unresolved_current} ({(unresolved_current/total_current)*100:.2f}%)")
        print(f"- **Error**: {error_current} ({(error_current/total_current)*100:.2f}%)\n")
        print("Resolved: {} ({:.2f}%)".format(resolved_current, rate_current))
    else:
        # A/B comparison
        baseline_results = parse_run_file(args.compare)
        if not baseline_results:
            print("Error: No valid results found in baseline run file.", file=sys.stderr)
            sys.exit(1)

        total_base = len(baseline_results)
        resolved_base = sum(1 for s in baseline_results.values() if s == "resolved")
        unresolved_base = sum(1 for s in baseline_results.values() if s == "unresolved")
        error_base = sum(1 for s in baseline_results.values() if s == "error")

        rate_base = (resolved_base / total_base) * 100 if total_base > 0 else 0.0

        diff_resolved = resolved_current - resolved_base
        diff_unresolved = unresolved_current - unresolved_base
        diff_error = error_current - error_base
        diff_rate = rate_current - rate_base

        print("# SWE-bench A/B Evaluation Comparison\n")
        print("| Metric | Baseline (other) | Mythrax (current) | Delta |")
        print("| --- | --- | --- | --- |")
        print(f"| Total Instances | {total_base} | {total_current} | {total_current - total_base} |")
        
        sign_resolved = "+" if diff_resolved >= 0 else ""
        sign_rate = "+" if diff_rate >= 0 else ""
        print(f"| Resolved | {resolved_base} ({rate_base:.2f}%) | {resolved_current} ({rate_current:.2f}%) | {sign_resolved}{diff_resolved} ({sign_rate}{diff_rate:.2f} percentage points) |")
        
        sign_unresolved = "+" if diff_unresolved >= 0 else ""
        rate_unresolved_base = (unresolved_base/total_base)*100 if total_base > 0 else 0.0
        rate_unresolved_curr = (unresolved_current/total_current)*100 if total_current > 0 else 0.0
        print(f"| Unresolved | {unresolved_base} ({rate_unresolved_base:.2f}%) | {unresolved_current} ({rate_unresolved_curr:.2f}%) | {sign_unresolved}{diff_unresolved} ({sign_unresolved}{rate_unresolved_curr - rate_unresolved_base:.2f} percentage points) |")
        
        sign_error = "+" if diff_error >= 0 else ""
        rate_error_base = (error_base/total_base)*100 if total_base > 0 else 0.0
        rate_error_curr = (error_current/total_current)*100 if total_current > 0 else 0.0
        print(f"| Error | {error_base} ({rate_error_base:.2f}%) | {error_current} ({rate_error_curr:.2f}%) | {sign_error}{diff_error} ({sign_error}{rate_error_curr - rate_error_base:.2f} percentage points) |\n")

        # Per-instance changes
        changes = []
        all_keys = set(current_results.keys()).union(set(baseline_results.keys()))
        for k in sorted(all_keys):
            base_status = baseline_results.get(k, "missing")
            curr_status = current_results.get(k, "missing")
            if base_status != curr_status:
                change_type = "Neutral"
                if base_status in ["unresolved", "error", "missing"] and curr_status == "resolved":
                    change_type = "Improved (+)"
                elif base_status == "resolved" and curr_status in ["unresolved", "error", "missing"]:
                    change_type = "Regressed (-)"
                elif base_status == "error" and curr_status == "unresolved":
                    change_type = "Improved (+)"
                elif base_status == "unresolved" and curr_status == "error":
                    change_type = "Regressed (-)"
                
                changes.append((k, base_status, curr_status, change_type))

        if changes:
            print("## Per-Instance Status Changes\n")
            print("| Instance ID | Baseline | Mythrax | Change |")
            print("| --- | --- | --- | --- |")
            for k, base, curr, change in changes:
                print(f"| {k} | {base} | {curr} | {change} |")
        else:
            print("No status changes found between runs.")

if __name__ == "__main__":
    main()
