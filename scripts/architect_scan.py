import os
import re
import subprocess
import json
import sys

def get_git_commits(n=6): # 5 previous + current = 6
    result = subprocess.run(["git", "log", f"-n{n}", "--format=%H"], capture_output=True, text=True)
    return result.stdout.strip().split("\n")

def read_todo_md():
    try:
        with open("TODO.md", "r") as f:
            return f.read().lower()
    except FileNotFoundError:
        return ""

def scan_file(filepath, todo_text):
    debt_score = 0
    issues = []

    try:
        with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read()
    except Exception:
        return 0, [], []

    # 1. Dead code / unused imports / unreachable branches
    dead_code_allows = len(re.findall(r'#!?\[allow\(dead_code\)\]', content))
    unused_imports = len(re.findall(r'#!?\[allow\(unused_imports\)\]', content))
    unreachable = len(re.findall(r'#!?\[allow\(unreachable_code\)\]', content))

    debt_score += dead_code_allows * 5
    debt_score += unused_imports * 5
    debt_score += unreachable * 5
    if dead_code_allows > 0: issues.append(f"{dead_code_allows} dead_code allows")
    if unused_imports > 0: issues.append(f"{unused_imports} unused_imports allows")
    if unreachable > 0: issues.append(f"{unreachable} unreachable_code allows")

    # 2. TODO, FIXME, HACK, TEMP
    comments = re.findall(r'//\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', content, re.IGNORECASE)
    orphaned_todos = 0
    for tag, desc in comments:
        if desc.strip() and desc.strip().lower() not in todo_text:
            orphaned_todos += 1
            issues.append(f"Orphaned {tag}: {desc.strip()[:30]}")
    debt_score += orphaned_todos * 2

    # 3. Cyclomatic complexity
    func_pattern = re.compile(r'fn\s+\w+(?:<[^>]*>)?\s*\([^)]*\)\s*(?:->\s*[^\{]+)?\{')
    func_matches = func_pattern.finditer(content)
    high_complexity = 0
    for match in func_matches:
        start_idx = match.end()
        brace_count = 1
        end_idx = start_idx
        while brace_count > 0 and end_idx < len(content):
            if content[end_idx] == '{': brace_count += 1
            elif content[end_idx] == '}': brace_count -= 1
            end_idx += 1

        func_body = content[start_idx:end_idx]
        complexity = 1 + len(re.findall(r'\b(if|match|for|while|loop)\b', func_body)) + func_body.count('?') + func_body.count('&&') + func_body.count('||')
        if complexity > 15:
            high_complexity += 1
            issues.append(f"High complexity ({complexity}) in function")

    debt_score += high_complexity * 10

    # 4. Inconsistent error handling
    unwraps = content.count('.unwrap()')
    expects = content.count('.expect(')
    questions = content.count('?')
    matches = len(re.findall(r'\bmatch\s+', content))

    if (unwraps > 0 or expects > 0) and (questions > 0 or matches > 0):
        debt_score += 5
        issues.append(f"Inconsistent error handling: unwraps({unwraps}), expects({expects}), ?({questions}), match({matches})")

    # 5. Duplicated structs / enums
    structs = re.findall(r'(?:struct|enum)\s+(\w+)', content)

    # 6. Magic numbers / strings
    magic_numbers = len(re.findall(r'(?:==|!=|let\s+\w+\s*=)\s*(\d{2,})', content))
    debt_score += magic_numbers
    if magic_numbers > 0: issues.append(f"{magic_numbers} magic numbers")

    return debt_score, issues, structs

def scan_directory():
    todo_text = read_todo_md()
    file_stats = {}
    all_structs = []

    for root_dir in ['mythrax-core/src', 'scripts']:
        if not os.path.exists(root_dir): continue
        for root, dirs, files in os.walk(root_dir):
            dirs[:] = [d for d in dirs if d not in ['target', '.git', '.venv']]
            for file in files:
                if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                    filepath = os.path.join(root, file)
                    score, issues, structs = scan_file(filepath, todo_text)
                    file_stats[filepath] = {'score': score, 'issues': issues}
                    all_structs.extend(structs)

    from collections import Counter
    struct_counts = Counter(all_structs)
    duplicates = [name for name, count in struct_counts.items() if count > 1]

    return file_stats, duplicates

def main():
    current_commit = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()

    commits = get_git_commits(6)
    if not commits:
        print("No commits found.")
        return

    commits.reverse() # oldest to newest

    history = {}

    for commit in commits:
        subprocess.run(["git", "checkout", "-f", commit], check=True, capture_output=True)
        if os.path.exists("clippy.toml"):
            os.remove("clippy.toml")

        stats, duplicates = scan_directory()
        history[commit] = stats

    subprocess.run(["git", "checkout", "-f", current_commit], check=True, capture_output=True)

    latest_commit = commits[-1]
    oldest_commit = commits[0]

    latest_stats = history[latest_commit]

    report = ["# 🏗️ Chief Architect Sanitation Scorecard\n"]
    report.append(f"**Trajectory over last {len(commits)} commits:**\n")

    increasing_debt_files = []

    for filepath, data in latest_stats.items():
        latest_score = data['score']

        oldest_score = 0
        for c in commits:
            if filepath in history[c]:
                oldest_score = history[c][filepath]['score']
                break

        if latest_score > oldest_score:
            increasing_debt_files.append((filepath, oldest_score, latest_score, data['issues']))

    if increasing_debt_files:
        report.append("### 🚨 Files with Increasing Debt Density\n")
        for filepath, old_s, new_s, issues in increasing_debt_files:
            report.append(f"- **{filepath}** (Score: {old_s} ➡️ {new_s})")
            for issue in issues[:3]:
                report.append(f"  - {issue}")
    else:
        report.append("### ✅ No files with increasing debt density.\n")

    _, duplicates = scan_directory()
    if duplicates:
        report.append("### 👯 Duplicated Structs/Enums (Potential Unification)\n")
        report.append(f"- {', '.join(set(duplicates))[:200]}\n")

    report_text = "\n".join(report)
    print(report_text)

    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name in ["push", "pull_request"]:
        pr_number = None
        if event_name == "pull_request":
            pr_number = os.environ.get("GITHUB_EVENT_NUMBER")
            if not pr_number and "refs/pull/" in os.environ.get("GITHUB_REF", ""):
                pr_number = os.environ.get("GITHUB_REF").split("/")[2]
        else:
            branch = os.environ.get("GITHUB_REF_NAME")
            if branch:
                pr_check = subprocess.run(["gh", "pr", "list", "--head", branch, "--json", "number"], capture_output=True, text=True)
                if pr_check.returncode == 0 and pr_check.stdout.strip():
                    prs = json.loads(pr_check.stdout)
                    if prs:
                        pr_number = prs[0]['number']

        if pr_number:
            with open("report.md", "w") as f:
                f.write(report_text)
            subprocess.run(["gh", "pr", "comment", str(pr_number), "-F", "report.md"])
            print(f"Commented on PR #{pr_number}")
        else:
            print("No associated PR found to comment on.")
            with open("report.md", "w") as f:
                f.write(report_text)

if __name__ == "__main__":
    main()
