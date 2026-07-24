import os
import sys
import subprocess
import json
import re

def get_files_to_scan():
    files = []
    for root, dirs, f in os.walk('mythrax-core'):
        if 'target' in dirs:
            dirs.remove('target')
        for file in f:
            if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                files.append(os.path.join(root, file))
    for root, dirs, f in os.walk('scripts'):
        for file in f:
            if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                files.append(os.path.join(root, file))
    return files

def parse_clippy_json(json_file):
    debt = {}
    if not os.path.exists(json_file):
        return debt

    with open(json_file, 'r') as f:
        for line in f:
            if not line.strip():
                continue
            try:
                msg = json.loads(line)
                if msg.get('reason') == 'compiler-message' and msg.get('message', {}).get('code'):
                    code = msg['message']['code'].get('code', '')
                    level = msg['message'].get('level', '')
                    if level in ['warning', 'error']:
                        spans = msg['message'].get('spans', [])
                        if spans:
                            primary_span = next((s for s in spans if s.get('is_primary')), spans[0])
                            file_name = primary_span.get('file_name')
                            if file_name:
                                if file_name not in debt:
                                    debt[file_name] = 0
                                debt[file_name] += 1
            except json.JSONDecodeError:
                pass
    return debt

def scan_file_for_regex_debt(filepath, todo_items):
    file_debt = 0
    duplicates = []

    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
    except Exception:
        return 0, []

    # 1. Orphaned TODO/FIXME/HACK/TEMP
    matches = re.finditer(r'(?i)\b(TODO|FIXME|HACK|TEMP)\b[^\n]*', content)
    for match in matches:
        comment_text = match.group(0).lower()

        # A simple check: if the actual text of the comment isn't found anywhere in the TODO.md text, it's orphaned.
        # Since the comment might be just "TODO", let's extract words > 3 chars
        words = re.findall(r'\b\w{4,}\b', comment_text)
        is_tracked = False
        if words:
            for w in words:
                if w in todo_items.lower():
                    is_tracked = True
                    break
        else:
            is_tracked = False # Just "TODO" is technically orphaned debt as it's not descriptive enough to be tracked.

        if not is_tracked:
            file_debt += 1

    if filepath.endswith('.rs'):
        # #[allow(dead_code)] is debt
        dead_code_allows = re.findall(r'#\[allow\(dead_code\)\]', content)
        file_debt += len(dead_code_allows)

        # Magic strings/numbers
        lines = content.split('\n')
        for line in lines:
            if 'const ' in line:
                continue
            # Magic numbers
            magic_nums = re.findall(r'(?<![a-zA-Z0-9_])(?:[2-9]|\d{2,})(?![a-zA-Z0-9_])', line)
            file_debt += len(magic_nums)

            # Magic strings: string literals that aren't empty or single chars or basic punctuation,
            # and aren't inside macros like println!, format!, panic!, assert! etc.
            # It's tricky to regex parse all Rust strings correctly, but we can do a rough check.
            if not any(m in line for m in ['println!', 'format!', 'panic!', 'assert', 'log::', 'tracing::', 'Err(', 'anyhow!']):
                magic_strs = re.findall(r'"([^"\\]{4,})"', line)
                file_debt += len(magic_strs)

        # 3. Structs/Enums for duplication check
        structs = re.findall(r'\b(?:pub\s+)?(?:struct|enum)\s+([A-Z][a-zA-Z0-9_]*)', content)
        duplicates.extend(structs)

    return file_debt, duplicates

def run():
    print("Running Architect Scan...")

    original_commit = subprocess.check_output(['git', 'rev-parse', 'HEAD']).decode('utf-8').strip()
    commits = subprocess.check_output(['git', 'log', '-n', '6', '--format=%H']).decode('utf-8').strip().split('\n')

    # Save current clippy.toml
    clippy_toml_path = 'mythrax-core/clippy.toml'
    clippy_content = None
    if os.path.exists(clippy_toml_path):
        with open(clippy_toml_path, 'r') as f:
            clippy_content = f.read()

    # Track TODO.md
    todo_content = ""
    if os.path.exists('TODO.md'):
        with open('TODO.md', 'r', encoding='utf-8') as f:
            todo_content = f.read()

    history_debt = {}
    current_duplicates = {}

    try:
        for i, commit in enumerate(reversed(commits)):
            print(f"Checking commit {commit}")
            # Ensure no untracked files block checkout
            if os.path.exists(clippy_toml_path):
                os.remove(clippy_toml_path)

            subprocess.run(['git', 'checkout', '-q', commit], check=True)

            # Ensure clippy.toml exists
            if clippy_content and not os.path.exists(clippy_toml_path):
                os.makedirs('mythrax-core', exist_ok=True)
                with open(clippy_toml_path, 'w') as f:
                    f.write(clippy_content)

            clippy_json_path = 'clippy_output.json'
            # Run clippy in mythrax-core
            subprocess.run('cd mythrax-core && cargo clippy --message-format=json -A warnings -W clippy::cognitive_complexity -W dead_code -W unreachable_code -W unused_imports -W clippy::unwrap_used -W clippy::expect_used > ../' + clippy_json_path + ' 2>/dev/null', shell=True)

            clippy_debt = parse_clippy_json(clippy_json_path)

            total_debt_for_commit = 0
            file_debt_map = {}

            all_structs = {}

            # Read files at current commit
            files_to_scan = get_files_to_scan()

            # Read TODO.md at current commit if it exists
            current_todo = ""
            if os.path.exists('TODO.md'):
                with open('TODO.md', 'r', encoding='utf-8') as f:
                    current_todo = f.read()
            else:
                current_todo = todo_content # fallback

            for filepath in files_to_scan:
                regex_debt, structs = scan_file_for_regex_debt(filepath, current_todo)
                file_debt_map[filepath] = file_debt_map.get(filepath, 0) + regex_debt

                for c_file, c_debt in clippy_debt.items():
                    if filepath.endswith(c_file):
                        file_debt_map[filepath] = file_debt_map.get(filepath, 0) + c_debt

                for s in structs:
                    all_structs[s] = all_structs.get(s, 0) + 1

            for filepath, count in file_debt_map.items():
                total_debt_for_commit += count

            duplicate_debt = 0
            for s, count in all_structs.items():
                if count > 1:
                    duplicate_debt += count

            total_debt_for_commit += duplicate_debt

            history_debt[commit] = {
                'total': total_debt_for_commit,
                'files': file_debt_map,
                'is_current': (i == len(commits) - 1)
            }

            if i == len(commits) - 1:
                current_duplicates = {k: v for k, v in all_structs.items() if v > 1}

    finally:
        if os.path.exists(clippy_toml_path):
            os.remove(clippy_toml_path)
        subprocess.run(['git', 'checkout', '-q', original_commit], check=True)
        if os.path.exists('clippy_output.json'):
            os.remove('clippy_output.json')
        # Restore clippy.toml just in case
        if clippy_content:
            os.makedirs('mythrax-core', exist_ok=True)
            with open(clippy_toml_path, 'w') as f:
                f.write(clippy_content)

    # Generate Report
    report = ["# Architecture Sanitation Scorecard\n"]

    commits_list = list(history_debt.keys())
    oldest = commits_list[0]
    current = commits_list[-1]

    oldest_total = history_debt[oldest]['total']
    current_total = history_debt[current]['total']

    report.append(f"**Debt Trajectory:** Oldest ({oldest[:7]}): {oldest_total} -> Current ({current[:7]}): {current_total}")

    if current_total > oldest_total:
        report.append("📉 **Trajectory:** DEGRADING (Debt is increasing)\n")
    elif current_total < oldest_total:
        report.append("📈 **Trajectory:** IMPROVING (Debt is decreasing)\n")
    else:
        report.append("➡️ **Trajectory:** STAGNANT (Debt is unchanged)\n")

    report.append("## Files with Increasing Debt Density (Current vs Oldest)")
    oldest_files = history_debt[oldest]['files']
    current_files = history_debt[current]['files']

    increasing_files = []
    for f, c_debt in current_files.items():
        o_debt = oldest_files.get(f, 0)
        if c_debt > o_debt:
            increasing_files.append((f, o_debt, c_debt))

    if increasing_files:
        for f, o, c in increasing_files:
            report.append(f"- `{f}`: {o} -> {c}")
    else:
        report.append("- No files with increasing debt.")

    if current_duplicates:
        report.append("\n## Duplicated Structs/Enums")
        for k, v in current_duplicates.items():
            report.append(f"- `{k}` found {v} times")

    report_str = "\n".join(report)
    print(report_str)

    # GitHub PR Comment
    event_name = os.environ.get('GITHUB_EVENT_NAME')
    if event_name in ['push', 'pull_request']:
        try:
            # We want to comment on the PR if it exists.
            # If it's a push event, we must find the open PR.
            # If it's a PR event, gh pr comment should work directly if we use the branch name.
            ref_name = os.environ.get('GITHUB_REF_NAME')

            # For push, ref_name is often the branch name. In detached head, it might be the branch from GITHUB_REF_NAME.
            # The memory explicitly says: "If triggered by a push event, append findings as PR comments."
            if event_name == 'push':
                pr_list_cmd = ['gh', 'pr', 'list', '--head', ref_name, '--json', 'number', '--jq', '.[0].number']
                pr_num = subprocess.check_output(pr_list_cmd, text=True).strip()
            elif event_name == 'pull_request':
                # For pull_request event, GITHUB_REF_NAME might be a merge ref. It's safer to use the PR number from env if available,
                # but gh pr comment usually works implicitly in PR context if we provide the branch or PR number.
                # Let's try searching for the head branch or just rely on gh pr comment finding it.
                head_ref = os.environ.get('GITHUB_HEAD_REF', ref_name)
                pr_list_cmd = ['gh', 'pr', 'list', '--head', head_ref, '--json', 'number', '--jq', '.[0].number']
                pr_num = subprocess.check_output(pr_list_cmd, text=True).strip()

            if pr_num and pr_num != 'null':
                import shlex
                safe_body = shlex.quote(report_str)
                comment_cmd = f"gh pr comment {pr_num} --body {safe_body}"
                subprocess.run(comment_cmd, shell=True, check=True)
                print(f"Commented on PR #{pr_num}")
            else:
                print("No open PR found for this branch.")
        except Exception as e:
            print(f"Error commenting on PR: {e}")

if __name__ == "__main__":
    run()
