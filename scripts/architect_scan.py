#!/usr/bin/env python3
import subprocess
import json
import re
import os
import sys
from collections import defaultdict
import shutil

def run_command(cmd, cwd=None, env=None, check=False):
    return subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, check=check)

def parse_clippy_output(output, core_dir):
    debt = defaultdict(int)
    for line in output.split('\n'):
        if not line.strip():
            continue
        try:
            msg = json.loads(line)
            if msg.get("reason") == "compiler-message":
                code = msg.get("message", {}).get("code")
                if code:
                    code_name = code.get("code", "")
                    if code_name in ["dead_code", "unused_imports", "unreachable_code", "clippy::cognitive_complexity"]:
                        spans = msg.get("message", {}).get("spans", [])
                        if spans:
                            file_name = spans[0].get("file_name", "unknown")
                            if not os.path.isabs(file_name):
                                file_name = os.path.normpath(os.path.join(core_dir, file_name))
                            debt[file_name] += 1
        except json.JSONDecodeError:
            pass
    return debt

def scan_file_content(filepath, todo_content, all_types):
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
    except:
        return {}

    metrics = defaultdict(int)

    # 1. Dead code suppression
    suppressions = re.findall(r'#\[allow\((?:dead_code|unused_imports|unreachable_code)\)\]', content)
    metrics['suppressions'] += len(suppressions)

    # 2. Orphaned TODOs
    todos = re.findall(r'(?i)\b(?:TODO|FIXME|HACK|TEMP)\b[^\n]*', content)
    todo_words = set(re.findall(r'\w+', todo_content.lower()))
    for t in todos:
        words = set(re.findall(r'\w+', t.lower()))
        if not (words & todo_words):
             metrics['orphaned_todos'] += 1

    # 3. Error handling inconsistency
    has_unwrap = 'unwrap()' in content
    has_expect = 'expect(' in content
    has_question = '?' in content
    has_match = 'match ' in content
    if (has_unwrap or has_expect) and (has_question or has_match):
        metrics['inconsistent_error_handling'] += 1

    # 4. Duplicated types
    structs = re.findall(r'(?:struct|enum)\s+([A-Z]\w*)', content)
    for s in structs:
        if s in all_types and all_types[s] > 1:
            metrics['duplicated_types'] += 1

    # 5. Magic numbers and strings
    magic_numbers = re.findall(r'(?:==|!=|\+=|-=|\*=|/=|>|<|>=|<=)\s*(\d+)', content)
    for n in magic_numbers:
        if n not in ['0', '1']:
            metrics['magic_numbers'] += 1

    magic_strings = re.findall(r'(?:==|!=)\s*("[^"]+")', content)
    metrics['magic_strings'] += len(magic_strings)

    return dict(metrics)

def get_all_types(repo_root):
    all_types = defaultdict(int)
    for root, dirs, files in os.walk(repo_root):
        if 'target' in dirs: dirs.remove('target')
        if '.git' in dirs: dirs.remove('.git')
        if '.venv' in dirs: dirs.remove('.venv')
        for file in files:
            if file.endswith('.rs'):
                path = os.path.join(root, file)
                try:
                    with open(path, 'r', encoding='utf-8') as f:
                        content = f.read()
                        structs = re.findall(r'(?:struct|enum)\s+([A-Z]\w*)', content)
                        for s in structs:
                            all_types[s] += 1
                except:
                    pass
    return all_types

def scan_commit(repo_root, commit_hash):
    core_dir = os.path.join(repo_root, "mythrax-core")

    todo_path = os.path.join(repo_root, "TODO.md")
    todo_content = ""
    if os.path.exists(todo_path):
        with open(todo_path, "r", encoding="utf-8") as f:
            todo_content = f.read()

    all_types = get_all_types(repo_root)

    clippy_toml_path = os.path.join(core_dir, "clippy.toml")
    with open(clippy_toml_path, "w") as f:
        f.write("cognitive-complexity-threshold = 15\n")

    cmd = [
        "cargo", "clippy", "--no-default-features", "--message-format=json", "--",
        "-A", "warnings",
        "-W", "dead_code",
        "-W", "unused_imports",
        "-W", "unreachable_code",
        "-W", "clippy::cognitive_complexity"
    ]
    clippy_res = run_command(cmd, cwd=core_dir)

    if os.path.exists(clippy_toml_path):
        os.remove(clippy_toml_path)

    clippy_debt = parse_clippy_output(clippy_res.stdout, core_dir)

    file_metrics = defaultdict(lambda: defaultdict(int))
    for f, count in clippy_debt.items():
        rel_path = os.path.relpath(f, repo_root) if os.path.isabs(f) else f
        file_metrics[rel_path]['clippy'] += count

    for root, dirs, files in os.walk(repo_root):
        if 'target' in dirs: dirs.remove('target')
        if '.git' in dirs: dirs.remove('.git')
        if '.venv' in dirs: dirs.remove('.venv')
        for file in files:
            if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                path = os.path.join(root, file)
                rel_path = os.path.relpath(path, repo_root)
                metrics = scan_file_content(path, todo_content, all_types)
                for k, v in metrics.items():
                    if v > 0:
                        file_metrics[rel_path][k] += v

    total_debt = 0
    for f, metrics in file_metrics.items():
        total_debt += sum(metrics.values())

    return total_debt, file_metrics

def main():
    repo_root = os.getcwd()

    log_cmd = ["git", "log", "-n", "6", "--format=%H"]
    res = run_command(log_cmd, cwd=repo_root)
    commits = res.stdout.strip().split('\n')

    if not commits or not commits[0]:
        print("No commits found.")
        sys.exit(1)

    commits.reverse()

    orig_branch = run_command(["git", "rev-parse", "--abbrev-ref", "HEAD"], cwd=repo_root).stdout.strip()
    if orig_branch == "HEAD":
        orig_branch = os.environ.get("GITHUB_HEAD_REF") or os.environ.get("GITHUB_REF_NAME") or run_command(["git", "rev-parse", "HEAD"], cwd=repo_root).stdout.strip()

    core_dir = os.path.join(repo_root, "mythrax-core")
    clippy_toml_path = os.path.join(core_dir, "clippy.toml")
    if os.path.exists(clippy_toml_path):
        os.remove(clippy_toml_path)

    history = []

    for commit in commits:
        print(f"Scanning commit {commit}...")
        if os.path.exists(clippy_toml_path):
            os.remove(clippy_toml_path)

        run_command(["git", "checkout", "-f", commit], cwd=repo_root)
        total_debt, file_metrics = scan_commit(repo_root, commit)
        history.append({
            "commit": commit,
            "total_debt": total_debt,
            "file_metrics": file_metrics
        })

    if os.path.exists(clippy_toml_path):
        os.remove(clippy_toml_path)

    run_command(["git", "checkout", "-f", orig_branch], cwd=repo_root)

    current = history[-1]
    previous = history[-2] if len(history) > 1 else None

    report = "# Architecture Sanitation Scorecard\n\n"
    report += "## Trajectory (Last 6 Commits)\n"
    report += "| Commit | Total Debt |\n"
    report += "|--------|------------|\n"
    for h in history:
        report += f"| `{h['commit'][:7]}` | {h['total_debt']} |\n"

    if previous:
        report += "\n**Overall Trend:** "
        if current['total_debt'] < previous['total_debt']:
            report += "🟢 Improving\n"
        elif current['total_debt'] > previous['total_debt']:
            report += "🔴 Degrading\n"
        else:
            report += "🟡 Neutral\n"

        report += "\n### Files with Increasing Debt Density\n"
        found_increasing = False
        for file_name, cur_metrics in current['file_metrics'].items():
            cur_debt = sum(cur_metrics.values())
            prev_debt = sum(previous['file_metrics'].get(file_name, {}).values())
            if cur_debt > prev_debt:
                report += f"- `{file_name}`: {prev_debt} -> {cur_debt} (Debt details: {dict(cur_metrics)})\n"
                found_increasing = True
        if not found_increasing:
            report += "*No files showed increasing debt density in this commit.*\n"

    print(report)

    with open("scorecard.md", "w") as f:
        f.write(report)

    gh_token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    env = os.environ.copy()
    if gh_token:
        env["GH_TOKEN"] = gh_token

    if shutil.which("gh"):
        branch_name = os.environ.get("GITHUB_HEAD_REF") or os.environ.get("GITHUB_REF_NAME") or orig_branch

        print(f"Trying to find PR for branch: {branch_name}")
        pr_list_cmd = ["gh", "pr", "list", "--head", branch_name, "--json", "number"]
        res = run_command(pr_list_cmd, cwd=repo_root, env=env)
        if res.returncode == 0:
            try:
                prs = json.loads(res.stdout)
                if prs:
                    pr_number = str(prs[0]["number"])
                    print(f"Commenting on PR #{pr_number}")
                    run_command(["gh", "pr", "comment", pr_number, "-F", "scorecard.md"], cwd=repo_root, env=env)
                else:
                    print("No open PR found for this branch.")
            except:
                print("Failed to parse gh pr list output")
        else:
            print(f"gh pr list failed: {res.stderr}")
    else:
        print("gh command not found, skipping PR comment. File 'scorecard.md' has been generated.")

if __name__ == "__main__":
    main()
