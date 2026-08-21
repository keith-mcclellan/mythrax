#!/usr/bin/env python3

import os
import re
import subprocess
import json

def get_git_commits(n=6):
    res = subprocess.run(["git", "log", f"-n{n}", "--format=%H", "HEAD"], capture_output=True, text=True)
    commits = res.stdout.strip().split("\n")
    return [c for c in commits if c]

def list_files(commit):
    files = []
    # Use git ls-tree to list files at a specific commit
    res = subprocess.run(["git", "ls-tree", "-r", "--name-only", commit], capture_output=True, text=True)
    for path in res.stdout.strip().split("\n"):
        if path.endswith(".rs") or path.endswith(".py") or path.endswith(".sh"):
            files.append(path)
    return files

def get_file_content(commit, path):
    res = subprocess.run(["git", "show", f"{commit}:{path}"], capture_output=True, text=True, errors="replace")
    if res.returncode != 0:
        return ""
    return res.stdout

def get_tracked_todos(commit):
    res = subprocess.run(["git", "show", f"{commit}:TODO.md"], capture_output=True, text=True, errors="replace")
    if res.returncode != 0:
        return ""
    content = res.stdout
    return content.lower()

def analyze_file(content, path, tracked_todos_text):
    # Strip string literals and block comments from content for cleaner parsing
    # This prevents counting {} inside strings
    clean_content = re.sub(r'".*?(?<!\\)"', '""', content, flags=re.DOTALL)
    clean_content = re.sub(r'//.*', '', clean_content)
    clean_content = re.sub(r'/\*.*?\*/', '', clean_content, flags=re.DOTALL)

    dead_code = len(re.findall(r'#\[allow\(dead_code\)\]', content))
    unused_imports = len(re.findall(r'#\[allow\(unused_imports\)\]', content))
    unreachable_code = len(re.findall(r'#\[allow\(unreachable_code\)\]', content))

    total_suppressions = dead_code + unused_imports + unreachable_code

    # Extract comments from ORIGINAL content
    debt_comments = re.findall(r'(?:#|//)\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', content, re.IGNORECASE)

    orphaned_debt = []
    for c_type, text in debt_comments:
        text = text.strip().lower()
        if text and text not in tracked_todos_text:
            orphaned_debt.append((c_type, text))
        elif not text:
            orphaned_debt.append((c_type, "Empty comment"))

    # Struct / enum deduplication check
    structs = re.findall(r'\bstruct\s+([a-zA-Z0-9_]+)', clean_content)
    enums = re.findall(r'\benum\s+([a-zA-Z0-9_]+)', clean_content)

    # Cyclomatic complexity
    complexities = []
    funcs = []
    idx = 0
    while True:
        match = re.search(r'\bfn\s+([a-zA-Z0-9_]+)[^{]*\{', clean_content[idx:])
        if not match:
            break
        start = idx + match.start()
        body_start = idx + match.end() - 1
        brace_count = 0
        body_end = body_start
        for i, char in enumerate(clean_content[body_start:]):
            if char == '{':
                brace_count += 1
            elif char == '}':
                brace_count -= 1
                if brace_count == 0:
                    body_end = body_start + i + 1
                    break
        funcs.append((match.group(1), clean_content[start:body_end]))
        idx = body_end

    for name, body in funcs:
        c = 1
        c += len(re.findall(r'\bif\b', body))
        c += len(re.findall(r'\bwhile\b', body))
        c += len(re.findall(r'\bfor\b', body))
        c += len(re.findall(r'\bmatch\b', body))
        c += len(re.findall(r'\?', body))
        c += len(re.findall(r'&&', body))
        c += len(re.findall(r'\|\|', body))
        if c > 15:
            complexities.append((name, c))

    err_unwrap = len(re.findall(r'\.unwrap\(\)', clean_content))
    err_expect = len(re.findall(r'\.expect\(', clean_content))
    err_question = len(re.findall(r'\?', clean_content))
    err_match = len(re.findall(r'match\s+[^{]*\{[^}]*Err\(', clean_content))

    # Inconsistent error handling
    err_handling_score = (1 if err_unwrap > 0 else 0) + (1 if err_expect > 0 else 0) + (1 if err_question > 0 else 0) + (1 if err_match > 0 else 0)
    inconsistent_errors = err_handling_score > 1

    # Magic numbers / strings
    magic_numbers = len(re.findall(r'\b(?<![a-zA-Z0-9_])(?!0\b|1\b)\d+\b', clean_content))
    # Magic strings - any string literal not in a const or static definition (rough approximation)
    magic_strings = len(re.findall(r'(?<!const\s)(?<!static\s)[a-zA-Z0-9_]+\s*[:=]\s*".*?"', content))

    debt_score = total_suppressions * 5 + len(orphaned_debt) * 3 + len(complexities) * 10 + (5 if inconsistent_errors else 0) + (magic_numbers + magic_strings)

    return {
        "suppressions": total_suppressions,
        "orphaned_debt": len(orphaned_debt),
        "complexities": len(complexities),
        "inconsistent_errors": inconsistent_errors,
        "structs": structs,
        "enums": enums,
        "debt_score": debt_score,
        "magic_numbers": magic_numbers,
        "magic_strings": magic_strings
    }

def main():
    commits = get_git_commits(6)
    if not commits:
        print("No commits found.")
        return

    results = {}

    for commit in commits:
        tracked_todos = get_tracked_todos(commit)
        files = list_files(commit)
        commit_score = 0
        file_scores = {}

        all_structs = []
        all_enums = []

        for path in files:
            content = get_file_content(commit, path)
            stats = analyze_file(content, path, tracked_todos)
            commit_score += stats["debt_score"]
            file_scores[path] = stats

            all_structs.extend(stats["structs"])
            all_enums.extend(stats["enums"])

        struct_counts = {}
        for s in all_structs: struct_counts[s] = struct_counts.get(s, 0) + 1
        enum_counts = {}
        for e in all_enums: enum_counts[e] = enum_counts.get(e, 0) + 1

        dup_structs = sum(1 for k, v in struct_counts.items() if v > 1)
        dup_enums = sum(1 for k, v in enum_counts.items() if v > 1)

        commit_score += (dup_structs + dup_enums) * 5

        results[commit] = {
            "score": commit_score,
            "file_scores": file_scores,
            "dup_structs": dup_structs,
            "dup_enums": dup_enums
        }

    current = commits[0]
    previous = commits[1:]

    report = "# Technical Debt Sanitation Scorecard\n\n"
    report += "## Trajectory (Last 6 Commits)\n"
    for c in reversed(commits):
        score = results[c]["score"]
        report += f"- Commit {c[:7]}: {score} debt points\n"

    report += "\n## Current Debt Density Analysis\n"
    curr_stats = results[current]
    report += f"Total Debt Score: {curr_stats['score']}\n"
    report += f"Duplicated Structs: {curr_stats['dup_structs']}\n"
    report += f"Duplicated Enums: {curr_stats['dup_enums']}\n\n"

    report += "### Files with Increasing Debt Density\n"
    if len(commits) > 1:
        prev_stats = results[commits[1]]
        for path, scores in curr_stats["file_scores"].items():
            if path in prev_stats["file_scores"]:
                if scores["debt_score"] > prev_stats["file_scores"][path]["debt_score"]:
                    report += f"- `{path}`: {prev_stats['file_scores'][path]['debt_score']} -> {scores['debt_score']}\n"
    else:
        report += "Not enough history to compare.\n"

    report += "\n### High Debt Files\n"
    sorted_files = sorted(curr_stats["file_scores"].items(), key=lambda x: x[1]["debt_score"], reverse=True)
    for path, scores in sorted_files[:10]:
        if scores["debt_score"] > 0:
            report += f"- `{path}`: {scores['debt_score']} points (Suppressions: {scores['suppressions']}, Orphaned Debt: {scores['orphaned_debt']}, Complex Fns: {scores['complexities']}, Inconsistent Errors: {scores['inconsistent_errors']}, Magic Values: {scores['magic_numbers'] + scores['magic_strings']})\n"

    print(report)

    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ.get("GITHUB_STEP_SUMMARY"), "a") as f:
            f.write(report)

    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "push":
        sha = commits[0]
        res = subprocess.run(["gh", "pr", "list", "--search", sha, "--state", "open", "--json", "number"], capture_output=True, text=True)
        if res.returncode == 0:
            try:
                prs = json.loads(res.stdout)
                for pr in prs:
                    pr_num = str(pr["number"])
                    subprocess.run(["gh", "pr", "comment", pr_num, "-b", report])
                    print(f"Commented on PR #{pr_num}")
            except Exception as e:
                print(f"Failed to post PR comment: {e}")

if __name__ == "__main__":
    main()
