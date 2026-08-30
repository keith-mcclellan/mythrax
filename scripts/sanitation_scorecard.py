import os
import re
import subprocess
import urllib.request
import urllib.parse
import json
import sys

def load_system_prompt():
    try:
        with open('.agents/agents/chief_architect/agent.json', 'r') as f:
            config = json.load(f)
            sections = config.get('config', {}).get('customAgent', {}).get('systemPromptSections', [])
            prompt = "\n".join(sec.get('content', '') for sec in sections)
            return prompt
    except Exception as e:
        return f"Could not load system prompt: {e}"

def get_files_at_commit(commit):
    try:
        output = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', commit], text=True)
        return [f for f in output.splitlines() if (f.startswith('mythrax-core/') or f.startswith('scripts/')) and (f.endswith('.rs') or f.endswith('.py') or f.endswith('.sh'))]
    except subprocess.CalledProcessError:
        return []

def get_file_content_at_commit(commit, filepath):
    try:
        return subprocess.check_output(['git', 'show', f'{commit}:{filepath}'], text=True)
    except subprocess.CalledProcessError:
        return ""

def analyze_code(content, filename, todo_md_content):
    metrics = {
        'dead_code_suppressions': 0,
        'orphaned_todos': 0,
        'complex_functions': 0,
        'inconsistent_error_handling': 0,
        'duplicated_structs': [],
        'magic_numbers_strings': 0,
        'total_debt': 0
    }

    # 1. Dead code suppressions
    dead_code_matches = len(re.findall(r'#\[allow\((?:dead_code|unused_imports|unreachable_code)\)\]', content))
    metrics['dead_code_suppressions'] = dead_code_matches

    # 2. Orphaned TODOs
    todos = re.findall(r'(?:#|//)\s*(?:TODO|FIXME|HACK|TEMP):?(.*)', content, re.IGNORECASE)
    for todo in todos:
        todo_text = todo.strip()
        if todo_text and todo_text not in todo_md_content:
            metrics['orphaned_todos'] += 1

    # 3. Cyclomatic complexity
    in_function = False
    brace_level = 0
    current_fn_complexity = 1

    lines = content.splitlines()
    for line in lines:
        if re.search(r'\bfn\s+\w+', line) or re.search(r'\bdef\s+\w+', line):
            in_function = True
            brace_level = 0
            current_fn_complexity = 1

        if in_function:
            brace_level += line.count('{') - line.count('}')
            current_fn_complexity += len(re.findall(r'\b(if|else if|for|while|loop|match)\b', line))
            current_fn_complexity += line.count('?') + line.count('&&') + line.count('||')

            if brace_level <= 0 and '{' in content[content.find(line):] or '}' in line:
                if brace_level <= 0:
                    in_function = False
                    if current_fn_complexity > 15:
                        metrics['complex_functions'] += 1

        # 4. Inconsistent error handling
    if filename.endswith('.rs'):
        has_unwrap = '.unwrap()' in content
        has_expect = '.expect(' in content
        has_question = '?' in content
        has_match = 'match ' in content

        if (has_unwrap or has_expect) and (has_question or has_match):
            metrics['inconsistent_error_handling'] = 1

    # 5. Duplicated structs/enums
    structs = re.findall(r'(?:struct|enum)\s+(\w+)', content)
    metrics['duplicated_structs'] = structs

    # 6. Magic numbers/strings
    magic_nums = len(re.findall(r'(?:==|!=|>|<|>=|<=|\+|-|\*|/)\s*\b(?:[3-9]|\d{2,})\b', content))
    magic_strs = len(re.findall(r'(?:==|!=)\s*"[^"]+"', content))
    metrics['magic_numbers_strings'] = magic_nums + magic_strs

    metrics['total_debt'] = (metrics['dead_code_suppressions'] +
                             metrics['orphaned_todos'] +
                             metrics['complex_functions'] +
                             metrics['inconsistent_error_handling'] +
                             metrics['magic_numbers_strings'])
    return metrics

def run_analysis(commit):
    files = get_files_at_commit(commit)
    todo_md = get_file_content_at_commit(commit, 'TODO.md')

    total_metrics = {
        'dead_code_suppressions': 0,
        'orphaned_todos': 0,
        'complex_functions': 0,
        'inconsistent_error_handling': 0,
        'magic_numbers_strings': 0,
        'total_debt': 0
    }

    all_structs = []
    duplicated_structs_count = 0
    file_debts = {}

    for f in files:
        content = get_file_content_at_commit(commit, f)
        metrics = analyze_code(content, f, todo_md)

        for k in total_metrics:
            if k != 'duplicated_structs':
                total_metrics[k] += metrics[k]

        all_structs.extend(metrics['duplicated_structs'])
        file_debts[f] = metrics['total_debt']

    # Count duplicated structs
    struct_counts = {}
    for s in all_structs:
        struct_counts[s] = struct_counts.get(s, 0) + 1

    for s, count in struct_counts.items():
        if count > 1:
            duplicated_structs_count += 1
            total_metrics['total_debt'] += 1

    total_metrics['duplicated_structs'] = duplicated_structs_count

    return total_metrics, file_debts

def main():
    print(f"--- Persona Settings ---\n{load_system_prompt()}\n------------------------\n")

    subprocess.run(['git', 'fetch', '--unshallow'], stderr=subprocess.DEVNULL)
    subprocess.run(['git', 'fetch', '--all'], stderr=subprocess.DEVNULL)

    try:
        commits_output = subprocess.check_output(['git', 'log', '-n', '6', '--format=%H'], text=True)
        commits = commits_output.splitlines()
    except subprocess.CalledProcessError:
        commits = []

    if not commits:
        print("No commits found.")
        return

    current_commit = commits[0]
    history_commits = commits[1:]

    current_metrics, current_file_debts = run_analysis(current_commit)

    history_metrics = []
    for c in history_commits:
        m, fd = run_analysis(c)
        history_metrics.append((c, m, fd))

    report = "## Chief Architect Sanitation Scorecard\n\n"
    report += f"### Current Commit ({current_commit[:7]}) Debt Metrics:\n"
    report += f"- Dead code suppressions: {current_metrics['dead_code_suppressions']}\n"
    report += f"- Orphaned TODOs/FIXMEs: {current_metrics['orphaned_todos']}\n"
    report += f"- Complex functions (>15): {current_metrics['complex_functions']}\n"
    report += f"- Inconsistent error handling files: {current_metrics['inconsistent_error_handling']}\n"
    report += f"- Duplicated structs/enums: {current_metrics['duplicated_structs']}\n"
    report += f"- Magic numbers/strings: {current_metrics['magic_numbers_strings']}\n"
    report += f"**Total Debt Score:** {current_metrics['total_debt']}\n\n"

    report += "### Historical Trajectory (Last 5 Commits):\n"
    report += "| Commit | Total Debt | Trend |\n"
    report += "|--------|------------|-------|\n"
    report += f"| {current_commit[:7]} (Current) | {current_metrics['total_debt']} | - |\n"

    prev_debt = current_metrics['total_debt']
    for c, m, _ in history_metrics:
        trend = "📈 Degrading" if prev_debt > m['total_debt'] else ("📉 Improving" if prev_debt < m['total_debt'] else "➡️ Stable")
        report += f"| {c[:7]} | {m['total_debt']} | {trend} |\n"
        prev_debt = m['total_debt']

    report += "\n### Files with Increasing Debt Density:\n"
    increasing_files = []
    if history_metrics:
        last_commit, last_m, last_fd = history_metrics[0]
        for f, debt in current_file_debts.items():
            prev_file_debt = last_fd.get(f, 0)
            if debt > prev_file_debt:
                increasing_files.append(f"- `{f}`: {prev_file_debt} -> {debt} debt items")

    if increasing_files:
        report += "\n".join(increasing_files) + "\n"
    else:
        report += "No files with increasing debt density compared to the last commit. Good job!\n"

    print(report)

    gh_token = os.environ.get('GH_TOKEN')
    commit_sha = os.environ.get('GITHUB_SHA', current_commit)
    repo_name = os.environ.get('GITHUB_REPOSITORY')

    if gh_token and repo_name and commit_sha:
        try:
            url = f"https://api.github.com/repos/{repo_name}/commits/{commit_sha}/pulls"
            req = urllib.request.Request(url, headers={
                'Authorization': f'token {gh_token}',
                'Accept': 'application/vnd.github.groot-preview+json'
            })

            with urllib.request.urlopen(req) as response:
                prs = json.loads(response.read().decode())

            for pr in prs:
                pr_num = pr['number']
                comment_url = f"https://api.github.com/repos/{repo_name}/issues/{pr_num}/comments"
                data = json.dumps({'body': report}).encode('utf-8')
                comment_req = urllib.request.Request(comment_url, data=data, headers={
                    'Authorization': f'token {gh_token}',
                    'Accept': 'application/vnd.github.v3+json',
                    'Content-Type': 'application/json'
                })
                urllib.request.urlopen(comment_req)
                print(f"Successfully posted report to PR #{pr_num}")

        except Exception as e:
            print(f"Could not post to GitHub PR: {e}")

if __name__ == '__main__':
    main()
