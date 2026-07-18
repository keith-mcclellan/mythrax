#!/usr/bin/env python3
import subprocess
import json
import re
import os
import sys
from collections import defaultdict

def run_cmd(cmd, cwd=None):
    res = subprocess.run(cmd, shell=True, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    return res.stdout.strip()

def get_tracked_todos():
    try:
        with open('TODO.md', 'r', encoding='utf-8') as f:
            return f.read().lower()
    except FileNotFoundError:
        return ""

def scan_rust_files(repo_path, tracked_todos):
    metrics = {
        "allow_dead_code": 0,
        "orphaned_debt": 0,
        "inconsistent_error": 0,
        "magic_numbers": 0,
        "duplicated_structs": 0
    }
    struct_counts = defaultdict(int)
    file_debt = defaultdict(int)

    src_dir = os.path.join(repo_path, 'mythrax-core', 'src')
    if not os.path.exists(src_dir):
        return metrics, file_debt

    for root, dirs, files in os.walk(src_dir):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r', encoding='utf-8') as f:
                        content = f.read()
                except:
                    continue

                rel_path = os.path.relpath(filepath, repo_path)

                # allow(dead_code)
                adc = len(re.findall(r'#\[allow\(\s*dead_code\s*\)\]', content))
                metrics["allow_dead_code"] += adc
                file_debt[rel_path] += adc

                # orphaned debt
                comments = re.findall(r'//\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE)
                orphaned = 0
                for tag, desc in comments:
                    desc_clean = desc.strip().lower()
                    words = [w for w in desc_clean.split() if len(w) > 3]
                    found = False
                    if tracked_todos:
                        for word in words:
                            if word in tracked_todos:
                                found = True
                                break
                    if not found and len(words) > 0:
                        orphaned += 1
                    elif not tracked_todos:
                        orphaned += 1
                metrics["orphaned_debt"] += orphaned
                file_debt[rel_path] += orphaned

                # Inconsistent error handling
                has_unwrap = 'unwrap(' in content
                has_expect = 'expect(' in content
                has_question = '?' in content
                has_match = 'match ' in content
                if sum([has_unwrap, has_expect, has_question, has_match]) >= 3:
                    metrics["inconsistent_error"] += 1
                    file_debt[rel_path] += 1

                # Magic numbers and string literals
                mn = len(re.findall(r'(?<![a-zA-Z0-9_])(?:let|mut)\s+[a-zA-Z0-9_]+\s*(?::\s*[a-zA-Z0-9_<>]+)?\s*=\s*\d{2,}', content))
                mn += len(re.findall(r'(?<![a-zA-Z0-9_])(?:let|mut)\s+[a-zA-Z0-9_]+\s*(?::\s*[a-zA-Z0-9_<>]+)?\s*=\s*"[^"]{5,}"', content))
                metrics["magic_numbers"] += mn
                file_debt[rel_path] += mn

                # Structs
                structs = re.findall(r'(?:pub\s+)?(?:struct|enum)\s+([A-Z][a-zA-Z0-9_]*)', content)
                for s in structs:
                    struct_counts[s] += 1

    for s, count in struct_counts.items():
        if count > 1:
            metrics["duplicated_structs"] += (count - 1)

    return metrics, file_debt

def parse_clippy_json(filepath):
    metrics = {
        "dead_code": 0,
        "unused_imports": 0,
        "unreachable": 0,
        "cyclomatic": 0
    }
    if not os.path.exists(filepath):
        return metrics

    with open(filepath, 'r', encoding='utf-8') as f:
        for line in f:
            if not line.strip(): continue
            try:
                msg = json.loads(line)
                if msg.get('reason') == 'compiler-message':
                    code = msg.get('message', {}).get('code', {})
                    if code:
                        code_id = code.get('code', '')
                        if code_id == 'dead_code':
                            metrics["dead_code"] += 1
                        elif code_id == 'unused_imports':
                            metrics["unused_imports"] += 1
                        elif code_id in ('unreachable_code', 'unreachable_patterns'):
                            metrics["unreachable"] += 1
                        elif code_id in ('clippy::cognitive_complexity', 'clippy::cyclomatic_complexity'):
                            metrics["cyclomatic"] += 1
            except:
                pass
    return metrics

def main():
    repo_path = os.getcwd()

    is_dirty = bool(run_cmd("git status --porcelain"))

    if os.environ.get("GITHUB_ACTIONS"):
        # Explicitly fetching HEAD so the PR merge commit history is valid
        commits = run_cmd("git rev-list HEAD -n 6").split('\n')
        if not commits or commits == ['']:
            commits = ['HEAD']
        else:
            commits.reverse()
    else:
        # Local run - just analyze current if dirty to avoid losing work
        if is_dirty:
            commits = ['current']
        else:
            commits = run_cmd("git rev-list HEAD -n 6").split('\n')
            if not commits or commits == ['']:
                commits = ['current']
            else:
                commits.reverse()

    history = []
    file_debt_history = []

    current_branch = run_cmd("git branch --show-current")
    if not current_branch:
        current_branch = run_cmd("git rev-parse HEAD")

    try:
        for commit in commits:
            if commit != 'current':
                run_cmd("git checkout .")
                run_cmd(f"git checkout {commit} --force")

            if not os.path.exists('mythrax-core/clippy.toml'):
                if not os.path.exists('mythrax-core'):
                    os.makedirs('mythrax-core', exist_ok=True)
                with open('mythrax-core/clippy.toml', 'w') as f:
                    f.write("cognitive-complexity-threshold = 15\n")

            run_cmd("cargo clippy --message-format=json > clippy_out.json 2>/dev/null", cwd=os.path.join(repo_path, 'mythrax-core'))
            clippy_metrics = parse_clippy_json(os.path.join(repo_path, 'mythrax-core', 'clippy_out.json'))

            tracked_todos = get_tracked_todos()
            custom_metrics, file_debt = scan_rust_files(repo_path, tracked_todos)

            combined = {**clippy_metrics, **custom_metrics}
            history.append({
                "commit": commit[:7] if commit != 'current' else 'current',
                "metrics": combined
            })
            file_debt_history.append(file_debt)

    finally:
        if any(c != 'current' for c in commits):
            run_cmd("git checkout .")
            run_cmd(f"git checkout {current_branch} --force")

    if not history:
        print("No history found.")
        sys.exit(1)

    latest_metrics = history[-1]["metrics"]

    with open("scorecard.md", "w") as f:
        f.write("# Sanitation Scorecard\n\n")
        f.write("## Trajectory (Last 5 Commits -> Current)\n\n")
        f.write("| Metric | ")
        for h in history:
            f.write(f"{h['commit']} | ")
        f.write("\n")

        f.write("|---|")
        for _ in history:
            f.write("---|")
        f.write("\n")

        keys = list(latest_metrics.keys())
        for k in keys:
            f.write(f"| {k} | ")
            for h in history:
                f.write(f"{h['metrics'][k]} | ")
            f.write("\n")

        f.write("\n## Debt Density Flagging\n\n")
        if len(history) > 1:
            oldest_debt = file_debt_history[0]
            newest_debt = file_debt_history[-1]
            flagged = []
            for file, debt in newest_debt.items():
                old_debt = oldest_debt.get(file, 0)
                if debt > old_debt:
                    flagged.append(f"- **{file}**: {old_debt} -> {debt} issues")
            if flagged:
                f.write("### Warning: The following files have increasing debt density:\n")
                f.write("\n".join(flagged))
                f.write("\n")
            else:
                f.write("No files have increasing debt density. Good job!\n")
        else:
            f.write("Not enough history to determine debt density trajectory.\n")

if __name__ == "__main__":
    main()
