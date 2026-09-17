import os
import sys
import subprocess
import re
import urllib.request
import json

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    return result.stdout.strip()

def get_todo_md_content():
    try:
        with open("TODO.md", "r") as f:
            return f.read().lower()
    except FileNotFoundError:
        return ""

def parse_rust_file(content):
    issues = {
        'complexity': [],
        'dead_code': 0,
        'orphan_todos': [],
        'error_mix': set(),
        'structs': [],
        'magic': 0
    }

    todo_md = get_todo_md_content()

    lines = content.split('\n')
    brace_depth = 0
    in_func = False
    func_name = ""
    func_complexity = 1
    func_base_depth = 0

    in_struct = False
    struct_name = ""
    struct_body = []
    struct_base_depth = 0

    for line in lines:
        stripped = line.strip()

        if "#[allow(dead_code)]" in stripped:
            issues['dead_code'] += 1

        todo_match = re.search(r'//.*\b(TODO|FIXME|HACK|TEMP)\b(.*)', stripped)
        if todo_match:
            todo_text = todo_match.group(2).strip().lower()
            if todo_text and todo_text not in todo_md:
                issues['orphan_todos'].append(todo_match.group(0))

        if ".unwrap()" in stripped: issues['error_mix'].add('unwrap')
        if ".expect(" in stripped: issues['error_mix'].add('expect')
        if "?" in stripped and not stripped.startswith("//"): issues['error_mix'].add('?')

        line_no_strings = re.sub(r'".*?(?<!\\)"', '', stripped)
        line_no_strings = re.sub(r'//.*', '', line_no_strings)

        for char in line_no_strings:
            if char == '{': brace_depth += 1
            elif char == '}': brace_depth -= 1

        if not in_func and not in_struct:
            fn_match = re.match(r'(?:pub\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)', line_no_strings)
            if fn_match:
                in_func = True
                func_name = fn_match.group(1)
                func_complexity = 1
                func_base_depth = brace_depth - line_no_strings.count('{')
                if func_base_depth < 0: func_base_depth = 0

            struct_match = re.match(r'(?:pub\s+)?(?:struct|enum)\s+([a-zA-Z0-9_]+)', line_no_strings)
            if struct_match:
                in_struct = True
                struct_name = struct_match.group(1)
                struct_body = []
                struct_base_depth = brace_depth - line_no_strings.count('{')
                if struct_base_depth < 0: struct_base_depth = 0

        if in_func:
            func_complexity += line_no_strings.count('if ') + line_no_strings.count('while ') + \
                               line_no_strings.count('for ') + line_no_strings.count('match ') + \
                               line_no_strings.count('&&') + line_no_strings.count('||') + \
                               line_no_strings.count('?')
            if brace_depth <= func_base_depth:
                if func_complexity > 15:
                    issues['complexity'].append((func_name, func_complexity))
                in_func = False

        if in_struct:
            struct_body.append(line_no_strings)
            if brace_depth <= struct_base_depth:
                issues['structs'].append((struct_name, "".join(struct_body)))
                in_struct = False

        if in_func and re.search(r'==\s*\d+', line_no_strings) and not re.search(r'==\s*0', line_no_strings):
            issues['magic'] += 1

    return issues

def analyze_commit(commit_hash, files_to_scan):
    results = {}
    for f in files_to_scan:
        if not f.endswith('.rs') and not f.endswith('.py') and not f.endswith('.sh'):
            continue
        try:
            content = run_cmd(f"git show {commit_hash}:{f}")
            if content:
                results[f] = parse_rust_file(content)
        except Exception:
            pass
    return results

def get_files_to_scan(event_name):
    if event_name in ['push', 'pull_request']:
        out = run_cmd("git diff --name-only HEAD^1 HEAD")
        files = [f for f in out.split('\n') if f]
        if 'ARCHITECTURE.md' not in files: files.append('ARCHITECTURE.md')
        if 'mock_audit_report.md' not in files: files.append('mock_audit_report.md')
        return files
    else:
        out = run_cmd("git ls-tree -r --name-only HEAD")
        return [f for f in out.split('\n') if f]

def calculate_debt_score(issues):
    score = 0
    score += issues['dead_code']
    score += len(issues['orphan_todos'])
    score += len(issues['complexity'])
    score += issues['magic']
    if len(issues['error_mix']) > 2:
        score += 1
    return score

def main():
    event_name = os.environ.get('GITHUB_EVENT_NAME', 'schedule')
    files = get_files_to_scan(event_name)

    commits = run_cmd("git log -n 6 --format=%H").split()
    if not commits:
        print("No commits found.")
        return

    history = []
    for c in commits:
        history.append(analyze_commit(c, files))

    if not history:
        return

    current = history[0]

    report = "### 🏛️ Chief Architect Sanitation Scorecard\n\n"

    degrading_files = []

    for f in files:
        if f not in current: continue
        current_score = calculate_debt_score(current[f])

        scores = []
        for h in history:
            if f in h:
                scores.append(calculate_debt_score(h[f]))
            else:
                scores.append(0)

        trajectory = " → ".join(map(str, reversed(scores)))

        if len(scores) > 1 and scores[0] > scores[1]:
            degrading_files.append((f, current_score, current[f]))
            report += f"- 🔴 **{f}**: Debt increased! Trajectory: {trajectory}\n"
        elif len(scores) > 1 and scores[0] < scores[1]:
            report += f"- 🟢 **{f}**: Debt decreased. Trajectory: {trajectory}\n"
        else:
            report += f"- ⚪ **{f}**: No change. Trajectory: {trajectory}\n"

    if degrading_files:
        report += "\n#### 🚨 Debt Details for Degrading Files\n"
        for f, score, issues in degrading_files:
            report += f"\n**{f}**:\n"
            if issues['dead_code'] > 0:
                report += f"- Dead code suppressions: {issues['dead_code']}\n"
            if issues['orphan_todos']:
                report += f"- Orphaned TODOs: {len(issues['orphan_todos'])}\n"
            if issues['complexity']:
                report += f"- High complexity functions: {', '.join(fn for fn, c in issues['complexity'])}\n"
            if len(issues['error_mix']) > 2:
                report += f"- Inconsistent error handling: mixed {', '.join(issues['error_mix'])}\n"
            if issues['magic'] > 0:
                report += f"- Magic numbers found: {issues['magic']}\n"

    print(report)

    if event_name == 'push':
        token = os.environ.get('GITHUB_TOKEN')
        repo = os.environ.get('GITHUB_REPOSITORY')
        sha = os.environ.get('GITHUB_SHA')
        if token and repo and sha:
            url = f"https://api.github.com/repos/{repo}/commits/{sha}/pulls"
            req = urllib.request.Request(url, headers={'Authorization': f'token {token}', 'Accept': 'application/vnd.github.v3+json'})
            try:
                with urllib.request.urlopen(req) as response:
                    prs = json.loads(response.read().decode())
                    if prs:
                        pr_num = prs[0]['number']

                        comments_url = f"https://api.github.com/repos/{repo}/issues/{pr_num}/comments"
                        req_comments = urllib.request.Request(comments_url, headers={'Authorization': f'token {token}', 'Accept': 'application/vnd.github.v3+json'})
                        with urllib.request.urlopen(req_comments) as c_res:
                            comments = json.loads(c_res.read().decode())
                            for c in comments:
                                if "Chief Architect Sanitation Scorecard" in c.get('body', ''):
                                    print("Comment already exists. Skipping.")
                                    return

                        post_req = urllib.request.Request(comments_url, data=json.dumps({'body': report}).encode(), headers={'Authorization': f'token {token}', 'Accept': 'application/vnd.github.v3+json', 'Content-Type': 'application/json'}, method='POST')
                        urllib.request.urlopen(post_req)
                        print("Posted PR comment.")
            except Exception as e:
                print(f"Error posting comment: {e}")

if __name__ == "__main__":
    main()
