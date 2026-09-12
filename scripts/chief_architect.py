import os
import sys
import subprocess
import re
import requests
import hashlib

COMMENT_RE = re.compile(r'//\s*(TODO|FIXME|HACK|TEMP):?\s*(.*)', re.IGNORECASE)
PY_COMMENT_RE = re.compile(r'#\s*(TODO|FIXME|HACK|TEMP):?\s*(.*)', re.IGNORECASE)
STRUCT_ENUM_START = re.compile(r'^\s*(?:pub\s+)?(?:struct|enum)\s+\w+\s*\{')
FUNC_START = re.compile(r'^\s*(?:pub\s+)?(?:async\s+)?fn\s+\w+')
PY_FUNC_START = re.compile(r'^\s*(?:async\s+)?def\s+\w+')
COMPLEXITY_KEYWORDS = re.compile(r'\b(if|for|while|match)\b|&&|\|\||\?')
PY_COMPLEXITY_KEYWORDS = re.compile(r'\b(if|for|while|match|and|or)\b')
MAGIC_STRING = re.compile(r'"([^"]{4,})"')

def run_cmd(cmd_list):
    result = subprocess.run(cmd_list, capture_output=True, text=True)
    return result.stdout.strip()

def get_file_content_at_commit(commit, filepath):
    return run_cmd(["git", "show", f"{commit}:{filepath}"])

def normalize_body(body):
    return re.sub(r'\s+', '', body)

def analyze_file(content, filepath, tracked_todos):
    debt_score = 0
    findings = []

    is_rust = filepath.endswith('.rs')
    is_py = filepath.endswith('.py')

    if is_rust:
        suppressions = content.count('#[allow(dead_code)]')
        suppressions += content.count('#[allow(unused_imports)]')
        suppressions += content.count('#[allow(unreachable_code)]')
        if suppressions > 0:
            findings.append(f"Found {suppressions} debt suppressions (dead_code/unused_imports)")
            debt_score += suppressions * 5

    in_struct_enum = False
    struct_start_depth = -1
    struct_body = []

    in_func = False
    func_complexity = 0
    func_name = ""
    func_start_depth = -1

    brace_depth = 0
    py_indent_level = -1

    struct_bodies = []

    lines = content.split('\n')
    for line_idx, line in enumerate(lines):
        line_num = line_idx + 1
        stripped = line.strip()

        if not stripped:
            continue

        match = COMMENT_RE.search(line) if is_rust else PY_COMMENT_RE.search(line)
        if match:
            tag = match.group(1).upper()
            desc = match.group(2).strip().lower()
            if len(desc) > 3 and desc not in tracked_todos:
                findings.append(f"Line {line_num}: Orphaned {tag} comment '{desc}' not found in TODO.md")
                debt_score += 3

        open_braces = line.count('{')
        close_braces = line.count('}')
        brace_depth += open_braces - close_braces

        current_indent = len(line) - len(line.lstrip())

        if is_rust:
            if FUNC_START.search(line) and not in_struct_enum and not in_func:
                in_func = True
                func_name = stripped.split('fn ')[-1].split('(')[0]
                func_complexity = 1
                # Function block typically starts on this line or next line.
                # If we have an open brace on this line, start depth is the depth *before* it opened
                func_start_depth = brace_depth - open_braces + close_braces

            elif in_func:
                matches = COMPLEXITY_KEYWORDS.findall(line)
                func_complexity += len(matches)

                if brace_depth <= func_start_depth and close_braces > 0:
                    in_func = False
                    if func_complexity > 15:
                        findings.append(f"Function {func_name} has excessive cyclomatic complexity ({func_complexity} > 15)")
                        debt_score += (func_complexity - 15) * 2

            if STRUCT_ENUM_START.search(line) and not in_func and not in_struct_enum:
                in_struct_enum = True
                struct_body = [stripped]
                struct_start_depth = brace_depth - open_braces + close_braces

            elif in_struct_enum:
                struct_body.append(stripped)
                if brace_depth <= struct_start_depth and close_braces > 0:
                    in_struct_enum = False
                    normalized = normalize_body("".join(struct_body))
                    struct_bodies.append((line_num, normalized))

        elif is_py:
            if PY_FUNC_START.search(line):
                if in_func and py_indent_level != -1 and current_indent <= py_indent_level:
                    if func_complexity > 15:
                        findings.append(f"Function {func_name} has excessive cyclomatic complexity ({func_complexity} > 15)")
                        debt_score += (func_complexity - 15) * 2
                in_func = True
                func_name = stripped.split('def ')[-1].split('(')[0]
                func_complexity = 1
                py_indent_level = current_indent

            if in_func and py_indent_level != -1:
                if current_indent <= py_indent_level and not PY_FUNC_START.search(line):
                    in_func = False
                    if func_complexity > 15:
                        findings.append(f"Function {func_name} has excessive cyclomatic complexity ({func_complexity} > 15)")
                        debt_score += (func_complexity - 15) * 2
                else:
                    matches = PY_COMPLEXITY_KEYWORDS.findall(line)
                    func_complexity += len(matches)

        if not stripped.startswith('const ') and not stripped.startswith('static ') and not in_struct_enum and not stripped.startswith('def ') and not stripped.startswith('class '):
            if MAGIC_STRING.search(line):
                debt_score += 1

    if is_py and in_func:
        if func_complexity > 15:
            findings.append(f"Function {func_name} has excessive cyclomatic complexity ({func_complexity} > 15)")
            debt_score += (func_complexity - 15) * 2

    if is_rust:
        has_unwrap = 'unwrap()' in content or 'expect(' in content
        has_q = '?' in content
        has_match = 'match ' in content
        if has_unwrap and (has_q or has_match):
            findings.append("Inconsistent error handling: mixes unwrap()/expect() with ? or match")
            debt_score += 10

        seen_structs = {}
        for lnum, body in struct_bodies:
            h = hashlib.md5(body.encode('utf-8')).hexdigest()
            if h in seen_structs:
                findings.append(f"Line {lnum}: Struct/Enum definition is duplicated structurally")
                debt_score += 20
            else:
                seen_structs[h] = True

    return debt_score, findings

def get_commits():
    commits_str = run_cmd(["git", "log", "-n", "6", "--format=%H"])
    return [c for c in commits_str.split('\n') if c]

def analyze_commit(commit):
    files_str = run_cmd(["git", "ls-tree", "-r", "--name-only", commit])
    files = []
    for f in files_str.split('\n'):
        if f.startswith('mythrax-core/') and f.endswith('.rs'):
            files.append(f)
        elif f.startswith('scripts/') and f.endswith(('.py', '.rs', '.sh')):
            files.append(f)

    tracked_todos = ""
    todo_content = run_cmd(["git", "show", f"{commit}:TODO.md"])
    if not todo_content.startswith("fatal:"):
        tracked_todos = todo_content.lower()

    total_score = 0
    file_scores = {}
    file_findings = {}

    for f in files:
        content = get_file_content_at_commit(commit, f)
        if content and not content.startswith("fatal:"):
            score, findings = analyze_file(content, f, tracked_todos)
            total_score += score
            file_scores[f] = score
            if findings:
                file_findings[f] = findings

    return total_score, file_scores, file_findings

def post_pr_comment(body):
    github_repository = os.environ.get('GITHUB_REPOSITORY')
    github_sha = os.environ.get('GITHUB_SHA')
    github_token = os.environ.get('GITHUB_TOKEN')

    if not github_repository or not github_sha or not github_token:
        print("Missing GitHub environment variables for posting PR comment")
        return

    url = f"https://api.github.com/repos/{github_repository}/commits/{github_sha}/pulls"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {github_token}",
        "X-GitHub-Api-Version": "2022-11-28"
    }

    resp = requests.get(url, headers=headers)
    if resp.status_code != 200:
        print(f"Failed to fetch PRs: {resp.text}")
        return

    prs = resp.json()
    if not prs:
        print("No PR found for this commit")
        return

    pr_number = prs[0]['number']

    comments_url = f"https://api.github.com/repos/{github_repository}/issues/{pr_number}/comments"
    resp = requests.get(comments_url, headers=headers)
    if resp.status_code == 200:
        comments = resp.json()
        for c in comments:
            if "## Chief Architect Sanitation Scorecard" in c.get('body', ''):
                print("Comment already exists, not posting again to avoid spam.")
                return

    payload = {"body": body}
    resp = requests.post(comments_url, headers=headers, json=payload)
    if resp.status_code == 201:
        print("Successfully posted PR comment")
    else:
        print(f"Failed to post PR comment: {resp.text}")

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        sys.exit(1)

    print(f"Analyzing current commit {commits[0]} and previous {len(commits)-1} commits...")

    history = []
    for c in reversed(commits):
        if c:
            score, f_scores, f_findings = analyze_commit(c)
            history.append({
                "commit": c,
                "total_score": score,
                "file_scores": f_scores,
                "file_findings": f_findings
            })

    current = history[-1]
    prev = history[-2] if len(history) > 1 else current

    scorecard = "## Chief Architect Sanitation Scorecard\n\n"
    scorecard += f"**Current Debt Score:** {current['total_score']} "

    diff = current['total_score'] - prev['total_score']
    if diff > 0:
        scorecard += f"(📈 +{diff} - Degrading)\n"
    elif diff < 0:
        scorecard += f"(📉 {diff} - Improving)\n"
    else:
        scorecard += "(➖ No change)\n"

    scorecard += "\n### Trajectory (Last 5 Commits)\n"
    for h in history:
        scorecard += f"- `{h['commit'][:7]}`: Score {h['total_score']}\n"

    increasing_files = []
    for f, score in current['file_scores'].items():
        prev_score = prev['file_scores'].get(f, 0)
        if score > prev_score:
            increasing_files.append((f, score - prev_score))

    if increasing_files:
        scorecard += "\n### 🚩 Files with Increasing Debt\n"
        for f, increase in increasing_files:
            scorecard += f"- `{f}`: +{increase} debt\n"

    scorecard += "\n### Current Findings\n"
    for f, findings in current['file_findings'].items():
        if findings:
            scorecard += f"**`{f}`**\n"
            for finding in findings:
                scorecard += f"- {finding}\n"
            scorecard += "\n"

    print(scorecard)

    if os.environ.get('GITHUB_EVENT_NAME') == 'push':
        post_pr_comment(scorecard)

if __name__ == "__main__":
    main()
