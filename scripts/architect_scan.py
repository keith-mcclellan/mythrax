import os
import re
import subprocess
import json
import sys

def get_git_commits(limit=6):
    try:
        output = subprocess.check_output(['git', 'log', f'-{limit}', '--format=%H'], text=True)
        return [line.strip() for line in output.strip().split('\n') if line.strip()]
    except subprocess.CalledProcessError:
        return []

def scan_files_for_debt(directory):
    dead_code_suppressions = 0
    orphaned_todos = 0
    inconsistent_errors = 0
    magic_numbers_str = 0
    duplicated_structs = 0

    todo_md_content = ""
    if os.path.exists('TODO.md'):
        with open('TODO.md', 'r') as f:
            todo_md_content = f.read()

    files_debt = {}
    seen_types = {}

    for root, dirs, files in os.walk(directory):
        if 'target' in dirs: dirs.remove('target')
        if '.git' in dirs: dirs.remove('.git')
        if '.venv' in dirs: dirs.remove('.venv')
        if 'node_modules' in dirs: dirs.remove('node_modules')

        for file in files:
            if not file.endswith('.rs') and not file.endswith('.py') and not file.endswith('.sh'):
                continue

            filepath = os.path.join(root, file)
            with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read()

            debt = 0

            if file.endswith('.rs'):
                type_defs = re.findall(r'(struct|enum)\s+([A-Z][a-zA-Z0-9_]*)', content)
                for type_kind, type_name in type_defs:
                    if type_name in seen_types:
                        duplicated_structs += 1
                        debt += 1
                    else:
                        seen_types[type_name] = filepath

            dead_code = len(re.findall(r'#\[allow\([^)]*dead_code[^)]*\)\]', content))
            dead_code_suppressions += dead_code
            debt += dead_code

            todos = re.findall(r'(?i)(TODO|FIXME|HACK|TEMP).*', content)
            for todo in todos:
                todo_text = todo.strip()
                if todo_text and todo_text not in todo_md_content:
                    orphaned_todos += 1
                    debt += 1

            if file.endswith('.rs'):
                unwraps = content.count('.unwrap()')
                expects = content.count('.expect(')
                questions = content.count('?')
                matches = content.count('match ')

                if (unwraps + expects > 0) and (questions > 0 or matches > 0):
                    inconsistent_errors += (unwraps + expects)
                    debt += (unwraps + expects)

            if file.endswith('.rs'):
                lines = content.split('\n')
                for line in lines:
                    line = line.strip()
                    if line.startswith('//') or line.startswith('const ') or line.startswith('static '):
                        continue
                    magics = re.findall(r'\b(?!0\b)(?!1\b)[0-9]{2,}\b', line)
                    magic_strs = re.findall(r'"[^"]{3,}"', line)

                    magic_count = len(magics) + len(magic_strs)
                    magic_numbers_str += magic_count
                    debt += magic_count

            if debt > 0:
                files_debt[filepath] = debt

    return {
        'dead_code': dead_code_suppressions,
        'orphaned_todos': orphaned_todos,
        'inconsistent_errors': inconsistent_errors,
        'magic_numbers': magic_numbers_str,
        'duplicated_structs': duplicated_structs,
        'files_debt': files_debt
    }

def run_clippy(cwd):
    clippy_toml_path = os.path.join(cwd, 'clippy.toml')
    with open(clippy_toml_path, 'w') as f:
        f.write('cognitive-complexity-threshold = 15\n')

    clippy_debt = 0
    try:
        result = subprocess.run(['cargo', 'clippy', '--message-format=json', '--no-default-features', '--', '-A', 'warnings', '-W', 'clippy::cognitive_complexity', '-W', 'dead_code', '-W', 'unused_imports', '-W', 'unreachable_code'],
                       cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        lines = result.stdout.splitlines()

        for line in lines:
            if not line.strip():
                continue
            try:
                msg = json.loads(line)
                if msg.get('reason') == 'compiler-message' and msg.get('message', {}).get('level') in ('warning', 'error'):
                    code = msg.get('message', {}).get('code', {}).get('code', '')
                    if code in ('dead_code', 'unused_imports', 'unreachable_code', 'clippy::cognitive_complexity'):
                        clippy_debt += 1
            except Exception:
                pass
    except Exception as e:
        pass

    if os.path.exists(clippy_toml_path):
        os.remove(clippy_toml_path)

    return clippy_debt

def analyze_commit(commit_hash):
    subprocess.run(['git', 'checkout', '-f', commit_hash], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if os.path.exists('mythrax-core/clippy.toml'):
        os.remove('mythrax-core/clippy.toml')

    metrics = scan_files_for_debt('.')
    clippy_debt = 0
    if os.path.isdir('mythrax-core'):
        clippy_debt = run_clippy('mythrax-core')
        metrics['clippy_debt'] = clippy_debt
    else:
        metrics['clippy_debt'] = 0

    total_debt = metrics['dead_code'] + metrics['orphaned_todos'] + metrics['inconsistent_errors'] + metrics['magic_numbers'] + metrics['duplicated_structs'] + metrics['clippy_debt']
    return total_debt, metrics

def main():
    commits = get_git_commits(6)
    if not commits:
        print("No commits found.")
        return

    original_commit = commits[0]

    history = []

    for commit in reversed(commits):
        debt, metrics = analyze_commit(commit)
        history.append((commit, debt, metrics))

    subprocess.run(['git', 'checkout', '-f', original_commit], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    if os.path.exists('mythrax-core/clippy.toml'):
        os.remove('mythrax-core/clippy.toml')

    current_metrics = history[-1][2]
    prev_metrics = history[-2][2] if len(history) > 1 else current_metrics

    current_debt = history[-1][1]

    report = "## 🏗️ Architect Sanitation Scan Report\n\n"
    report += "### Trajectory\n"

    trajectory = "Improving 📉" if len(history) > 1 and history[-1][1] < history[0][1] else "Degrading 📈"
    if len(history) > 1 and history[-1][1] == history[0][1]:
        trajectory = "Stable ➡️"

    report += f"**Overall Status:** {trajectory}\n"
    report += f"**Current Debt Score:** {current_debt}\n\n"

    report += "| Commit | Debt Score |\n"
    report += "|--------|------------|\n"
    for commit, debt, _ in history:
        report += f"| `{commit[:7]}` | {debt} |\n"

    report += "\n### Current Debt Breakdown\n"
    report += f"- **Dead Code Suppressions:** {current_metrics['dead_code']}\n"
    report += f"- **Orphaned TODO/FIXME:** {current_metrics['orphaned_todos']}\n"
    report += f"- **Inconsistent Error Handling:** {current_metrics['inconsistent_errors']}\n"
    report += f"- **Magic Numbers and Strings:** {current_metrics['magic_numbers']}\n"
    report += f"- **Duplicated Structs/Enums:** {current_metrics['duplicated_structs']}\n"
    report += f"- **Clippy (Complexity, Unreachable, Unused):** {current_metrics['clippy_debt']}\n\n"

    report += "### Files with Increasing Debt Density\n"

    increasing_files = []
    for f, d in current_metrics['files_debt'].items():
        prev_d = prev_metrics['files_debt'].get(f, 0)
        if d > prev_d:
            increasing_files.append((f, prev_d, d))

    if increasing_files:
        for f, old, new in increasing_files:
            report += f"- `{f}`: {old} ➔ {new} (+{new-old})\n"
    else:
        report += "- None 🎉\n"

    print(report)

    if os.environ.get('GITHUB_EVENT_NAME') in ['pull_request', 'push']:
        pr_number = os.environ.get('GITHUB_PR_NUMBER')
        if not pr_number and os.environ.get('GITHUB_EVENT_NAME') == 'push':
            try:
                commit_msg = subprocess.check_output(['git', 'log', '-1', '--pretty=%B'], text=True)
                match = re.search(r'\(#(\d+)\)', commit_msg)
                if match:
                    pr_number = match.group(1)
            except Exception:
                pass

        if pr_number:
            try:
                subprocess.run(['gh', 'pr', 'comment', pr_number, '-b', report], check=True)
            except subprocess.CalledProcessError as e:
                print(f"Failed to post comment to PR {pr_number}: {e}")
        else:
            print("Could not determine PR number to comment on.")

if __name__ == '__main__':
    main()
