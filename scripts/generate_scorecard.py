import os
import re
import subprocess
import urllib.request
import json
import sys
from collections import defaultdict

def get_todo_md_items():
    items = []
    if os.path.exists('TODO.md'):
        with open('TODO.md', 'r', encoding='utf-8') as f:
            for line in f:
                if line.strip().startswith('- [ ]'):
                    items.append(line.strip().replace('- [ ]', '').strip().lower())
    return items

def scan_commit(commit_hash, todo_md_items):
    result = subprocess.run(['git', 'ls-tree', '-r', '--name-only', commit_hash], capture_output=True, text=True)
    files = [f for f in result.stdout.splitlines() if (f.endswith('.rs') or f.endswith('.py') or f.endswith('.sh'))]

    metrics = {
        'dead_code': 0,
        'todo_orphaned': 0,
        'complex_funcs': 0,
        'mixed_error_files': 0,
        'magic_numbers': 0,
        'duplicated_structs': 0,
    }

    file_debt = defaultdict(int)

    allow_dead_code_re = re.compile(r'#\[allow\(dead_code\)\]')
    # Support both // for Rust and # for Python/Bash
    todo_re = re.compile(r'(?://|#)\s*(TODO|FIXME|HACK|TEMP):?\s*(.*)', re.IGNORECASE)
    branch_re = re.compile(r'\b(if|for|while|match|loop)\b')
    # Magic numbers and string literals
    magic_num_re = re.compile(r'\b(?!0\b|1\b|2\b)[0-9]{3,}\b')
    string_literal_re = re.compile(r'(?<![A-Za-z_])"[^"]{4,}"')

    struct_definitions = []

    for file in files:
        if 'scratch/' in file or 'target/' in file: continue

        file_content_res = subprocess.run(['git', 'show', f'{commit_hash}:{file}'], capture_output=True, text=True, errors='ignore')
        lines = file_content_res.stdout.splitlines()

        function_complexity = 0
        current_function = None
        has_unwrap = False
        has_expect = False
        has_question = False
        has_match = False

        for i, line in enumerate(lines):
            stripped_line = line.strip()

            if allow_dead_code_re.search(line):
                metrics['dead_code'] += 1
                file_debt[file] += 1

            todo_match = todo_re.search(line)
            if todo_match:
                desc = todo_match.group(2).strip().lower()
                is_orphaned = True
                for item in todo_md_items:
                    if desc and (desc in item or item in desc):
                        is_orphaned = False
                        break
                if is_orphaned:
                    metrics['todo_orphaned'] += 1
                    file_debt[file] += 1

            # Magic numbers and string literals check
            if not stripped_line.startswith('//') and not stripped_line.startswith('/*') and not stripped_line.startswith('*') and not stripped_line.startswith('#'):
                if (magic_num_re.search(line) or string_literal_re.search(line)) and not ('const ' in line or 'static ' in line or '#[' in line):
                     metrics['magic_numbers'] += 1

            if '.unwrap()' in line: has_unwrap = True
            if '.expect(' in line: has_expect = True
            if '?' in line and not stripped_line.startswith('//'): has_question = True
            if 'match ' in line: has_match = True

            if ' fn ' in line or stripped_line.startswith('fn '):
                if current_function and function_complexity > 15:
                    metrics['complex_funcs'] += 1
                    file_debt[file] += 1

                match = re.search(r'fn\s+([a-zA-Z0-9_]+)', line)
                if match:
                    current_function = match.group(1)
                    function_complexity = 1

            if current_function and branch_re.search(line):
                function_complexity += 1

            # Catch struct and enum definitions
            struct_match = re.search(r'(?:struct|enum)\s+([a-zA-Z0-9_]+)', line)
            if struct_match:
                struct_definitions.append(struct_match.group(1))

        if current_function and function_complexity > 15:
            metrics['complex_funcs'] += 1
            file_debt[file] += 1

        error_types = sum([has_unwrap, has_expect, has_question, has_match])
        if error_types > 2:
            metrics['mixed_error_files'] += 1
            file_debt[file] += 1

    struct_counts = {}
    for s in struct_definitions:
        struct_counts[s] = struct_counts.get(s, 0) + 1
        if struct_counts[s] == 2:
            metrics['duplicated_structs'] += 1

    return metrics, file_debt

def post_pr_comment(report_body):
    token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    event_path = os.environ.get('GITHUB_EVENT_PATH')

    if not (token and repo and event_path):
        print("Running locally or missing environment variables, PR comment not posted.")
        return

    try:
        with open(event_path, 'r') as f:
            event = json.load(f)
    except Exception as e:
        print(f"Failed to read event path: {e}")
        return

    pr_number = None
    if 'pull_request' in event:
        pr_number = event['pull_request']['number']
    elif 'commits' in event and len(event['commits']) > 0:
        # It's a push event. Try to find the associated PR.
        commit_sha = event['after']
        if commit_sha:
            url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
            headers = {
                "Authorization": f"Bearer {token}",
                "Accept": "application/vnd.github+json"
            }
            req = urllib.request.Request(url, headers=headers)
            try:
                with urllib.request.urlopen(req) as response:
                    pulls = json.loads(response.read().decode('utf-8'))
                    if pulls and len(pulls) > 0:
                        pr_number = pulls[0]['number']
            except Exception as e:
                print(f"Failed to find PR for push event: {e}")

    if not pr_number:
        print("Could not determine PR number for this event.")
        return

    url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json",
        "Content-Type": "application/json"
    }
    data = json.dumps({"body": report_body}).encode('utf-8')

    req = urllib.request.Request(url, data=data, headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Successfully posted PR comment to PR #{pr_number}.")
    except Exception as e:
        print(f"Failed to post PR comment: {e}")

def generate_report():
    commits_res = subprocess.run(['git', 'log', '--format=%H', '-n', '6'], capture_output=True, text=True)
    commits = [c for c in commits_res.stdout.splitlines() if c.strip()]
    commits.reverse()

    if len(commits) == 0:
        return "No commits found to scan."

    todo_items = get_todo_md_items()
    history = []

    report = ["## Chief Architect: Codebase Sanitation Scorecard\n"]

    for i, commit in enumerate(commits):
        metrics, file_debt = scan_commit(commit, todo_items)
        history.append((commit, metrics, file_debt))

        label = "Current Commit" if i == len(commits)-1 else f"Commit {i - len(commits) + 1}"
        short_hash = commit[:7]

        if i == len(commits) - 1:
            report.append(f"### {label} ({short_hash})")
            report.append(f"- Dead Code Suppressions (`#[allow(dead_code)]`): {metrics['dead_code']}")
            report.append(f"- Orphaned TODOs/Hacks (Not in TODO.md): {metrics['todo_orphaned']}")
            report.append(f"- Complex Functions (>15 Cyclomatic): {metrics['complex_funcs']}")
            report.append(f"- Files w/ Inconsistent Error Handling: {metrics['mixed_error_files']}")
            report.append(f"- Potentially Duplicated Structs/Enums: {metrics['duplicated_structs']}")
            report.append(f"- Magic Numbers & String Literals (Approx): {metrics['magic_numbers']}\n")

    report.append("### Trajectory Analysis (Last 5 Commits)")
    if len(history) > 1:
        oldest = history[0][1]
        newest = history[-1][1]

        for key in oldest.keys():
            diff = newest[key] - oldest[key]
            trend = "🔴 Degrading" if diff > 0 else "🟢 Improving" if diff < 0 else "⚪ Stable"
            report.append(f"- **{key}**: {oldest[key]} -> {newest[key]} ({trend} {diff:+d})")

        report.append("\n### 🚩 Files with Increasing Debt Density")
        old_debt = history[0][2]
        new_debt = history[-1][2]

        increasing = False
        for file, debt in new_debt.items():
            if debt > old_debt.get(file, 0):
                increasing = True
                diff = debt - old_debt.get(file, 0)
                report.append(f"- `{file}`: +{diff} issues ({old_debt.get(file, 0)} -> {debt})")

        if not increasing:
            report.append("- No files showed increasing debt density.")
    else:
        report.append("Not enough history to perform trajectory analysis (less than 2 commits found).")

    return "\n".join(report)

if __name__ == '__main__':
    report = generate_report()
    print(report)
    post_pr_comment(report)
