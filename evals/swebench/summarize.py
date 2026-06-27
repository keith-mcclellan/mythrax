#!/usr/bin/env python3
"""Summarize / A-B compare runs of the OFFICIAL SWE-bench harness.

Parses the official harness report JSON (the `<model>.<run_id>.json` artifact emitted
by `python -m swebench.harness.run_evaluation`), which contains `resolved_ids`,
`unresolved_ids`, and `error_ids` lists — NOT a pre-baked per-line `status` field.
We do not invent statuses; everything is derived from the official lists.
"""
import argparse
import json
import sys


def parse_report(path):
    """Read an official harness report JSON -> {instance_id: status}.

    status in {"resolved", "unresolved", "error"} derived from the official id lists.
    """
    with open(path, "r") as f:
        report = json.load(f)

    resolved = set(report.get("resolved_ids", []))
    errored = set(report.get("error_ids", []))
    # The harness reports unresolved explicitly; fall back to any completed-but-not-resolved.
    unresolved = set(report.get("unresolved_ids", []))
    # empty-patch instances (no prediction) count as unresolved for rate purposes.
    unresolved |= set(report.get("empty_patch_ids", []))

    results = {}
    for iid in resolved:
        results[iid] = "resolved"
    for iid in errored:
        results.setdefault(iid, "error")
    for iid in unresolved:
        results.setdefault(iid, "unresolved")
    return results, report


def tally(results):
    total = len(results)
    resolved = sum(1 for s in results.values() if s == "resolved")
    unresolved = sum(1 for s in results.values() if s == "unresolved")
    error = sum(1 for s in results.values() if s == "error")
    rate = (resolved / total) * 100 if total > 0 else 0.0
    return total, resolved, unresolved, error, rate


def pct(n, total):
    return (n / total) * 100 if total > 0 else 0.0


def main():
    parser = argparse.ArgumentParser(
        description="Summarize / compare official SWE-bench harness report JSONs."
    )
    parser.add_argument("run_file", help="Path to the current run's official report JSON.")
    parser.add_argument("--compare", help="Path to a baseline report JSON for A/B comparison.")
    args = parser.parse_args()

    current_results, _ = parse_report(args.run_file)
    if not current_results:
        print("Error: no instances found in current report.", file=sys.stderr)
        sys.exit(1)

    total_c, res_c, unres_c, err_c, rate_c = tally(current_results)

    if not args.compare:
        print("# SWE-bench Verified Evaluation Summary (official harness)\n")
        print(f"- **Total Instances**: {total_c}")
        print(f"- **Resolved**: {res_c} ({rate_c:.2f}%)")
        print(f"- **Unresolved**: {unres_c} ({pct(unres_c, total_c):.2f}%)")
        print(f"- **Error**: {err_c} ({pct(err_c, total_c):.2f}%)\n")
        print("Resolved: {} ({:.2f}%)".format(res_c, rate_c))
        return

    baseline_results, _ = parse_report(args.compare)
    if not baseline_results:
        print("Error: no instances found in baseline report.", file=sys.stderr)
        sys.exit(1)

    total_b, res_b, unres_b, err_b, rate_b = tally(baseline_results)

    d_res = res_c - res_b
    d_unres = unres_c - unres_b
    d_err = err_c - err_b
    d_rate = rate_c - rate_b

    sgn = lambda x: "+" if x >= 0 else ""

    print("# SWE-bench Verified A/B Comparison (official harness)\n")
    print("| Metric | Baseline | Mythrax | Delta |")
    print("| --- | --- | --- | --- |")
    print(f"| Total Instances | {total_b} | {total_c} | {total_c - total_b} |")
    print(
        f"| Resolved | {res_b} ({rate_b:.2f}%) | {res_c} ({rate_c:.2f}%) | "
        f"{sgn(d_res)}{d_res} ({sgn(d_rate)}{d_rate:.2f} percentage points) |"
    )
    ru_b, ru_c = pct(unres_b, total_b), pct(unres_c, total_c)
    print(
        f"| Unresolved | {unres_b} ({ru_b:.2f}%) | {unres_c} ({ru_c:.2f}%) | "
        f"{sgn(d_unres)}{d_unres} ({sgn(ru_c - ru_b)}{ru_c - ru_b:.2f} percentage points) |"
    )
    re_b, re_c = pct(err_b, total_b), pct(err_c, total_c)
    print(
        f"| Error | {err_b} ({re_b:.2f}%) | {err_c} ({re_c:.2f}%) | "
        f"{sgn(d_err)}{d_err} ({sgn(re_c - re_b)}{re_c - re_b:.2f} percentage points) |\n"
    )

    changes = []
    all_keys = set(current_results) | set(baseline_results)
    for k in sorted(all_keys):
        b = baseline_results.get(k, "missing")
        c = current_results.get(k, "missing")
        if b == c:
            continue
        change = "Neutral"
        if b in ("unresolved", "error", "missing") and c == "resolved":
            change = "Improved (+)"
        elif b == "resolved" and c in ("unresolved", "error", "missing"):
            change = "Regressed (-)"
        elif b == "error" and c == "unresolved":
            change = "Improved (+)"
        elif b == "unresolved" and c == "error":
            change = "Regressed (-)"
        changes.append((k, b, c, change))

    if changes:
        print("## Per-Instance Status Changes\n")
        print("| Instance ID | Baseline | Mythrax | Change |")
        print("| --- | --- | --- | --- |")
        for k, b, c, change in changes:
            print(f"| {k} | {b} | {c} | {change} |")
    else:
        print("No status changes found between runs.")


if __name__ == "__main__":
    main()
