#!/usr/bin/env python3
import os
import subprocess
import json
import re
import sys
from collections import Counter

def get_git_commits(limit=6):
    try:
        output = subprocess.check_output(['git', 'log', f'-{limit}', '--format=%H']).decode('utf-8')
        return [commit.strip() for commit in output.split('\n') if commit.strip()]
    except Exception:
        return []

def get_todo_md():
    try:
        with open('TODO.md', 'r', encoding='utf-8') as f:
            return f.read()
    except Exception:
        return ""

def scan_commit(commit_hash=None):
    metrics = {
        'dead_code_and_suppressions': 0,
        'orphaned_todos': 0,
        'high_complexity': 0,
        'inconsistent_error_handling': 0,
        'duplicated_structs': 0,
        'magic_numbers': 0,
        'string_literals': 0,
        'total_debt': 0
    }
    file_debt = {}

    def add_file_debt(path, amount=1):
        if path.startswith('./'):
            path = path[2:]
        file_debt[path] = file_debt.get(path, 0) + amount

    # clippy
    try:
        output = subprocess.check_output(
            ['cargo', 'clippy', '--message-format=json'],
            cwd='mythrax-core',
            stderr=subprocess.DEVNULL
        ).decode('utf-8')

        for line in output.split('\n'):
            if not line.strip(): continue
            try:
                msg = json.loads(line)
                if msg.get('reason') == 'compiler-message':
                    message = msg.get('message', {})
                    code_val = message.get('code', {})
                    span = message.get('spans', [{}])[0] if message.get('spans') else {}
                    file_path = span.get('file_name', '')

                    if code_val:
                        code_str = code_val.get('code', '')
                        if code_str == 'clippy::cognitive_complexity':
                            metrics['high_complexity'] += 1
                            if file_path: add_file_debt('mythrax-core/' + file_path)
                        elif code_str in ('dead_code', 'unused_imports', 'unreachable_code'):
                            metrics['dead_code_and_suppressions'] += 1
                            if file_path: add_file_debt('mythrax-core/' + file_path)
            except:
                pass
    except Exception as e:
        pass

    todo_md = get_todo_md()
    struct_defs = set()

    files_to_scan = []
    for root, dirs, files in os.walk('.'):
        if '.git' in root or 'target' in root:
            continue
        for file in files:
            if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                files_to_scan.append(os.path.join(root, file))

    for path in files_to_scan:
        try:
            with open(path, 'r', encoding='utf-8') as f:
                content = f.read()
        except Exception:
            continue

        dead_code_suppressions = len(re.findall(r'#\[allow\(dead_code\)\]', content))
        if dead_code_suppressions > 0:
            metrics['dead_code_and_suppressions'] += dead_code_suppressions
            add_file_debt(path, dead_code_suppressions)

        for line in content.split('\n'):
            match = re.search(r'\b(TODO|FIXME|HACK|TEMP)\b[^\w]*([\w\s]+)', line)
            if match:
                desc = match.group(2).strip()
                if desc and desc not in todo_md:
                    metrics['orphaned_todos'] += 1
                    add_file_debt(path)

            if path.endswith('.rs'):
                struct_match = re.search(r'(?:struct|enum)\s+([A-Z][a-zA-Z0-9_]*)', line)
                if struct_match:
                    name = struct_match.group(1)
                    if name in struct_defs:
                        metrics['duplicated_structs'] += 1
                        add_file_debt(path)
                    struct_defs.add(name)

        if path.endswith('.rs'):
            unwraps = len(re.findall(r'\.unwrap\(\)', content))
            expects = len(re.findall(r'\.expect\(', content))
            questions = len(re.findall(r'\?', content))
            matches = len(re.findall(r'match\s+.*(?:Ok|Err)', content))

            if (unwraps > 0 or expects > 0) and (questions > 0 or matches > 0):
                debt = unwraps + expects
                metrics['inconsistent_error_handling'] += debt
                add_file_debt(path, debt)

        magic_numbers = len(re.findall(r'\b(?!(?:0|1|2)\b)\d+\b', content))
        scaled_magic = magic_numbers // 10
        if scaled_magic > 0:
            metrics['magic_numbers'] += scaled_magic
            add_file_debt(path, scaled_magic)

        strings = re.findall(r'"([^"\\]{4,})"', content)
        str_counts = Counter(strings)
        duplicate_strings = sum(1 for v in str_counts.values() if v > 1)
        if duplicate_strings > 0:
            metrics['string_literals'] += duplicate_strings
            add_file_debt(path, duplicate_strings)

    metrics['total_debt'] = sum(metrics.values()) - metrics.get('total_debt', 0)
    return metrics, file_debt

def main():
    commits = get_git_commits(6)
    if not commits:
        commits = get_git_commits(1)
        if not commits:
            print("No commits found.")
            sys.exit(0)

    original_commit = commits[0]
    history = []

    clippy_toml_path = 'mythrax-core/clippy.toml'
    clippy_content = "cognitive-complexity-threshold = 15\n"

    print("Gathering metrics for commits...")
    for commit in commits:
        subprocess.run(['git', 'checkout', '-f', commit], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        try:
            os.makedirs('mythrax-core', exist_ok=True)
            with open(clippy_toml_path, 'w') as f:
                f.write(clippy_content)
        except Exception:
            pass

        try:
            metrics, file_debt = scan_commit(commit)
            history.append((commit, metrics, file_debt))
        finally:
            if os.path.exists(clippy_toml_path):
                os.remove(clippy_toml_path)

    subprocess.run(['git', 'checkout', '-f', original_commit], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        os.makedirs('mythrax-core', exist_ok=True)
        with open(clippy_toml_path, 'w') as f:
            f.write(clippy_content)
    except Exception:
        pass

    current_metrics, current_file_debt = history[0][1], history[0][2]
    old_file_debt = history[1][2] if len(history) > 1 else {}

    report = ["# Sanitation Scorecard\n"]

    report.append("## Current Debt")
    for k, v in current_metrics.items():
        if k != 'total_debt':
            report.append(f"- **{k}**: {v}")

    report.append("\n## Trajectory (Last 5 Commits)")
    report.append("| Commit | Total Debt | Trend |")
    report.append("|---|---|---|")

    prev_debt = None
    for commit, metrics, _ in reversed(history):
        trend = "➖"
        if prev_debt is not None:
            if metrics['total_debt'] > prev_debt:
                trend = "🔴 Degrading"
            elif metrics['total_debt'] < prev_debt:
                trend = "🟢 Improving"
        report.append(f"| {commit[:7]} | {metrics['total_debt']} | {trend} |")
        prev_debt = metrics['total_debt']

    increasing_files = []
    for f, current_val in current_file_debt.items():
        old_val = old_file_debt.get(f, 0)
        if current_val > old_val:
            increasing_files.append(f)

    if increasing_files:
        report.append("\n## ⚠️ Files with Increasing Debt Density")
        for f in increasing_files[:10]:
            report.append(f"- `{f}`")
        if len(increasing_files) > 10:
            report.append("- ...")

    report_text = "\n".join(report)
    print(report_text)

    if os.environ.get('GITHUB_EVENT_NAME') in ('pull_request', 'push'):
        try:
            pr_number = None
            if os.environ.get('GITHUB_EVENT_NAME') == 'pull_request':
                with open(os.environ['GITHUB_EVENT_PATH'], 'r') as f:
                    event = json.load(f)
                    pr_number = event.get('pull_request', {}).get('number')
            else:
                try:
                    pr_out = subprocess.check_output(['gh', 'pr', 'list', '--search', original_commit, '--json', 'number']).decode('utf-8')
                    prs = json.loads(pr_out)
                    if prs:
                        pr_number = prs[0]['number']
                except:
                    pass

            if pr_number:
                subprocess.run(['gh', 'pr', 'comment', str(pr_number), '--body', report_text], check=True)
        except Exception as e:
            print(f"Failed to comment on PR: {e}")

if __name__ == '__main__':
    main()
