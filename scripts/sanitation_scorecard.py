#!/usr/bin/env python3
import os
import re
import subprocess
import sys
import shlex

def run_cmd(cmd):
    result = subprocess.run(cmd, capture_output=True, text=True, check=False)
    return result.stdout.strip()

def get_commits():
    out = run_cmd(["git", "log", "-n", "6", "--format=%H"])
    return [c for c in out.split('\n') if c]

def get_files(commit):
    out = run_cmd(["git", "ls-tree", "-r", "--name-only", commit])
    files = []
    for f in out.split('\n'):
        if not f: continue
        if f.startswith('mythrax-core/') or f.startswith('scripts/'):
            if f.endswith('.rs') or f.endswith('.py') or f.endswith('.sh'):
                files.append(f)
    return files

def get_file_content(commit, path):
    return run_cmd(["git", "show", f"{commit}:{path}"])

def get_todo_md(commit):
    return run_cmd(["git", "show", f"{commit}:TODO.md"])

def analyze_file(content, filename, todo_md_content, global_structs):
    debt = 0
    issues = []

    # 1. Dead code & Suppressions
    suppressions = len(re.findall(r'#\[allow\([^)]*(?:dead_code|unused_imports|unreachable_code)[^)]*\)\]', content))
    if suppressions > 0:
        debt += suppressions * 5
        issues.append(f"Found {suppressions} debt suppression annotations")

    # 2. TODO, FIXME, HACK, TEMP
    todos = re.findall(r'(?:#|//)\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', content, re.IGNORECASE)
    for tag, desc in todos:
        desc = desc.strip()
        if desc and desc not in todo_md_content:
            debt += 10
            issues.append(f"Orphaned {tag}: {desc}")

    if filename.endswith('.rs'):
        # 3. Cyclomatic complexity
        funcs = re.split(r'\bfn\s+\w+', content)
        for f_body in funcs[1:]:
            complexity = 1
            complexity += len(re.findall(r'\bif\b', f_body))
            complexity += len(re.findall(r'\bwhile\b', f_body))
            complexity += len(re.findall(r'\bfor\b', f_body))
            complexity += len(re.findall(r'\bmatch\b', f_body))
            complexity += len(re.findall(r'\?', f_body))
            complexity += len(re.findall(r'&&', f_body))
            complexity += len(re.findall(r'\|\|', f_body))
            if complexity > 15:
                debt += (complexity - 15) * 2
                issues.append(f"High cyclomatic complexity ({complexity})")

        # 4. Error handling
        unwraps = len(re.findall(r'\.unwrap\(\)', content))
        expects = len(re.findall(r'\.expect\(', content))
        question_marks = len(re.findall(r'\?', content))
        matches = len(re.findall(r'\bmatch\b\s+.*\{', content))

        err_types = sum(1 for x in [unwraps, expects, question_marks, matches] if x > 0)

        if err_types > 1:
            debt += 15
            issues.append("Inconsistent error handling (mixed patterns)")

        # 5. Duplicated struct/enum
        structs = re.findall(r'(?:struct|enum)\s+(\w+)[^{]*\{([^}]+)\}', content)
        for s_name, s_body in structs:
            fields = re.sub(r'\s+', '', s_body)
            for existing_name, existing_fields in global_structs:
                if fields == existing_fields and s_name != existing_name:
                    debt += 20
                    issues.append(f"Duplicated struct/enum: {s_name} looks like {existing_name}")
            global_structs.append((s_name, fields))

        # 6. Magic numbers
        magic_numbers = len(re.findall(r'==\s*\d+', content)) + len(re.findall(r'==\s*".*?"', content))
        if magic_numbers > 0:
            debt += magic_numbers * 2
            issues.append(f"Found {magic_numbers} potential magic numbers/strings")

    return debt, issues

def analyze_commit(commit):
    files = get_files(commit)
    todo_md = get_todo_md(commit)

    total_debt = 0
    file_debts = {}
    global_structs = []

    for f in files:
        content = get_file_content(commit, f)
        debt, issues = analyze_file(content, f, todo_md, global_structs)
        total_debt += debt
        file_debts[f] = debt

    return total_debt, file_debts

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        sys.exit(1)

    commits = commits[::-1]

    results = []
    for c in commits:
        debt, f_debts = analyze_commit(c)
        results.append((c, debt, f_debts))

    scorecard = "## Technical Debt Sanitation Scorecard\n\n"
    scorecard += "| Commit | Total Debt Score | Trajectory |\n"
    scorecard += "|---|---|---|\n"

    prev_debt = None
    for c, debt, f_debts in results:
        traj = "N/A"
        if prev_debt is not None:
            if debt < prev_debt:
                traj = "📉 Improving"
            elif debt > prev_debt:
                traj = "📈 Degrading"
            else:
                traj = "➡️ Stable"
        scorecard += f"| {c[:7]} | {debt} | {traj} |\n"
        prev_debt = debt

    scorecard += "\n### Increasing Debt Density Files (Latest Commit)\n"

    latest_c, latest_debt, latest_f_debts = results[-1]
    prev_c, prev_debt, prev_f_debts = results[-2] if len(results) > 1 else (None, 0, {})

    increasing_files = False
    for f, d in latest_f_debts.items():
        pd = prev_f_debts.get(f, 0)
        if d > pd:
            scorecard += f"- `{f}`: +{d - pd} debt score\n"
            increasing_files = True

    if not increasing_files:
        scorecard += "- No files with increasing debt density!\n"

    print(scorecard)

    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "push":
        pr_url = run_cmd(["gh", "pr", "view", "--json", "url", "-q", ".url"])
        if pr_url:
            subprocess.run(["gh", "pr", "comment", pr_url, "-b", scorecard])
            print("Appended findings to PR.")
        else:
            print("No PR found for this push.")

if __name__ == "__main__":
    main()
