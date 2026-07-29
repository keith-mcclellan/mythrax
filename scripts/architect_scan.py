import os
import subprocess
import json
import re
import sys
import shlex

def get_commits(n: int) -> list:
    try:
        res = subprocess.run(["git", "rev-list", "-n", str(n), "HEAD"], capture_output=True, text=True, check=True)
        return res.stdout.strip().split("\n")
    except subprocess.CalledProcessError:
        return []

def run_clippy() -> dict:
    metrics = {"cognitive_complexity": 0, "dead_code": 0, "unused_imports": 0, "unreachable_code": 0}
    if not os.path.exists("mythrax-core"):
        return metrics

    cwd = os.path.abspath("mythrax-core")
    clippy_toml_path = os.path.join(cwd, "clippy.toml")
    try:
        with open(clippy_toml_path, "w") as f:
            f.write("cognitive-complexity-threshold = 15\n")

        cmd = ["cargo", "clippy", "--message-format=json", "--lib", "--", "-A", "warnings", "-W", "clippy::cognitive_complexity", "-W", "dead_code", "-W", "unused_imports", "-W", "unreachable_code"]
        res = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)

        for line in res.stdout.split("\n"):
            if not line.strip(): continue
            try:
                msg = json.loads(line)
                if msg.get("reason") == "compiler-message" and msg.get("message", {}).get("level") in ["warning", "error"]:
                    code = msg.get("message", {}).get("code", {}).get("code", "")
                    if code == "clippy::cognitive_complexity":
                        metrics["cognitive_complexity"] += 1
                    elif code == "dead_code":
                        metrics["dead_code"] += 1
                    elif code == "unused_imports":
                        metrics["unused_imports"] += 1
                    elif code == "unreachable_code":
                        metrics["unreachable_code"] += 1
            except json.JSONDecodeError:
                pass
    finally:
        if os.path.exists(clippy_toml_path):
            os.remove(clippy_toml_path)
    return metrics

def get_tracked_todos() -> list:
    tracked = []
    if os.path.exists("TODO.md"):
        with open("TODO.md", "r") as f:
            for line in f:
                line = line.strip()
                if line.startswith("- [ ]") or line.startswith("- [x]"):
                    tracked.append(line[5:].strip().lower())
    return tracked

def scan_file(filepath: str, tracked_todos: list) -> dict:
    debt = {
        "suppressed_dead_code": 0,
        "orphaned_todos": 0,
        "unwraps_expects": 0,
        "magic_numbers": 0,
        "struct_enum_defs": [],
    }
    try:
        with open(filepath, "r", encoding="utf-8") as f:
            content = f.read()
            lines = content.split('\n')

            # #[allow(dead_code)]
            debt["suppressed_dead_code"] = len(re.findall(r'#\[allow\(dead_code\)\]', content))

            # Unwraps / Expects
            debt["unwraps_expects"] += len(re.findall(r'\.unwrap\(\)', content))
            debt["unwraps_expects"] += len(re.findall(r'\.expect\(', content))

            # TODO, FIXME, HACK, TEMP
            for line in lines:
                if any(x in line for x in ["TODO", "FIXME", "HACK", "TEMP"]):
                    lower_line = line.lower()
                    if not any(t in lower_line for t in tracked_todos if t):
                        debt["orphaned_todos"] += 1

            # Magic numbers (heuristics: literal numbers > 1 in Rust code not in const declarations)
            if filepath.endswith(".rs"):
                for line in lines:
                    line = line.strip()
                    if line.startswith("const ") or line.startswith("static ") or line.startswith("//"):
                        continue
                    # find numbers that are not 0 or 1
                    nums = re.findall(r'\b[2-9]\d*\b', line)
                    debt["magic_numbers"] += len(nums)

            # Struct/enum definitions for duplicate check
            if filepath.endswith(".rs"):
                structs = re.findall(r'struct\s+([A-Za-z0-9_]+)\s*\{', content)
                enums = re.findall(r'enum\s+([A-Za-z0-9_]+)\s*\{', content)
                debt["struct_enum_defs"].extend(structs)
                debt["struct_enum_defs"].extend(enums)

    except Exception:
        pass
    return debt

def get_debt_density(file_debt: dict) -> float:
    return sum([
        file_debt["suppressed_dead_code"],
        file_debt["orphaned_todos"],
        file_debt["unwraps_expects"],
        file_debt["magic_numbers"]
    ])

def main():
    commits = get_commits(6)
    if not commits:
        print("No commits found.")
        return

    results = []

    # Store initial head to restore later
    initial_head = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()

    try:
        # Reverse to go from oldest to newest
        for commit in reversed(commits):
            subprocess.run(["git", "checkout", "-f", commit], check=True, capture_output=True)

            clippy_metrics = run_clippy()
            tracked_todos = get_tracked_todos()

            file_debts = {}
            structs_all = []

            for root, dirs, files in os.walk("."):
                dirs[:] = [d for d in dirs if d not in [".git", "target", ".venv", ".cargo"]]
                for file in files:
                    if file.endswith(".rs") or file.endswith(".py") or file.endswith(".sh"):
                        filepath = os.path.join(root, file)
                        debt = scan_file(filepath, tracked_todos)
                        file_debts[filepath] = debt
                        structs_all.extend(debt["struct_enum_defs"])

            duplicate_structs = 0
            struct_counts = {}
            for s in structs_all:
                struct_counts[s] = struct_counts.get(s, 0) + 1
            for k, v in struct_counts.items():
                if v > 1:
                    duplicate_structs += 1

            total_suppressed = sum(d["suppressed_dead_code"] for d in file_debts.values())
            total_orphaned = sum(d["orphaned_todos"] for d in file_debts.values())
            total_unwraps = sum(d["unwraps_expects"] for d in file_debts.values())
            total_magic = sum(d["magic_numbers"] for d in file_debts.values())

            file_densities = {f: get_debt_density(d) for f, d in file_debts.items()}

            results.append({
                "commit": commit,
                "clippy": clippy_metrics,
                "suppressed_dead_code": total_suppressed,
                "orphaned_todos": total_orphaned,
                "unwraps_expects": total_unwraps,
                "magic_numbers": total_magic,
                "duplicate_structs": duplicate_structs,
                "file_densities": file_densities
            })
    finally:
        subprocess.run(["git", "checkout", "-f", initial_head], check=True, capture_output=True)

    # Generate Markdown scorecard
    scorecard = "## Chief Architect Sanitation Scorecard\n\n"
    scorecard += "| Commit | Complexity | Dead Code | Unused Imports | Unreachable | Suppressed Code | Orphaned TODOs | Unwraps/Expects | Magic Numbers | Duplicate Structs |\n"
    scorecard += "|---|---|---|---|---|---|---|---|---|---|\n"

    for r in results:
        commit_short = r["commit"][:7]
        c = r["clippy"]
        scorecard += f"| {commit_short} | {c['cognitive_complexity']} | {c['dead_code']} | {c['unused_imports']} | {c['unreachable_code']} | {r['suppressed_dead_code']} | {r['orphaned_todos']} | {r['unwraps_expects']} | {r['magic_numbers']} | {r['duplicate_structs']} |\n"

    scorecard += "\n### Files with Increasing Debt Density\n"

    if len(results) > 1:
        prev = results[-2]["file_densities"]
        curr = results[-1]["file_densities"]

        increasing_files = []
        for f, dens in curr.items():
            if f in prev and dens > prev[f]:
                increasing_files.append(f"- `{f}`: {prev[f]} -> {dens}")

        if increasing_files:
            scorecard += "\n".join(increasing_files)
        else:
            scorecard += "No files with increasing debt density.\n"
    else:
        scorecard += "Not enough history to determine trajectory.\n"

    print(scorecard)

    with open("scorecard.md", "w") as f:
        f.write(scorecard)

    # If triggered by a GitHub `push` event, use the `gh` CLI to post as PR comment
    if os.environ.get("GITHUB_EVENT_NAME") == "push":
        branch_name = os.environ.get("GITHUB_REF_NAME")
        if branch_name:
            try:
                # Find the PR associated with this branch
                pr_cmd = ["gh", "pr", "list", "--head", branch_name, "--json", "number", "--jq", ".[0].number"]
                pr_res = subprocess.run(pr_cmd, capture_output=True, text=True, check=True)
                pr_number = pr_res.stdout.strip()

                if pr_number and pr_number != "null":
                    # Create the comment using gh API or gh pr comment
                    comment_cmd = ["gh", "pr", "comment", pr_number, "-F", "scorecard.md"]
                    subprocess.run(comment_cmd, check=True)
                    print(f"Posted scorecard to PR #{pr_number}")
                else:
                    print(f"No open PR found for branch {branch_name}")
            except subprocess.CalledProcessError as e:
                print(f"Failed to post PR comment: {e}")

if __name__ == "__main__":
    main()
