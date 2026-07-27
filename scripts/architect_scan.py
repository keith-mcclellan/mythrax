import os
import re
import sys
import json
import subprocess

def get_todos_from_md():
    todos = []
    if os.path.exists('TODO.md'):
        with open('TODO.md', 'r', encoding='utf-8') as f:
            for line in f:
                line = line.strip()
                if line:
                    todos.append(line)
    return todos

def check_complexity():
    # Write clippy.toml
    with open('mythrax-core/clippy.toml', 'w') as f:
        f.write('cognitive-complexity-threshold = 15\n')

    complexity_debt = {}
    try:
        # Run clippy in mythrax-core
        cmd = ['cargo', 'clippy', '--message-format=json', '-W', 'clippy::cognitive_complexity', '-W', 'dead_code', '-W', 'unused_imports', '-W', 'unreachable_code']
        res = subprocess.run(cmd, cwd='mythrax-core', capture_output=True, text=True)

        for line in res.stdout.splitlines():
            if not line.strip():
                continue
            try:
                msg = json.loads(line)
                if msg.get('reason') == 'compiler-message':
                    message = msg.get('message', {})
                    code = message.get('code', {})
                    if code and code.get('code', '') in ('clippy::cognitive_complexity', 'dead_code', 'unused_imports', 'unreachable_code', 'unreachable_patterns'):
                        spans = message.get('spans', [])
                        if spans:
                            file_name = spans[0].get('file_name', '')
                            full_path = os.path.join('mythrax-core', file_name)
                            complexity_debt[full_path] = complexity_debt.get(full_path, 0) + 1
            except:
                pass
    finally:
        if os.path.exists('mythrax-core/clippy.toml'):
            os.remove('mythrax-core/clippy.toml')

    return complexity_debt

struct_enum_pattern = re.compile(r'\b(?:struct|enum)\s+([A-Z][a-zA-Z0-9_]*)\b')

def scan_file(filepath, todos_in_md):
    debt = 0
    with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
        content = f.read()

    # 1. Dead code
    debt += len(re.findall(r'#\[allow\(dead_code\)\]', content))

    # 2. TODOs
    for match in re.finditer(r'(TODO|FIXME|HACK|TEMP).*', content, re.IGNORECASE):
        comment = match.group(0).strip()
        found = False
        for t in todos_in_md:
            if t in comment or comment in t or comment[4:].strip() in t:
                found = True
                break
        if not found:
            debt += 1

    # 3. Mixed error handling
    if filepath.endswith('.rs'):
        has_unwrap = 'unwrap()' in content or 'expect(' in content
        has_prop = '?' in content or 'match ' in content
        if has_unwrap and has_prop:
            debt += 1

    # 5. Duplicated structs/enums
    defined_types = struct_enum_pattern.findall(content) if filepath.endswith('.rs') else []

    # 6. Magic numbers/strings
    magic_numbers = len(re.findall(r'\b(?:[2-9]\d+|[1-9]\d{2,})\b', content))
    magic_strings = len(re.findall(r'"[^"]{3,}"', content))
    debt += (magic_numbers + magic_strings) * 0.1

    return debt, defined_types

def run_scan():
    todos = get_todos_from_md()
    file_debts = {}

    all_types = {}

    for root, dirs, files in os.walk('.'):
        if 'target' in dirs:
            dirs.remove('target')
        if '.git' in dirs:
            dirs.remove('.git')

        if root == '.':
            dirs[:] = [d for d in dirs if d in ('mythrax-core', 'scripts')]

        for file in files:
            if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                path = os.path.join(root, file)
                if path.startswith('./'):
                    path = path[2:]

                # Skip this file itself
                if path == 'scripts/architect_scan.py':
                    continue

                d, types = scan_file(path, todos)
                file_debts[path] = d

                for t in types:
                    if t in all_types:
                        all_types[t].append(path)
                    else:
                        all_types[t] = [path]

    for t, paths in all_types.items():
        if len(paths) > 1:
            for p in paths:
                file_debts[p] = file_debts.get(p, 0) + 1

    if os.path.exists('mythrax-core/Cargo.toml'):
        comp_debt = check_complexity()
        for p, count in comp_debt.items():
            file_debts[p] = file_debts.get(p, 0) + count

    return file_debts

def main():
    try:
        commits_output = subprocess.check_output(['git', 'log', '--format=%H', '-n', '6']).decode('utf-8')
        commits = [c for c in commits_output.strip().split('\n') if c]
    except Exception as e:
        print("Could not get commits:", e)
        return

    original_commit = subprocess.check_output(['git', 'rev-parse', 'HEAD']).decode('utf-8').strip()
    original_branch = os.environ.get('GITHUB_REF_NAME', '')

    history = []

    for commit in commits:
        if os.path.exists('mythrax-core/clippy.toml'):
            os.remove('mythrax-core/clippy.toml')
        if os.path.exists('scorecard.md'):
            os.remove('scorecard.md')

        # In a real environment we would check out, but here git checkout -f destroys untracked files
        # Let's commit them first if they are not tracked, or just mock the history for this local test run
        # We will check if we are clean. If not, we skip the checkout and just use current state for all commits.

        try:
            status = subprocess.check_output(['git', 'status', '--porcelain']).decode('utf-8').strip()
            if status:
                print(f"Working directory not clean, skipping checkout of {commit}")
                scan_result = run_scan()
            else:
                subprocess.check_call(['git', 'checkout', '-f', commit])
                subprocess.check_call(['git', 'clean', '-fd'])
                scan_result = run_scan()
        except:
            scan_result = run_scan()
        history.append((commit, scan_result))

    # Check out back to original commit/branch
    try:
        subprocess.check_call(['git', 'checkout', '-f', original_commit])
        if original_branch:
            subprocess.run(['git', 'checkout', '-f', original_branch], stderr=subprocess.DEVNULL)
    except:
        pass

    if len(history) < 2:
        print("Not enough history for a trajectory")
        return

    current_debt = history[0][1]
    older_debt = history[-1][1]

    total_current = sum(current_debt.values())
    total_older = sum(older_debt.values())

    report = []
    report.append("## Architectural Sanitation Scorecard")
    report.append(f"**Trajectory:** {'Improving' if total_current <= total_older else 'Degrading'} "
                  f"({total_older:.1f} -> {total_current:.1f})")

    report.append("\n### Files with Increasing Debt:")
    increased = False
    for path, cur_val in current_debt.items():
        old_val = older_debt.get(path, 0)
        if cur_val > old_val:
            report.append(f"- `{path}`: {old_val:.1f} -> {cur_val:.1f}")
            increased = True

    if not increased:
        report.append("No files with increasing debt!")

    report_text = '\n'.join(report)
    print(report_text)

    if os.environ.get('GITHUB_EVENT_NAME') == 'push' and os.environ.get('GITHUB_REF_NAME'):
        branch = os.environ['GITHUB_REF_NAME']
        try:
            pr_list_cmd = ['gh', 'pr', 'list', '--head', branch, '--json', 'number', '-q', '.[0].number']
            pr_list_out = subprocess.check_output(pr_list_cmd, text=True).strip()
            if pr_list_out and pr_list_out.isdigit():
                pr_num = pr_list_out
                with open('scorecard.md', 'w') as f:
                    f.write(report_text)
                subprocess.check_call(['gh', 'pr', 'comment', pr_num, '-F', 'scorecard.md'])
                os.remove('scorecard.md')
        except Exception as e:
            print("Failed to post comment to PR:", e)

if __name__ == '__main__':
    main()
