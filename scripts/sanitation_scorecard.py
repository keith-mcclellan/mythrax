#!/usr/bin/env python3
import os
import re
import subprocess
import sys
import urllib.request
import json

def run_cmd(cmd):
    result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, shell=True)
    return result.stdout.strip()

def get_files():
    files = []
    for root, _, filenames in os.walk("mythrax-core"):
        for f in filenames:
            if f.endswith(".rs"):
                files.append(os.path.join(root, f))
    for root, _, filenames in os.walk("scripts"):
        for f in filenames:
            if f.endswith((".py", ".sh", ".rs")):
                files.append(os.path.join(root, f))
    return files

def get_todo_content():
    if os.path.exists("TODO.md"):
        with open("TODO.md", "r", encoding="utf-8") as f:
            return f.read().lower()
    return ""

def get_structs_enums(content):
    items = re.findall(r'(struct|enum)\s+(\w+)\s*\{([^}]+)\}', content)
    return [re.sub(r'\s+', '', body) for _, name, body in items]

def analyze_file(filepath, todo_content, all_structs):
    try:
        with open(filepath, "r", encoding="utf-8") as f:
            content = f.read()
    except Exception:
        return 0, []

    score = 0
    issues = []

    # Improved regex to catch multiple suppressions
    suppressions = len(re.findall(r'#\[allow\([^\]]*(?:dead_code|unused_imports|unreachable_code)[^\]]*\)\]', content))
    if suppressions > 0:
        score += suppressions * 5
        issues.append(f"{suppressions} debt suppressions")

    todos = re.findall(r'(?:#|//)\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', content, re.IGNORECASE)
    orphaned = 0
    for kind, text in todos:
        text = text.strip().lower()
        words = [w for w in re.split(r'\W+', text) if len(w) > 4]
        if not words or not any(w in todo_content for w in words):
            orphaned += 1
    if orphaned > 0:
        score += orphaned * 3
        issues.append(f"{orphaned} orphaned TODO/FIXME/HACK/TEMPs")

    has_unwrap = '.unwrap()' in content
    has_expect = '.expect(' in content
    has_question = '?' in content
    has_match = 'match ' in content
    types_of_errors = sum([1 for x in [has_unwrap, has_expect, has_question, has_match] if x])
    if types_of_errors > 1:
        score += 5
        issues.append("Inconsistent error handling (mixing unwrap/expect/?/match)")

    functions = re.split(r'fn\s+\w+\s*\(', content)
    high_cc = 0
    for func in functions[1:]:
        keywords = len(re.findall(r'\b(if|match|while|for|loop)\b', func)) + func.count('&&') + func.count('||') + func.count('?')
        if keywords > 15:
            high_cc += 1
    if high_cc > 0:
        score += high_cc * 10
        issues.append(f"{high_cc} functions with >15 cyclomatic complexity")

    file_structs = get_structs_enums(content)
    duplicates = 0
    for s in file_structs:
        if s in all_structs:
            duplicates += 1
        all_structs.add(s)
    if duplicates > 0:
        score += duplicates * 10
        issues.append(f"{duplicates} duplicated struct/enum definitions")

    magic_numbers = len(re.findall(r'(?:==|!=|>|<|>=|<=|\+|-|\*|/|=)\s*(?!0\b|1\b)(\d{2,})', content))
    if magic_numbers > 5:
        score += magic_numbers
        issues.append(f"{magic_numbers} potential magic numbers")

    magic_strings = len(re.findall(r'(?:==|!=)\s*"(?!\\?n|\\?t|\s*)[^"]+"', content))
    if magic_strings > 0:
        score += magic_strings
        issues.append(f"{magic_strings} potential magic string literals")

    return score, issues

def analyze_commit(commit_hash):
    run_cmd(f"git checkout -q {commit_hash}")
    files = get_files()
    todo_content = get_todo_content()
    all_structs = set()

    total_score = 0
    file_scores = {}

    for f in files:
        if os.path.exists(f):
            score, issues = analyze_file(f, todo_content, all_structs)
            total_score += score
            file_scores[f] = score

    return total_score, file_scores

def comment_on_pr(repo, pr_number, token, body):
    url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }
    data = {"body": body}
    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            return response.status == 201
    except Exception as e:
        print(f"Failed to comment on PR: {e}")
        return False

def main():
    run_cmd("git fetch --unshallow || git fetch --all")

    commits = run_cmd("git log -n 6 --format=%H").split()
    if len(commits) == 0:
        print("No commits found.")
        sys.exit(0)

    current_commit = commits[0]
    history = []

    original_branch = run_cmd("git rev-parse --abbrev-ref HEAD")
    if original_branch == "HEAD":
        original_branch = current_commit

    print("Analyzing commits...")
    for commit in reversed(commits):
        score, f_scores = analyze_commit(commit)
        history.append((commit, score, f_scores))

    run_cmd(f"git checkout -q {original_branch}")

    scorecard = "## Sanitation Scorecard\n\n"
    scorecard += "| Commit | Total Debt Score |\n"
    scorecard += "|--------|------------------|\n"
    for commit, score, _ in history:
        scorecard += f"| {commit[:7]} | {score} |\n"

    scorecard += "\n### Trajectory Analysis\n"
    if len(history) > 1:
        prev_score = history[-2][1]
        curr_score = history[-1][1]
        if curr_score > prev_score:
            scorecard += f"⚠️ **DEGRADING**: Technical debt increased from {prev_score} to {curr_score}.\n"
        elif curr_score < prev_score:
            scorecard += f"✅ **IMPROVING**: Technical debt decreased from {prev_score} to {curr_score}.\n"
        else:
            scorecard += f"⚖️ **STABLE**: Technical debt remained at {curr_score}.\n"

        prev_files = history[-2][2]
        curr_files = history[-1][2]
        increasing_files = []
        for f, s in curr_files.items():
            if s > prev_files.get(f, 0):
                increasing_files.append((f, prev_files.get(f, 0), s))

        if increasing_files:
            scorecard += "\n#### Files with Increasing Debt Density\n"
            for f, prev, curr in increasing_files:
                scorecard += f"- `{f}`: {prev} -> {curr}\n"

    print(scorecard)

    gh_token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if gh_token and repo and os.environ.get("GITHUB_EVENT_NAME") == "push":
        branch = os.environ.get("GITHUB_REF_NAME")
        if branch:
            pr_number = run_cmd(f"gh pr list --head {branch} --json number -q '.[0].number'")
            if pr_number and pr_number != 'null' and pr_number.isdigit():
                comment_on_pr(repo, pr_number, gh_token, scorecard)

if __name__ == "__main__":
    main()
