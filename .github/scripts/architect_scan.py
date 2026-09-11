import os
import re
import subprocess
import json
import urllib.request
import urllib.error

def get_commits():
    # Get current commit and up to 5 previous commits
    res = subprocess.run(["git", "log", "-n", "6", "--format=%H"], capture_output=True, text=True)
    commits = res.stdout.strip().splitlines()
    return commits

def get_file_list(commit):
    res = subprocess.run(["git", "ls-tree", "-r", "--name-only", commit], capture_output=True, text=True)
    files = res.stdout.strip().splitlines()
    return [f for f in files if (f.endswith('.rs') or f.endswith('.py') or f.endswith('.sh')) and ('mythrax-core' in f or 'scripts' in f)]

def get_file_content(commit, filepath):
    res = subprocess.run(["git", "show", f"{commit}:{filepath}"], capture_output=True, text=True)
    return res.stdout

def get_todo_md(commit):
    res = subprocess.run(["git", "show", f"{commit}:TODO.md"], capture_output=True, text=True)
    return res.stdout if res.returncode == 0 else ""

def analyze_file(file_content, filename, todo_md_content, global_structs):
    debt = {
        'dead_code': 0,
        'orphaned_todos': 0,
        'complexity': 0,
        'error_inconsistency': 0,
        'duplicate_types': 0,
        'magic_numbers': 0
    }

    lines = file_content.splitlines()

    in_function = False
    func_brace_level = 0
    func_cc = 1

    in_struct = False
    struct_brace_level = 0
    struct_body = []

    err_unwrap = 0
    err_expect = 0
    err_question = 0
    err_match = 0

    todo_words = set(re.findall(r'\b\w{5,}\b', todo_md_content.lower()))

    for line in lines:
        line_stripped = line.strip()

        # Dead code
        if "#![allow(dead_code)]" in line_stripped or "#[allow(dead_code)]" in line_stripped or "allow(unused_imports)" in line_stripped or "allow(unreachable_code)" in line_stripped:
            debt['dead_code'] += 1

        # TODOs
        if "//" in line or "#" in line:
            m = re.search(r'(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', line, re.IGNORECASE)
            if m:
                todo_text = m.group(2).strip()
                if todo_text:
                    words = [w for w in re.split(r'\W+', todo_text.lower()) if len(w) > 4]
                    if words and not any(w in todo_words for w in words):
                        debt['orphaned_todos'] += 1
                else:
                    debt['orphaned_todos'] += 1

        # Error handling
        if "unwrap()" in line: err_unwrap += 1
        if "expect(" in line: err_expect += 1
        if "?" in line: err_question += 1
        if "match " in line: err_match += 1

        # Structs
        if not in_struct and not in_function:
            struct_match = re.search(r'(?:struct|enum)\s+(\w+)\s*\{', line)
            if struct_match:
                in_struct = True
                struct_brace_level = line[struct_match.end()-1:].count('{') - line[struct_match.end()-1:].count('}')
                struct_body = []
                continue
        if in_struct:
            struct_brace_level += line.count('{') - line.count('}')
            struct_body.append(line)
            if struct_brace_level <= 0:
                in_struct = False
                body_text = "\n".join(struct_body).strip()
                body_norm = re.sub(r'\s+', ' ', body_text)
                if body_norm in global_structs:
                    debt['duplicate_types'] += 1
                else:
                    global_structs.add(body_norm)
                continue

        # Complexity
        if not in_function and not in_struct:
            fn_match = re.search(r'fn\s+(\w+)', line)
            if fn_match and "{" in line:
                in_function = True
                func_brace_level = line[fn_match.end():].count('{') - line[fn_match.end():].count('}')
                func_cc = 1 + len(re.findall(r'\b(if|while|for|match)\b', line)) + line.count('&&') + line.count('||') + line.count('?')
                if func_brace_level <= 0:
                    in_function = False
                    if func_cc > 15: debt['complexity'] += 1
                continue
        if in_function:
            func_brace_level += line.count('{') - line.count('}')
            func_cc += len(re.findall(r'\b(if|while|for|match)\b', line)) + line.count('&&') + line.count('||') + line.count('?')
            if func_brace_level <= 0:
                in_function = False
                if func_cc > 15: debt['complexity'] += 1
                continue

        # Magic numbers
        if not line_stripped.startswith("const ") and not line_stripped.startswith("static ") and not "test" in filename:
            if re.search(r'\b(?:1[0-9]|[2-9][0-9]+)\b', line) and ("=" in line or "let " in line) and not "[" in line:
                 debt['magic_numbers'] += 1

    types_of_errors = sum([1 for x in [err_unwrap, err_expect, err_question, err_match] if x > 0])
    if types_of_errors >= 3:
        debt['error_inconsistency'] += 1

    return debt

def analyze_commit(commit):
    files = get_file_list(commit)
    todo_md = get_todo_md(commit)
    global_structs = set()
    commit_debt = {
        'dead_code': 0,
        'orphaned_todos': 0,
        'complexity': 0,
        'error_inconsistency': 0,
        'duplicate_types': 0,
        'magic_numbers': 0
    }
    file_debts = {}

    for f in files:
        content = get_file_content(commit, f)
        debt = analyze_file(content, f, todo_md, global_structs)
        total_file_debt = sum(debt.values())
        file_debts[f] = total_file_debt
        for k, v in debt.items():
            commit_debt[k] += v

    return commit_debt, file_debts

def find_pr_for_commit(token, repo, commit_sha):
    url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
    req = urllib.request.Request(url, headers={
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json"
    })
    try:
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            if data and len(data) > 0:
                return data[0]['number']
    except Exception as e:
        print(f"Error fetching PRs for commit: {e}")
    return None

def post_pr_comment(token, repo, pr_number, body):
    url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
    req = urllib.request.Request(url, method="POST", headers={
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }, data=json.dumps({"body": body}).encode("utf-8"))

    try:
        with urllib.request.urlopen(req) as response:
            print(f"Comment posted successfully to PR #{pr_number}")
    except Exception as e:
        print(f"Error posting comment to PR #{pr_number}: {e}")

def get_existing_comments(token, repo, pr_number):
    url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
    req = urllib.request.Request(url, headers={
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json"
    })
    try:
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            return [c['body'] for c in data]
    except Exception as e:
        print(f"Error fetching comments for PR #{pr_number}: {e}")
    return []

def main():
    commits = get_commits()
    results = []

    if not commits:
        print("No commits found.")
        return

    print(f"Found {len(commits)} commits to analyze.")
    for c in commits:
        c_debt, f_debt = analyze_commit(c)
        results.append({
            'commit': c,
            'debt': c_debt,
            'files': f_debt
        })

    # Generate scorecard
    current = results[0]
    older_commits = results[1:]

    scorecard = "## Chief Architect Sanitation Scorecard\n\n"

    scorecard += "| Metric | Current Commit | Avg Last 5 | Trajectory |\n"
    scorecard += "|---|---|---|---|\n"

    for metric in current['debt'].keys():
        curr_val = current['debt'][metric]
        if older_commits:
            avg_past = sum([c['debt'][metric] for c in older_commits]) / len(older_commits)
            diff = curr_val - avg_past
            if diff > 0:
                traj = f"🔴 Degrading (+{diff:.1f})"
            elif diff < 0:
                traj = f"🟢 Improving ({diff:.1f})"
            else:
                traj = "⚪ Stable"
        else:
            avg_past = 0
            traj = "N/A"

        scorecard += f"| `{metric}` | {curr_val} | {avg_past:.1f} | {traj} |\n"

    scorecard += "\n### Files with Increasing Debt Density\n"

    flagged_files = []
    for f, curr_debt in current['files'].items():
        if curr_debt > 0:
            past_debts = []
            for c in older_commits:
                if f in c['files']:
                    past_debts.append(c['files'][f])
            if past_debts:
                avg_past_file = sum(past_debts) / len(past_debts)
                if curr_debt > avg_past_file:
                    flagged_files.append((f, curr_debt, avg_past_file))

    if flagged_files:
        for f, c_d, a_d in flagged_files:
            scorecard += f"- `{f}`: {c_d} (was {a_d:.1f})\n"
    else:
        scorecard += "- No files showing increased debt density.\n"

    print(scorecard)

    # Check if we should post to a PR
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "push":
        token = os.environ.get("GITHUB_TOKEN")
        repo = os.environ.get("GITHUB_REPOSITORY")
        sha = os.environ.get("GITHUB_SHA")

        if token and repo and sha:
            pr_number = find_pr_for_commit(token, repo, sha)
            if pr_number:
                existing_comments = get_existing_comments(token, repo, pr_number)
                if any("## Chief Architect Sanitation Scorecard" in c for c in existing_comments):
                    print("Scorecard already posted on this PR.")
                else:
                    post_pr_comment(token, repo, pr_number, scorecard)
            else:
                print(f"No PR found for commit {sha}")

if __name__ == "__main__":
    main()
