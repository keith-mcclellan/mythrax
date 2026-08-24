#!/usr/bin/env python3
import os
import re
import subprocess

def run_cmd(cmd):
    try:
        return subprocess.check_output(cmd, shell=True).decode().strip()
    except Exception:
        return ""

def get_git_commits(limit=6):
    os.system("git fetch --unshallow >/dev/null 2>&1 || git fetch --all >/dev/null 2>&1")
    output = run_cmd(f"git log --format=%H -n{limit}")
    return [c for c in output.split('\n') if c]

def get_todo_content():
    if os.path.exists("TODO.md"):
        with open("TODO.md", "r", encoding="utf-8") as f:
            return f.read().lower()
    return ""

def analyze_codebase():
    todo_content = get_todo_content()

    stats = {
        "suppressions": 0,
        "orphaned_todos": 0,
        "complex_functions": 0,
        "inconsistent_error_handling": 0,
        "magic_numbers": 0,
        "duplicated_structs": 0,
        "total_debt": 0,
        "debt_by_file": {}
    }

    struct_names = []

    for root, dirs, files in os.walk("."):
        if ".git" in root or "target" in root or "evals" in root:
            continue
        for file in files:
            if not file.endswith((".rs", ".py", ".sh")):
                continue

            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8") as f:
                    content = f.read()
            except:
                continue

            file_debt = 0

            # Suppressions
            suppressions = len(re.findall(r'#\[allow\((?:dead_code|unused_imports|unused_variables|unreachable_code)\)\]', content))
            stats["suppressions"] += suppressions
            file_debt += suppressions

            # Orphaned TODOs
            # Account for regex heuristics for cross-language scripts using (?:#|//)
            todos = re.findall(r'(?:#|//)\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE)
            for t in todos:
                desc = t[1].strip().lower()
                if not desc:
                    stats["orphaned_todos"] += 1
                    file_debt += 1
                    continue

                found = False
                for line in todo_content.split('\n'):
                    if desc in line or (len(desc) > 10 and desc[:10] in line):
                        found = True
                        break
                if not found:
                    stats["orphaned_todos"] += 1
                    file_debt += 1

            # Inconsistent error handling
            unwraps = content.count(".unwrap()")
            expects = content.count(".expect(")
            questions = content.count("?")
            matches = content.count("match ")

            error_patterns = (1 if unwraps > 0 else 0) + (1 if expects > 0 else 0) + (1 if questions > 0 else 0) + (1 if matches > 0 else 0)
            if error_patterns > 1:
                stats["inconsistent_error_handling"] += (unwraps + expects)
                file_debt += (unwraps + expects)

            # Magic numbers and strings
            magics = len(re.findall(r'\blet\s+(?:mut\s+)?\w+\s*(?::\s*\w+\s*)?=\s*(?:\d+|"[^"]+");', content))
            stats["magic_numbers"] += magics
            file_debt += magics

            if file.endswith(".rs"):
                structs = re.findall(r'(?:struct|enum)\s+([A-Z]\w+)', content)
                struct_names.extend(structs)

                # Complex functions
                # Avoid naive heuristics like assuming any semicolon preceding an opening brace indicates a trait
                idx = 0
                while True:
                    match = re.search(r'fn\s+\w+', content[idx:])
                    if not match:
                        break
                    start = idx + match.start()
                    idx = start + match.end()

                    brace_match = re.search(r'\{', content[idx:])
                    if not brace_match:
                        break

                    body_start = idx + brace_match.end()
                    brace_depth = 1
                    body_end = body_start

                    for i in range(body_start, len(content)):
                        if content[i] == '{':
                            brace_depth += 1
                        elif content[i] == '}':
                            brace_depth -= 1
                            if brace_depth == 0:
                                body_end = i
                                break

                    body = content[body_start:body_end]
                    complexity = 1
                    complexity += len(re.findall(r'\bif\b', body))
                    complexity += len(re.findall(r'\bfor\b', body))
                    complexity += len(re.findall(r'\bwhile\b', body))
                    complexity += len(re.findall(r'\bmatch\b', body))
                    complexity += len(re.findall(r'\bloop\b', body))
                    complexity += body.count("?")
                    complexity += body.count("&&")
                    complexity += body.count("||")

                    if complexity > 15:
                        stats["complex_functions"] += 1
                        file_debt += (complexity - 15)

                    idx = body_end

            stats["debt_by_file"][filepath] = file_debt
            stats["total_debt"] += file_debt

    struct_counts = {}
    for s in struct_names:
        struct_counts[s] = struct_counts.get(s, 0) + 1
    dupes = sum(1 for k, v in struct_counts.items() if v > 1)
    stats["duplicated_structs"] = dupes
    stats["total_debt"] += dupes

    return stats

def main():
    commits = get_git_commits(6)
    if not commits:
        print("No commits found.")
        return

    original_commit = run_cmd("git rev-parse HEAD")

    history_stats = []
    for commit in reversed(commits):
        os.system(f"git checkout {commit} >/dev/null 2>&1")
        stats = analyze_codebase()
        history_stats.append((commit, stats))

    os.system(f"git checkout {original_commit} >/dev/null 2>&1")

    current_stats = history_stats[-1][1]

    scorecard = "## 🏗️ Architecture Sanitation Scorecard\n\n"

    scorecard += "### 📉 Debt Trajectory (Last 5 Commits -> Current)\n"
    scorecard += "| Commit | Total Debt | Suppressions | Orphaned TODOs | Complex Fns | Inconsistent Err | Magic Nums/Strs | Duplicate Structs |\n"
    scorecard += "|---|---|---|---|---|---|---|---|\n"
    for commit, stats in history_stats:
        short = commit[:7]
        scorecard += f"| {short} | {stats['total_debt']} | {stats['suppressions']} | {stats['orphaned_todos']} | {stats['complex_functions']} | {stats['inconsistent_error_handling']} | {stats['magic_numbers']} | {stats['duplicated_structs']} |\n"

    scorecard += "\n### 🚨 Increasing Debt by File\n"
    prev_stats = history_stats[-2][1] if len(history_stats) > 1 else current_stats
    increasing_files = []
    for filepath, debt in current_stats["debt_by_file"].items():
        prev_debt = prev_stats["debt_by_file"].get(filepath, 0)
        if debt > prev_debt:
            increasing_files.append((filepath, prev_debt, debt))

    if increasing_files:
        for filepath, prev_debt, debt in increasing_files:
            scorecard += f"- **{filepath}**: Debt increased from {prev_debt} to {debt}\n"
    else:
        scorecard += "- No files with increasing debt! 🎉\n"

    print(scorecard)

    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name in ["push", "pull_request"]:
        pr_number = os.environ.get("PR_NUMBER")
        if not pr_number:
            try:
                branch = run_cmd("git rev-parse --abbrev-ref HEAD")
                pr_info = run_cmd(f"gh pr view {branch} --json number --jq .number")
                if pr_info.isdigit():
                    pr_number = pr_info
            except:
                pass

        if pr_number:
            print(f"Posting comment to PR #{pr_number}")
            subprocess.run(["gh", "pr", "comment", pr_number, "-b", scorecard])
        else:
            print("Could not determine PR number to post comment.")

if __name__ == "__main__":
    main()
