import os
import re
import sys
import subprocess
from collections import defaultdict

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout.strip()

def get_files_at_commit(commit):
    out = run_cmd(f"git ls-tree -r {commit} --name-only")
    files = []
    for line in out.splitlines():
        if line.startswith("mythrax-core/") and line.endswith(".rs"):
            files.append(line)
        elif line.startswith("scripts/") and (line.endswith(".py") or line.endswith(".sh")):
            files.append(line)
    return files

def get_file_content_at_commit(commit, filepath):
    if commit is None:
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                return f.read()
        except:
            return ""
    else:
        out = subprocess.run(f"git show {commit}:{filepath}", shell=True, capture_output=True, text=True)
        if out.returncode == 0:
            return out.stdout
        return ""

def parse_todo_md(commit=None):
    content = get_file_content_at_commit(commit, "TODO.md")
    words = set(re.findall(r'[a-zA-Z0-9_]+', content.lower()))
    return words

def scan_code(commit=None):
    if commit is None:
        files = []
        for root, _, filenames in os.walk("mythrax-core"):
            for fname in filenames:
                if fname.endswith(".rs"):
                    files.append(os.path.join(root, fname))
        for root, _, filenames in os.walk("scripts"):
            for fname in filenames:
                if fname.endswith((".py", ".sh")):
                    files.append(os.path.join(root, fname))
    else:
        files = get_files_at_commit(commit)

    todo_words = parse_todo_md(commit)

    issues = []
    file_debt = defaultdict(int)
    structs_enums = set()

    for filepath in files:
        content = get_file_content_at_commit(commit, filepath)
        lines = content.splitlines()

        in_function = False
        func_name = ""
        complexity = 1
        braces = 0

        has_unwrap = False
        has_expect = False
        has_question = False
        has_match = False

        for i, line in enumerate(lines):
            line_stripped = line.strip()

            if "#[allow(dead_code)]" in line:
                issues.append((filepath, i+1, "dead_code", "Suppression of dead code found"))
                file_debt[filepath] += 1

            match = re.search(r'(?://|#)\s*(TODO|FIXME|HACK|TEMP):?\s*(.*)', line, re.IGNORECASE)
            if match:
                comment_text = match.group(2)
                comment_words = set(re.findall(r'[a-zA-Z0-9_]+', comment_text.lower()))
                if len(comment_words.intersection(todo_words)) < 2 and len(comment_words) > 0:
                    issues.append((filepath, i+1, "orphaned_todo", f"Orphaned {match.group(1)} found"))
                    file_debt[filepath] += 1

            if filepath.endswith(".rs"):
                if "fn " in line and "{" in line:
                    in_function = True
                    match_fn = re.search(r'fn\s+([a-zA-Z_][a-zA-Z0-9_]*)', line)
                    if match_fn:
                        func_name = match_fn.group(1)
                    else:
                        func_name = "unknown"
                    complexity = 1
                    braces = line.count("{") - line.count("}")
                elif in_function:
                    braces += line.count("{") - line.count("}")
                    branches = len(re.findall(r'\b(if|else if|match|for|while|loop)\b', line))
                    branches += line.count("?") + line.count("&&") + line.count("||")
                    complexity += branches

                    if braces <= 0:
                        in_function = False
                        if complexity > 15:
                            issues.append((filepath, i+1, "complexity", f"Function '{func_name}' has excessive cyclomatic complexity ({complexity} > 15)"))
                            file_debt[filepath] += (complexity - 15)

                if ".unwrap()" in line:
                    has_unwrap = True
                if ".expect(" in line:
                    has_expect = True
                if "?" in line:
                    has_question = True
                if "match " in line:
                    has_match = True

                match_struct = re.search(r'^\s*(?:pub\s+)?(?:struct|enum)\s+([a-zA-Z0-9_]+)', line)
                if match_struct:
                    name = match_struct.group(1)
                    if name in structs_enums:
                        issues.append((filepath, i+1, "duplicate_type", f"Duplicated struct/enum definition: {name}"))
                        file_debt[filepath] += 1
                    else:
                        structs_enums.add(name)

                if line_stripped.startswith("let ") and "=" in line_stripped:
                    magic_match = re.search(r'=\s*([0-9]{2,}|"[^"]{5,}")\s*;', line_stripped)
                    if magic_match and not any(x in line for x in ['const', 'static']):
                        issues.append((filepath, i+1, "magic_value", f"Potential magic number/string: {magic_match.group(1)}"))
                        file_debt[filepath] += 1

        if filepath.endswith(".rs"):
            mix_count = sum([has_unwrap, has_expect, has_question, has_match])
            if mix_count >= 3:
                issues.append((filepath, 0, "error_handling", "Inconsistent error handling patterns mixed (unwrap, expect, ?, match)"))
                file_debt[filepath] += 2

    return sum(file_debt.values()), file_debt, issues

def main():
    current_score, current_file_debt, current_issues = scan_code(commit=None)

    commits_out = run_cmd("git rev-list HEAD -n 6")
    commits = commits_out.splitlines()

    history_scores = []
    history_file_debts = []

    for c in commits:
        score, file_debt, _ = scan_code(commit=c)
        history_scores.append(score)
        history_file_debts.append(file_debt)

    report = ["# Sanitation Scorecard", ""]
    report.append(f"**Current Debt Score**: {current_score}")
    report.append("")
    report.append("## Trajectory (Last 5 commits)")

    # In CI, commit=None (working dir) is the same as HEAD (commits[0])
    # To compare trajectory safely, we compare current_score to the commit BEFORE it.
    # If the working directory has uncommitted changes, its score might differ from commits[0].
    # Otherwise, it equals commits[0], and we should compare against commits[1].

    if current_score != history_scores[0]:
        prev_score = history_scores[0]
        prev_file_debt = history_file_debts[0]
    elif len(history_scores) > 1:
        prev_score = history_scores[1]
        prev_file_debt = history_file_debts[1]
    else:
        prev_score = current_score
        prev_file_debt = current_file_debt

    # Output history of commits (skip the extra one used for lookback if needed)
    for i in range(min(5, len(commits))):
        report.append(f"- HEAD~{i} ({commits[i][:7]}): {history_scores[i]}")

    report.append("")
    if current_score > prev_score:
        report.append("**WARNING**: Debt is increasing compared to the previous commit!")
    elif current_score < prev_score:
        report.append("**SUCCESS**: Debt is decreasing!")
    else:
        report.append("Debt is stable compared to the previous commit.")

    report.append("")

    degrading_files = []
    for fp, debt in current_file_debt.items():
        prev_debt = prev_file_debt.get(fp, 0)
        if debt > prev_debt:
            degrading_files.append((fp, debt, prev_debt))

    if degrading_files:
        report.append("## 🚨 Files with Increasing Debt Density")
        for fp, debt, prev_debt in sorted(degrading_files, key=lambda x: x[1], reverse=True):
            report.append(f"- `{fp}`: increased from {prev_debt} to {debt}")
        report.append("")

    report.append("## Files with Debt")
    for fp, debt in sorted(current_file_debt.items(), key=lambda x: x[1], reverse=True):
        if debt > 0:
            report.append(f"- `{fp}`: {debt} points")

    report.append("")
    report.append("## Findings")
    findings_by_file = defaultdict(list)
    for fp, line, type_, msg in current_issues:
        findings_by_file[fp].append((line, msg))

    for fp, issues in findings_by_file.items():
        report.append(f"### {fp}")
        for line, msg in issues:
            report.append(f"- Line {line}: {msg}")

    report_content = "\n".join(report)

    with open("sanitation_scorecard.md", "w", encoding='utf-8') as f:
        f.write(report_content)

    # Mock behavior for environment without GitHub CLI
    with open("mock_pr_comment.md", "w", encoding='utf-8') as f:
        f.write(report_content)

    print("Scorecard generated.")

if __name__ == "__main__":
    main()
