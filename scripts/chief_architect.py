import os
import re
import sys
import subprocess
import json
import urllib.request
from collections import defaultdict

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout.strip()

def get_recent_commits(n=5):
    out = run_cmd(f"git log -n {n+1} --format=%H")
    return [c for c in out.split('\n') if c]

def get_file_content_at_commit(commit, filepath):
    if commit == 'CURRENT':
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                return f.read()
        except FileNotFoundError:
            return ""
    else:
        return run_cmd(f"git show {commit}:{filepath}")

def list_files_at_commit(commit):
    files = []
    if commit == 'CURRENT':
        for root, _, filenames in os.walk('.'):
            for filename in filenames:
                path = os.path.join(root, filename)
                if path.startswith('./'):
                    path = path[2:]
                if (path.startswith('mythrax-core/') and path.endswith('.rs')) or (path.startswith('scripts/') and path.endswith('.py')):
                    files.append(path)
    else:
        out = run_cmd(f"git ls-tree -r --name-only {commit}")
        for path in out.split('\n'):
            if (path.startswith('mythrax-core/') and path.endswith('.rs')) or (path.startswith('scripts/') and path.endswith('.py')):
                files.append(path)
    return files

def get_todo_md_content(commit):
    return get_file_content_at_commit(commit, "TODO.md").lower()

def analyze_file(content, filepath, todo_md_content, global_structs):
    debt_score = 0
    findings = []

    # 1. Dead code / Unused imports / Unreachable branches (suppressions)
    suppressions = re.findall(r'#\[allow\((dead_code|unused_imports|unreachable_code|unused_variables)\)\]', content)
    if suppressions:
        debt_score += len(suppressions) * 2
        findings.append(f"Contains {len(suppressions)} debt suppression annotations (e.g., #[allow(dead_code)]).")

    # 2. TODO, FIXME, HACK, TEMP comments
    todos = re.findall(r'(?i)//\s*(TODO|FIXME|HACK|TEMP)\b(.*?)$', content, re.MULTILINE)
    for tag, desc in todos:
        debt_score += 1
        desc_clean = desc.strip().lower()
        if desc_clean and desc_clean not in todo_md_content:
            debt_score += 2
            findings.append(f"Untracked {tag}: '{desc.strip()}' not found in TODO.md")

    # 3. Cyclomatic complexity
    if filepath.endswith('.rs'):
        func_pattern = re.compile(r'fn\s+([a-zA-Z0-9_]+)\s*\(')
        lines = content.split('\n')
        in_func = False
        brace_depth = 0
        func_name = ""
        complexity = 0
        for line in lines:
            if not in_func:
                m = func_pattern.search(line)
                if m:
                    in_func = True
                    func_name = m.group(1)
                    complexity = 1
                    # Account for impl blocks: base depth at function start
                    brace_depth = line.count('{') - line.count('}')
            else:
                brace_depth += line.count('{') - line.count('}')
                complexity += len(re.findall(r'\b(if|while|for|match|loop)\b', line))
                complexity += line.count('&&') + line.count('||') + line.count('?')
                if brace_depth <= 0:
                    in_func = False
                    if complexity > 15:
                        debt_score += (complexity - 15)
                        findings.append(f"Function '{func_name}' has excessive cyclomatic complexity ({complexity} > 15).")

    # 4. Inconsistent error handling
    if filepath.endswith('.rs'):
        has_unwrap = '.unwrap()' in content
        has_expect = '.expect(' in content
        has_qmark = '?' in content
        has_match_err = 'Err(' in content and 'match ' in content
        mix_score = sum([has_unwrap or has_expect, has_qmark, has_match_err])
        if mix_score > 1 and (has_unwrap or has_expect):
            debt_score += 3
            findings.append("Inconsistent error handling: mixing unwrap()/expect() with ? or match.")

    # 5. Struct/Enum duplicates
    if filepath.endswith('.rs'):
        structs = re.findall(r'struct\s+([A-Z][a-zA-Z0-9_]+)', content)
        enums = re.findall(r'enum\s+([A-Z][a-zA-Z0-9_]+)', content)
        for s in structs + enums:
            global_structs[s].append(filepath)

    # 6. Magic numbers / strings (simplified)
    if not ('test' in filepath or '#[cfg(test)]' in content):
        magic_numbers = re.findall(r'(?<![a-zA-Z0-9_])([1-9][0-9]{2,})(?![a-zA-Z0-9_])', content)
        if magic_numbers:
            findings.append(f"Potential magic numbers found: {list(set(magic_numbers))[:3]}...")
            debt_score += 1

    return debt_score, findings

def scan_commit(commit):
    files = list_files_at_commit(commit)
    todo_md_content = get_todo_md_content(commit)

    total_score = 0
    file_scores = {}
    global_structs = defaultdict(list)

    for filepath in files:
        content = get_file_content_at_commit(commit, filepath)
        score, findings = analyze_file(content, filepath, todo_md_content, global_structs)
        file_scores[filepath] = {
            'score': score,
            'findings': findings
        }
        total_score += score

    duplicate_score = 0
    for name, paths in global_structs.items():
        if len(set(paths)) > 1:
            duplicate_score += 5
            for p in set(paths):
                if p in file_scores:
                    file_scores[p]['score'] += 2
                    file_scores[p]['findings'].append(f"Duplicated struct/enum '{name}' found in multiple files.")

    total_score += duplicate_score
    return total_score, file_scores

def get_pr_number(commit_sha):
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        return None

    url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json"
    })

    try:
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            if data and len(data) > 0:
                return data[0]['number']
    except Exception as e:
        print(f"Error fetching PR number: {e}")
    return None

def post_pr_comment(pr_number, body):
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        return

    url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json"
    })
    comment_id = None
    try:
        with urllib.request.urlopen(req) as response:
            comments = json.loads(response.read().decode())
            for comment in comments:
                if "## Chief Architect Sanitation Scorecard" in comment.get("body", ""):
                    comment_id = comment["id"]
                    break
    except:
        pass

    if comment_id:
        url = f"https://api.github.com/repos/{repo}/issues/comments/{comment_id}"
        method = "PATCH"
    else:
        method = "POST"

    req = urllib.request.Request(url, data=json.dumps({"body": body}).encode('utf-8'), headers={
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }, method=method)
    try:
        with urllib.request.urlopen(req) as response:
            print("Comment posted to PR.")
    except Exception as e:
        print(f"Error posting comment: {e}")

def main():
    commits = get_recent_commits(5)
    current_commit = os.environ.get("GITHUB_SHA", "CURRENT")

    if current_commit != "CURRENT" and current_commit not in commits:
        commits.insert(0, current_commit)

    # In CI, CURRENT is the latest commit. We want to skip comparing CURRENT to commits[0] if they are the same.
    targets = ["CURRENT"]
    for c in commits:
        if current_commit != "CURRENT" and c == current_commit:
            continue # Skip adding the latest commit again, CURRENT already covers it
        targets.append(c)
    targets = targets[:6]

    scores = []
    file_histories = defaultdict(list)



    for c in targets:
        total_score, f_scores = scan_commit(c)
        scores.append((c, total_score))
        for f, data in f_scores.items():
            file_histories[f].append(data['score'])

    current_total = scores[0][1]
    report = []
    report.append("## Chief Architect Sanitation Scorecard\n")
    report.append(f"**Current Total Debt Score:** {current_total}\n")

    if len(scores) > 1:
        prev_total = scores[1][1]
        if current_total > prev_total:
            report.append(f"🚨 **Trajectory:** DEGRADING (Debt increased from {prev_total} to {current_total})\n")
        elif current_total < prev_total:
            report.append(f"✅ **Trajectory:** IMPROVING (Debt decreased from {prev_total} to {current_total})\n")
        else:
            report.append(f"➖ **Trajectory:** STABLE (Debt remains at {current_total})\n")

    report.append("### Degrading Files (Debt Density Increasing)")
    degrading_files = False
    for f, h in file_histories.items():
        if len(h) > 1 and h[0] > h[1]:
            degrading_files = True
            report.append(f"- **{f}**: Score increased from {h[1]} to {h[0]}")
    if not degrading_files:
        report.append("- No files are actively degrading.")

    report.append("\n### Key Findings in Current Commit")
    _, current_f_scores = scan_commit("CURRENT")
    for f, data in current_f_scores.items():
        if data['findings']:
            report.append(f"#### {f} (Score: {data['score']})")
            for finding in set(data['findings']):
                report.append(f"- {finding}")

    report_text = "\n".join(report)
    print(report_text)

    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "push" and current_commit != "CURRENT":
        pr_num = get_pr_number(current_commit)
        if pr_num:
            post_pr_comment(pr_num, report_text)

if __name__ == "__main__":
    main()
