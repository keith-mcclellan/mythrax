import os
import sys
import re
import json
import subprocess
import tempfile
import shutil
from collections import defaultdict

def get_commits(repo_path, count=6):
    try:
        out = subprocess.check_output(['git', 'log', f'-n{count}', '--format=%H'], cwd=repo_path)
        return out.decode().strip().split('\n')
    except Exception as e:
        print(f"Failed to get commits: {e}")
        return []

def scan_commit(commit_hash, repo_path):
    print(f"Scanning commit {commit_hash}...")
    subprocess.run(['git', 'checkout', '-f', commit_hash], cwd=repo_path, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    # Write clippy.toml if not exists or overwrite
    clippy_toml_path = os.path.join(repo_path, 'mythrax-core', 'clippy.toml')
    if not os.path.exists(os.path.dirname(clippy_toml_path)):
        os.makedirs(os.path.dirname(clippy_toml_path), exist_ok=True)
    with open(clippy_toml_path, 'w') as f:
        f.write("cognitive-complexity-threshold = 15\n")

    metrics = {
        'dead_code_allows': 0,
        'orphaned_todos': 0,
        'complexity_issues': 0,
        'error_handling_inconsistencies': 0,
        'duplicate_structs': 0,
        'magic_numbers': 0,
        'total_debt': 0,
        'file_debt': defaultdict(int)
    }

    # Run clippy
    # Redirect to file to avoid broken pipe issues
    clippy_cmd = "cargo clippy --message-format=json -- -A warnings -W clippy::cognitive_complexity -W dead_code -W unused_imports -W unreachable_code > ../clippy_out.json 2>/dev/null"
    subprocess.run(clippy_cmd, shell=True, cwd=os.path.join(repo_path, 'mythrax-core'))

    clippy_out = os.path.join(repo_path, 'clippy_out.json')
    if os.path.exists(clippy_out):
        with open(clippy_out, 'r') as f:
            for line in f:
                if not line.strip(): continue
                try:
                    msg = json.loads(line)
                    if msg.get('reason') == 'compiler-message':
                        message = msg.get('message', {})
                        code = message.get('code', {}) or {}
                        code_id = code.get('code', '')
                        if code_id in ['clippy::cognitive_complexity', 'cognitive_complexity', 'dead_code', 'unused_imports', 'unreachable_code', 'unreachable_patterns']:
                            spans = message.get('spans', [])
                            if spans:
                                filename = spans[0].get('file_name', '')
                                metrics['file_debt'][filename] += 1
                                if 'cognitive_complexity' in code_id:
                                    metrics['complexity_issues'] += 1
                                else:
                                    # count as dead code/unreachable debt
                                    pass
                except json.JSONDecodeError:
                    pass

    # Read TODO.md
    todo_text = ""
    todo_path = os.path.join(repo_path, 'TODO.md')
    if os.path.exists(todo_path):
        with open(todo_path, 'r') as f:
            todo_text = f.read().lower()

    struct_names = set()
    for root, dirs, files in os.walk(repo_path):
        if 'target' in dirs:
            dirs.remove('target')
        if '.git' in dirs:
            dirs.remove('.git')

        for file in files:
            if not file.endswith('.rs') and not file.endswith('.py') and not file.endswith('.sh'):
                continue

            filepath = os.path.join(root, file)
            relpath = os.path.relpath(filepath, repo_path)

            try:
                with open(filepath, 'r') as f:
                    content = f.read()
            except:
                continue

            # #[allow(dead_code)]
            dead_code_allows = len(re.findall(r'#\[allow\(dead_code\)\]', content))
            metrics['dead_code_allows'] += dead_code_allows
            metrics['file_debt'][relpath] += dead_code_allows

            # Orphaned TODOs
            comments = re.findall(r'//\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE)
            for tag, text in comments:
                clean_text = text.strip().lower()
                # If comment text isn't in TODO.md, it's orphaned
                if len(clean_text) > 5 and clean_text not in todo_text:
                    metrics['orphaned_todos'] += 1
                    metrics['file_debt'][relpath] += 1

            if file.endswith('.rs'):
                # Error handling
                has_unwrap_expect = 'unwrap(' in content or 'expect(' in content
                has_question = '?' in content
                has_match = 'match ' in content
                if has_unwrap_expect and (has_question or has_match):
                    metrics['error_handling_inconsistencies'] += 1
                    metrics['file_debt'][relpath] += 1

                # Struct/Enum duplicates
                structs = re.findall(r'\b(?:struct|enum)\s+([A-Z][a-zA-Z0-9_]*)', content)
                for name in structs:
                    if name in struct_names:
                        metrics['duplicate_structs'] += 1
                        metrics['file_debt'][relpath] += 1
                    else:
                        struct_names.add(name)

                # Magic numbers
                magic = len(re.findall(r'==\s*\d{2,}', content)) + len(re.findall(r'==\s*"[^"]+"', content))
                metrics['magic_numbers'] += magic
                metrics['file_debt'][relpath] += magic

    metrics['total_debt'] = sum(metrics['file_debt'].values())
    return metrics

def main():
    original_cwd = os.getcwd()

    with tempfile.TemporaryDirectory() as tmpdir:
        repo_tmp = os.path.join(tmpdir, 'repo')
        shutil.copytree(original_cwd, repo_tmp)

        commits = get_commits(repo_tmp, 6)
        if not commits:
            print("No commits found.")
            sys.exit(0)

        history_metrics = []
        for commit in reversed(commits): # oldest to newest
            m = scan_commit(commit, repo_tmp)
            history_metrics.append((commit, m))

        current_commit, current_metrics = history_metrics[-1]

        # Calculate trajectory
        if len(history_metrics) > 1:
            old_debt = history_metrics[-2][1]['total_debt']
            new_debt = current_metrics['total_debt']
            trajectory = "Improving" if new_debt < old_debt else "Degrading" if new_debt > old_debt else "Flat"
        else:
            trajectory = "Unknown"

        # Identify degrading files (debt increased from older commits to current)
        degrading_files = []
        if len(history_metrics) > 1:
            oldest_metrics = history_metrics[0][1]
            for file, current_debt in current_metrics['file_debt'].items():
                old_debt = oldest_metrics['file_debt'].get(file, 0)
                if current_debt > old_debt:
                    degrading_files.append((file, old_debt, current_debt))

        # Build Scorecard
        scorecard = f"## 🏗️ Chief Architect Sanitation Scorecard\n\n"
        scorecard += f"**Trajectory:** {trajectory}\n"
        scorecard += f"**Total Debt:** {current_metrics['total_debt']} issues\n\n"

        scorecard += "### Current Debt Breakdown\n"
        scorecard += f"- **Dead Code (`#[allow(dead_code)]`):** {current_metrics['dead_code_allows']}\n"
        scorecard += f"- **Orphaned TODOs/FIXMEs:** {current_metrics['orphaned_todos']}\n"
        scorecard += f"- **Complexity Issues (>15):** {current_metrics['complexity_issues']}\n"
        scorecard += f"- **Error Handling Inconsistencies:** {current_metrics['error_handling_inconsistencies']}\n"
        scorecard += f"- **Duplicated Structs/Enums:** {current_metrics['duplicate_structs']}\n"
        scorecard += f"- **Magic Numbers/Strings:** {current_metrics['magic_numbers']}\n\n"

        if degrading_files:
            scorecard += "### ⚠️ Degrading Files (Increasing Debt Density)\n"
            for f, old, new in degrading_files:
                scorecard += f"- `{f}`: {old} -> {new} issues\n"

        print(scorecard)

        # Post to PR if triggered by push event
        event_name = os.environ.get('GITHUB_EVENT_NAME')
        if event_name == 'push':
            branch_name = os.environ.get('GITHUB_REF_NAME')
            if branch_name:
                try:
                    # properly escape branch name for shell
                    import shlex
                    safe_branch = shlex.quote(branch_name)
                    cmd = f"gh pr list --head {safe_branch} --json number --jq '.[0].number'"
                    pr_number = subprocess.check_output(cmd, shell=True, text=True).strip()
                    if pr_number and pr_number != 'null':
                        with open('/tmp/scorecard.md', 'w') as f:
                            f.write(scorecard)
                        subprocess.run(['gh', 'pr', 'comment', pr_number, '-F', '/tmp/scorecard.md'], check=True)
                except Exception as e:
                    print(f"Failed to comment on PR: {e}")

if __name__ == '__main__':
    main()
