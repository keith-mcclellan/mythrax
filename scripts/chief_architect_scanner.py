import os
import re
import sys
import subprocess
import urllib.request
import json
from collections import defaultdict

def get_commits():
    result = subprocess.run(["git", "rev-list", "--max-count=6", "HEAD"], capture_output=True, text=True)
    commits = result.stdout.strip().split("\n")
    return [c for c in commits if c]

def get_files_at_commit(commit):
    result = subprocess.run(["git", "ls-tree", "-r", "--name-only", commit], capture_output=True, text=True)
    files = result.stdout.strip().split("\n")
    return [f for f in files if (f.startswith("mythrax-core/") or f.startswith("scripts/")) and (f.endswith(".rs") or f.endswith(".py") or f.endswith(".sh"))]

def read_file_at_commit(commit, filepath):
    result = subprocess.run(["git", "show", f"{commit}:{filepath}"], capture_output=True, text=True, errors="replace")
    return result.stdout

def read_todo_md(commit):
    result = subprocess.run(["git", "show", f"{commit}:TODO.md"], capture_output=True, text=True, errors="replace")
    if result.returncode == 0:
        return result.stdout.lower()
    return ""

def analyze_file(filepath, content, todo_text):
    debt = 0
    issues = []
    error_patterns = set()
    structs = []

    lines = content.split('\n')

    in_struct = False
    struct_body = []
    struct_name = ""
    brace_depth = 0

    in_function = False
    func_name = ""
    func_complexity = 1
    func_brace_depth = 0

    for i, line in enumerate(lines):
        line_num = i + 1
        stripped = line.strip()

        # Dead code / suppression
        if re.search(r'#\[allow\((dead_code|unused_imports|unused_variables|unreachable_code)\)\]', stripped):
            issues.append(f"L{line_num}: Code debt suppression (`#[allow(...)]`)")
            debt += 1

        # Orphaned TODOs
        todo_match = re.search(r'(TODO|FIXME|HACK|TEMP)\b[:\s]*(.*)', stripped, re.IGNORECASE)
        if todo_match and not stripped.startswith('// TODO.md') and not stripped.startswith('// TODO:'):
            # We skip standard comment lines that might just be referencing TODO.md
            tag = todo_match.group(1).upper()
            desc = todo_match.group(2).strip().lower()
            if desc and len(desc) > 3 and desc not in todo_text:
                issues.append(f"L{line_num}: Orphaned {tag} missing from TODO.md")
                debt += 1

        # Error handling patterns
        if '.unwrap()' in stripped: error_patterns.add('unwrap()')
        if '.expect(' in stripped: error_patterns.add('expect()')
        if '?' in stripped and not stripped.startswith('//'): error_patterns.add('?')
        if 'match ' in stripped and 'Err' in stripped: error_patterns.add('match (Err)')

        # Struct / Enum deduplication
        if not in_struct:
            struct_match = re.match(r'^(?:pub\s+)?(?:struct|enum)\s+([A-Za-z0-9_]+)', stripped)
            if struct_match:
                in_struct = True
                struct_name = struct_match.group(1)
                struct_body = [stripped]
                brace_depth = stripped.count('{') - stripped.count('}')
                if brace_depth <= 0 and '{' in stripped:
                    in_struct = False
                    if len(struct_body) > 2:
                        structs.append((struct_name, "\n".join(struct_body)))
        else:
            struct_body.append(stripped)
            brace_depth += stripped.count('{') - stripped.count('}')
            if brace_depth <= 0:
                in_struct = False
                if len(struct_body) > 2:
                    structs.append((struct_name, "\n".join(struct_body)))

        # Cyclomatic complexity
        if not in_function:
            func_match = re.match(r'^(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)', stripped)
            if func_match:
                in_function = True
                func_name = func_match.group(1)
                func_complexity = 1
                func_brace_depth = stripped.count('{') - stripped.count('}')
                if func_brace_depth <= 0 and '{' in stripped:
                    in_function = False
        else:
            func_brace_depth += stripped.count('{') - stripped.count('}')
            tokens = re.split(r'\W+', stripped)
            func_complexity += sum(1 for t in tokens if t in ('if', 'while', 'for', 'loop', 'match'))
            func_complexity += stripped.count('&&') + stripped.count('||') + stripped.count('?')

            # Magic numbers > 10 in function logic (naive heuristic)
            if not stripped.startswith('//') and not 'const' in stripped:
                if re.search(r'(?<![A-Za-z0-9_])(1[1-9]|[2-9]\d|\d{3,})(?![A-Za-z0-9_])', stripped):
                    if re.search(r'\(\s*\d{2,}\s*\)', stripped) or re.search(r':\s*\d{2,}\s*[,}]', stripped) or re.search(r'\+\s*\d{2,}', stripped) or re.search(r'==\s*\d{2,}', stripped):
                        issues.append(f"L{line_num}: Magic number without named constant")
                        debt += 1

            if func_brace_depth <= 0:
                in_function = False
                if func_complexity > 15:
                    issues.append(f"Function `{func_name}` has cyclomatic complexity {func_complexity} (>15)")
                    debt += 3

    if len(error_patterns) > 2:
        issues.append(f"Inconsistent error handling mixing: {', '.join(error_patterns)}")
        debt += 2

    return debt, issues, structs

def run_analysis(commit):
    files = get_files_at_commit(commit)
    todo_text = read_todo_md(commit)

    total_debt = 0
    file_debts = {}
    all_structs = {}

    for filepath in files:
        content = read_file_at_commit(commit, filepath)
        debt, issues, structs = analyze_file(filepath, content, todo_text)
        if debt > 0:
            file_debts[filepath] = {"debt": debt, "issues": issues}
            total_debt += debt

        for name, body in structs:
            # normalize body to ignore whitespace differences
            normalized = re.sub(r'\s+', ' ', body).strip()
            # Ignore very small structs
            if len(normalized.split(' ')) < 5: continue
            if normalized not in all_structs:
                all_structs[normalized] = []
            all_structs[normalized].append((filepath, name))

    # Check for duplicated structs
    for body, occurrences in all_structs.items():
        if len(occurrences) > 1:
            names = [f"{o[0]}:{o[1]}" for o in occurrences]
            first_file = occurrences[0][0]
            if first_file not in file_debts:
                file_debts[first_file] = {"debt": 0, "issues": []}
            file_debts[first_file]["issues"].append(f"Duplicated structure found in: {', '.join(names)}")
            file_debts[first_file]["debt"] += 5
            total_debt += 5

    return {"total_debt": total_debt, "file_debts": file_debts}

def post_pr_comment(comment_body):
    github_token = os.environ.get("GITHUB_TOKEN")
    github_repo = os.environ.get("GITHUB_REPOSITORY")
    github_sha = os.environ.get("GITHUB_SHA")

    if not github_token or not github_repo or not github_sha:
        print("Missing GitHub environment variables (GITHUB_TOKEN, GITHUB_REPOSITORY, GITHUB_SHA). Skipping PR comment.")
        return

    url = f"https://api.github.com/repos/{github_repo}/commits/{github_sha}/pulls"
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {github_token}",
        "Accept": "application/vnd.github.v3+json",
        "X-GitHub-Api-Version": "2022-11-28"
    })

    try:
        with urllib.request.urlopen(req) as response:
            prs = json.loads(response.read().decode())
            if prs:
                pr_number = prs[0]["number"]
                comment_url = f"https://api.github.com/repos/{github_repo}/issues/{pr_number}/comments"
                comment_req = urllib.request.Request(comment_url, data=json.dumps({"body": comment_body}).encode("utf-8"), headers={
                    "Authorization": f"Bearer {github_token}",
                    "Accept": "application/vnd.github.v3+json",
                    "Content-Type": "application/json",
                    "X-GitHub-Api-Version": "2022-11-28"
                }, method="POST")
                with urllib.request.urlopen(comment_req) as comment_res:
                    print(f"Successfully posted comment to PR #{pr_number}")
            else:
                print(f"No PR found for commit {github_sha}")
    except urllib.error.URLError as e:
        print(f"Failed to query PRs or post comment: {e}")

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        return

    print(f"Analyzing {len(commits)} commits: {commits}")

    results = {}
    for c in commits:
        results[c] = run_analysis(c)

    current_commit = commits[0]
    current_result = results[current_commit]

    print(f"Current debt: {current_result['total_debt']}")

    scorecard = "## 🏗️ Chief Architect: Sanitation Scorecard\n\n"
    scorecard += "**Debt Trajectory (Last 6 Commits):**\n"
    for c in reversed(commits):
        scorecard += f"- `{c[:7]}`: {results[c]['total_debt']} debt units\n"

    scorecard += "\n**Files with Increasing Debt Density:**\n"
    increasing_debt_found = False

    if len(commits) > 1:
        prev_result = results[commits[1]]
        for filepath, data in current_result["file_debts"].items():
            prev_debt = prev_result["file_debts"].get(filepath, {}).get("debt", 0)
            if data["debt"] > prev_debt:
                increasing_debt_found = True
                scorecard += f"- `{filepath}`: {prev_debt} -> {data['debt']}\n"
                for issue in data["issues"]:
                    scorecard += f"  - {issue}\n"

    if not increasing_debt_found:
        scorecard += "✅ No files with increasing debt density found.\n"

    print(scorecard)

    # Only post on push event or explicit trigger if in CI
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "push":
        post_pr_comment(scorecard)
    else:
        print(f"Event is {event_name}, not posting PR comment.")

if __name__ == "__main__":
    main()
