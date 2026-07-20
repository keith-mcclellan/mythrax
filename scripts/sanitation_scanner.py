import os
import subprocess
import json
import re

def get_git_commits(n=6):
    try:
        stdout = subprocess.check_output(['git', 'log', f'-n{n}', '--format=%H']).decode()
        commits = stdout.strip().split('\n')
        if not commits or commits == ['']:
            return []
        return commits
    except Exception as e:
        print(f"Git log failed: {e}")
        return []

def run_clippy(cwd):
    clippy_cmd = "cargo clippy --message-format=json -- -A warnings -W clippy::cognitive_complexity -W clippy::unwrap_used -W clippy::expect_used -W dead_code -W unused_imports -W unreachable_code > clippy_out.json 2>/dev/null"
    subprocess.run(clippy_cmd, shell=True, cwd=cwd)
    clippy_out_path = os.path.join(cwd, 'clippy_out.json')
    output = ""
    if os.path.exists(clippy_out_path):
        with open(clippy_out_path, 'r') as f:
            output = f.read()
        os.remove(clippy_out_path)
    return output

def parse_clippy_json(json_output):
    issues = {}
    for line in json_output.split('\n'):
        if not line.strip():
            continue
        try:
            msg = json.loads(line)
            if msg.get('reason') == 'compiler-message':
                message = msg.get('message', {})
                level = message.get('level')
                if level in ['warning', 'error']:
                    spans = message.get('spans', [])
                    for span in spans:
                        if span.get('is_primary'):
                            file_name = span.get('file_name')
                            if file_name not in issues:
                                issues[file_name] = []
                            issues[file_name].append({
                                'code': message.get('code', {}).get('code') if message.get('code') else None,
                                'message': message.get('message')
                            })
        except:
            pass
    return issues

def scan_file(filepath, todo_content):
    debt = 0
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    allow_dead_code = len(re.findall(r'#\[allow\(dead_code\)\]', content))
    debt += allow_dead_code

    comments = re.findall(r'//\s*(TODO|FIXME|HACK|TEMP):?\s*(.*)', content, re.IGNORECASE)
    orphaned_todos = 0
    for tag, text in comments:
        text = text.strip()
        words = text.lower().split()
        found = False
        for line in todo_content.split('\n'):
            matching_words = [w for w in words if len(w) > 3 and w in line]
            if matching_words and len(matching_words) >= min(2, len([w for w in words if len(w) > 3])):
                found = True
                break
        if not found and words:
            orphaned_todos += 1
    debt += orphaned_todos

    has_unwrap = 'unwrap()' in content
    has_expect = 'expect(' in content
    has_question = '?' in content
    has_match = 'match ' in content

    styles = sum([has_unwrap or has_expect, has_question, has_match])
    inconsistent_errors = 0
    if styles >= 2 and (has_unwrap or has_expect):
         inconsistent_errors += 1
    debt += inconsistent_errors * 2

    structs = re.findall(r'(?:struct|enum)\s+\w+\s*\{([^}]+)\}', content)

    magic_numbers = len(re.findall(r'=\s*\d+;', content)) - len(re.findall(r'=\s*0;', content)) - len(re.findall(r'=\s*1;', content))
    magic_strings = len(re.findall(r'=\s*".+?";', content))
    debt += magic_numbers + magic_strings

    return {
        'debt': debt,
        'allow_dead_code': allow_dead_code,
        'orphaned_todos': orphaned_todos,
        'inconsistent_errors': inconsistent_errors,
        'magic_numbers': magic_numbers,
        'magic_strings': magic_strings,
        'struct_bodies': structs
    }

def analyze_directory(root_dir, todo_content):
    file_stats = {}
    all_structs = {}

    for subdir, dirs, files in os.walk(root_dir):
        if 'target' in dirs:
            dirs.remove('target')
        if '.git' in dirs:
            dirs.remove('.git')

        for file in files:
            if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                filepath = os.path.join(subdir, file)
                try:
                    stats = scan_file(filepath, todo_content)
                    rel_path = os.path.relpath(filepath, root_dir)
                    file_stats[rel_path] = stats

                    for body in stats['struct_bodies']:
                        body_clean = re.sub(r'\s+', '', body)
                        if body_clean not in all_structs:
                            all_structs[body_clean] = []
                        all_structs[body_clean].append(rel_path)
                except Exception as e:
                    pass

    for body, paths in all_structs.items():
        if len(paths) > 1 and len(body) > 15:
            for path in paths:
                file_stats[path]['debt'] += 3

    return file_stats

def main():
    commits = get_git_commits(6)
    if not commits:
        print("No commits found.")
        return

    original_commit = commits[0]
    history_scores = []

    clippy_toml_content = ""
    if os.path.exists('mythrax-core/clippy.toml'):
        with open('mythrax-core/clippy.toml', 'r') as f:
            clippy_toml_content = f.read()

    try:
        for i, commit in enumerate(commits):
            if os.path.exists('mythrax-core/clippy.toml'):
                os.remove('mythrax-core/clippy.toml')

            subprocess.run(['git', 'checkout', commit], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

            if clippy_toml_content:
                os.makedirs('mythrax-core', exist_ok=True)
                with open('mythrax-core/clippy.toml', 'w') as f:
                    f.write(clippy_toml_content)

            todo_content = ""
            if os.path.exists('TODO.md'):
                with open('TODO.md', 'r') as f:
                    todo_content = f.read().lower()

            clippy_json = ""
            if os.path.exists('mythrax-core/Cargo.toml'):
                clippy_json = run_clippy('mythrax-core')

            clippy_issues = parse_clippy_json(clippy_json)

            file_stats = analyze_directory('.', todo_content)

            total_debt = 0
            for f, stats in file_stats.items():
                f_norm = f
                if f.startswith('mythrax-core/'):
                    f_norm = f[len('mythrax-core/'):]

                c_issues = clippy_issues.get(f_norm, [])
                stats['debt'] += len(c_issues)
                total_debt += stats['debt']

            history_scores.append({
                'commit': commit,
                'total_debt': total_debt,
                'file_stats': file_stats
            })
    finally:
        if os.path.exists('mythrax-core/clippy.toml'):
            os.remove('mythrax-core/clippy.toml')
        subprocess.run(['git', 'checkout', original_commit], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if clippy_toml_content:
            os.makedirs('mythrax-core', exist_ok=True)
            with open('mythrax-core/clippy.toml', 'w') as f:
                f.write(clippy_toml_content)

    current = history_scores[0]
    previous = history_scores[1:]

    report = "## Codebase Sanitation Scorecard\n\n"
    report += f"**Current Debt Score:** {current['total_debt']}\n\n"

    report += "### Trajectory (Last 5 commits)\n"
    for p in previous:
        diff = current['total_debt'] - p['total_debt']
        direction = "Degrading" if diff > 0 else "Improving" if diff < 0 else "Stable"
        report += f"- Commit {p['commit'][:7]}: {p['total_debt']} -> {direction} (Delta: {diff:+d})\n"

    report += "\n### Files with Increasing Debt Density\n"
    degrading_files = []
    if previous:
        p1_stats = previous[0]['file_stats']
        for f, stats in current['file_stats'].items():
            p_debt = p1_stats.get(f, {}).get('debt', 0)
            if stats['debt'] > p_debt:
                degrading_files.append((f, p_debt, stats['debt']))

    if degrading_files:
        for f, p, c in degrading_files:
            report += f"- `{f}`: {p} -> {c} debt items\n"
    else:
        report += "No files with increasing debt density. Great job!\n"

    print(report)

    if os.environ.get('GITHUB_ACTIONS') == 'true':
        event_name = os.environ.get('GITHUB_EVENT_NAME')
        try:
            with open('report.md', 'w') as f:
                f.write(report)

            if event_name == 'push':
                commit_hash = subprocess.check_output(['git', 'log', '-1', '--format=%H']).decode().strip()
                pr_info = subprocess.check_output(f"gh pr list --search '{commit_hash}' --state all --json number -q '.[0].number'", shell=True).decode().strip()
                if pr_info and pr_info != 'null':
                    subprocess.run(f"gh pr comment {pr_info} --body-file report.md", shell=True)
            elif event_name == 'pull_request':
                pr_number = os.environ.get('PR_NUMBER')
                if pr_number:
                    subprocess.run(f"gh pr comment {pr_number} --body-file report.md", shell=True)
        except Exception as e:
            print(f"Failed to comment on PR: {e}")

if __name__ == "__main__":
    main()
