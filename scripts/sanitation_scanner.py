#!/usr/bin/env python3
import os
import subprocess
import json
import re
import sys
import shutil
from collections import defaultdict

def run_cmd(cmd, cwd=None):
    res = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=cwd)
    return res.stdout, res.returncode

def get_tracked_todos(repo_root):
    todo_file = os.path.join(repo_root, "TODO.md")
    if not os.path.exists(todo_file):
        return []
    with open(todo_file, "r") as f:
        content = f.read()

    todos = set()
    for line in content.split('\n'):
        line = line.strip().lower()
        if line:
            todos.add(line)
    return todos

def analyze_commit(repo_root, commit_hash, clippy_toml_path):
    print(f"Checking out {commit_hash}...", file=sys.stderr)
    run_cmd(f"git checkout --quiet {commit_hash}", cwd=repo_root)

    mythrax_core = os.path.join(repo_root, "mythrax-core")

    # Temporarily copy clippy.toml back in if it was deleted by checkout
    temp_clippy = os.path.join(mythrax_core, "clippy.toml")
    if not os.path.exists(temp_clippy) and os.path.exists(clippy_toml_path):
        shutil.copy2(clippy_toml_path, temp_clippy)
        copied_clippy = True
    else:
        copied_clippy = False

    # 1. Run clippy
    print(f"Running cargo clippy on {commit_hash}...", file=sys.stderr)
    stdout, rc = run_cmd("cargo clippy --message-format=json", cwd=mythrax_core)

    debt_by_file = defaultdict(int)

    for line in stdout.split('\n'):
        if not line.strip():
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue

        if msg.get('reason') == 'compiler-message':
            code_info = msg['message'].get('code')
            if code_info:
                rule = code_info.get('code', '')
                if rule in [
                    'dead_code', 'unused_imports', 'unreachable_code',
                    'clippy::cognitive_complexity', 'clippy::too_many_arguments',
                    'clippy::type_complexity'
                ]:
                    spans = msg['message'].get('spans', [])
                    if spans:
                        file_name = spans[0].get('file_name', '')
                        debt_by_file[file_name] += 1

    # 2. Static Analysis
    tracked_todos = get_tracked_todos(repo_root)

    # Store struct/enum names to detect duplicates
    struct_enum_defs = defaultdict(list)

    for root, _, files in os.walk(mythrax_core):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                rel_path = os.path.relpath(filepath, mythrax_core)

                with open(filepath, 'r') as f:
                    try:
                        lines = f.readlines()
                    except UnicodeDecodeError:
                        continue

                content = "".join(lines)

                # #[allow(dead_code)]
                debt_by_file[rel_path] += len(re.findall(r'#\[allow\(dead_code\)\]', content))

                # TODO, FIXME, HACK, TEMP - cross referenced
                for line in lines:
                    line_upper = line.upper()
                    if any(tag in line_upper for tag in ['TODO', 'FIXME', 'HACK', 'TEMP']):
                        # Check if this comment text exists in tracked_todos
                        # simple check: if any of the text (ignoring case) matches a tracked todo
                        clean_line = line.strip().lower()
                        # Removing common comment prefixes
                        clean_line = re.sub(r'^//\s*', '', clean_line)
                        clean_line = re.sub(r'^(todo|fixme|hack|temp)[\s:]*', '', clean_line)

                        is_tracked = False
                        for tracked in tracked_todos:
                            if clean_line and clean_line in tracked:
                                is_tracked = True
                                break

                        if not is_tracked:
                            debt_by_file[rel_path] += 1

                # Error handling inconsistency
                error_patterns = 0
                if 'unwrap()' in content: error_patterns += 1
                if 'expect(' in content: error_patterns += 1
                if '?' in content: error_patterns += 1
                if 'match ' in content: error_patterns += 1

                if error_patterns > 2:
                    debt_by_file[rel_path] += 1

                # Store structs and enums
                matches = re.finditer(r'(?:pub\s+)?(?:struct|enum)\s+([A-Za-z0-9_]+)\s*(?:\{|\()', content)
                for match in matches:
                    name = match.group(1)
                    struct_enum_defs[name].append(rel_path)

                # Magic numbers
                debt_by_file[rel_path] += len(re.findall(r'let\s+[a-zA-Z0-9_]+\s*=\s*[0-9]{2,};', content))

                # Magic strings (e.g. let var = "some string"; excluding const/static)
                # This regex looks for 'let' declarations with string literals.
                magic_strings = re.findall(r'let\s+(?:mut\s+)?[a-zA-Z0-9_]+\s*[:\w<>\s]*\s*=\s*"[^"]+";', content)
                debt_by_file[rel_path] += len(magic_strings)

    # Calculate duplicate struct/enum debt
    for name, locations in struct_enum_defs.items():
        if len(locations) > 1:
            for loc in set(locations):
                debt_by_file[loc] += 1

    if copied_clippy:
        os.remove(temp_clippy)

    total_debt = sum(debt_by_file.values())
    return total_debt, debt_by_file

def main():
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

    stdout, rc = run_cmd("git log -n 6 --format=%H", cwd=repo_root)
    commits = [c for c in stdout.strip().split('\n') if c]

    if not commits:
        print("No commits found.")
        return

    orig_stdout, _ = run_cmd("git rev-parse --abbrev-ref HEAD", cwd=repo_root)
    orig_branch = orig_stdout.strip()
    if orig_branch == "HEAD":
        orig_branch = commits[0] # detached head

    # Backup clippy.toml
    mythrax_core = os.path.join(repo_root, "mythrax-core")
    clippy_toml_path = os.path.join(mythrax_core, "clippy.toml")
    backup_clippy_path = "/tmp/mythrax_clippy.toml"

    if os.path.exists(clippy_toml_path):
        shutil.copy2(clippy_toml_path, backup_clippy_path)

    results_total = {}
    results_by_file = {}

    try:
        for commit in commits:
            tot, by_file = analyze_commit(repo_root, commit, backup_clippy_path)
            results_total[commit] = tot
            results_by_file[commit] = by_file
    finally:
        print(f"Restoring original state ({orig_branch})...", file=sys.stderr)
        run_cmd(f"git checkout --quiet {orig_branch}", cwd=repo_root)

        # Restore original clippy.toml if it existed
        if os.path.exists(backup_clippy_path):
            shutil.copy2(backup_clippy_path, clippy_toml_path)
            os.remove(backup_clippy_path)

    # Generate Scorecard
    scorecard = "## Sanitation Scorecard\n\n"
    scorecard += "| Commit | Total Debt |\n"
    scorecard += "|---|---|\n"

    for commit in reversed(commits):
        scorecard += f"| {commit[:7]} | {results_total[commit]} |\n"

    trajectory = "Stable"
    if len(commits) > 1:
        current = commits[0]
        prev = commits[1]
        if results_total[current] < results_total[prev]:
            trajectory = "Improving"
        elif results_total[current] > results_total[prev]:
            trajectory = "Degrading"

    scorecard += f"\n**Trajectory:** {trajectory}\n"

    # Increasing debt density files
    scorecard += "\n### Files with Increasing Debt Density (Current vs Previous)\n"
    current_files = results_by_file[commits[0]]
    if len(commits) > 1:
        prev_files = results_by_file[commits[1]]
        increasing_files = []
        for f, debt in current_files.items():
            prev_debt = prev_files.get(f, 0)
            if debt > prev_debt:
                increasing_files.append((f, prev_debt, debt))

        if increasing_files:
            scorecard += "| File | Previous Debt | Current Debt |\n"
            scorecard += "|---|---|---|\n"
            for f, p, c in increasing_files:
                scorecard += f"| {f} | {p} | {c} |\n"
        else:
            scorecard += "No files with increasing debt.\n"

    print(scorecard)

    # Append findings as comments to the relevant PR if the trigger is a push event.
    github_event = os.environ.get("GITHUB_EVENT_NAME")
    if github_event in ["push", "pull_request"]:
        # Find PR number or post to PR
        pr_number = os.environ.get("PR_NUMBER")
        if not pr_number:
            # Try to get PR associated with the commit using gh
            stdout, rc = run_cmd(f"gh pr list --search {commits[0]} --json number --jq '.[0].number'", cwd=repo_root)
            pr_number = stdout.strip()

        if pr_number and pr_number != 'null':
            # Write to a temp file
            with open("scorecard.md", "w") as f:
                f.write(scorecard)
            run_cmd(f"gh pr comment {pr_number} --body-file scorecard.md", cwd=repo_root)
            os.remove("scorecard.md")
        else:
            print("No PR found for this commit.", file=sys.stderr)

if __name__ == "__main__":
    main()
