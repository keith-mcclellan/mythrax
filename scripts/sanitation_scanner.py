#!/usr/bin/env python3
import os
import re
import sys
import subprocess
from collections import defaultdict

def run_cmd(cmd):
    try:
        return subprocess.check_output(cmd, shell=True, text=True, stderr=subprocess.DEVNULL)
    except subprocess.CalledProcessError:
        return ""

def get_todo_content():
    try:
        with open("TODO.md", "r") as f:
            return f.read().lower()
    except FileNotFoundError:
        return ""

TODO_TEXT = get_todo_content()

def is_orphaned_todo(text):
    text_clean = re.sub(r'[^a-zA-Z0-9\s]', '', text.lower()).strip()
    if not text_clean:
        return True

    words = text_clean.split()
    if not words:
        return True

    if len(words) < 3:
         return text_clean not in TODO_TEXT
    for i in range(len(words)-2):
         chunk = f"{words[i]} {words[i+1]} {words[i+2]}"
         if chunk in TODO_TEXT:
             return False
    return True

def analyze_file(content, filename):
    score = 0
    issues = []

    # 1. Dead code allowance
    dead_code_matches = re.finditer(r'#\[allow\(dead_code\)\]', content)
    for _ in dead_code_matches:
        issues.append(f"Debt: `#[allow(dead_code)]` found in `{filename}`")
        score += 1

    # 2. TODO/FIXME/HACK/TEMP
    todo_matches = re.finditer(r'//\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE)
    for m in todo_matches:
        tag = m.group(1).upper()
        text = m.group(2).strip()
        if is_orphaned_todo(text):
            issues.append(f"Debt: Orphaned {tag} found in `{filename}` ('{text}')")
            score += 1

    # 3. Cyclomatic complexity
    functions = re.split(r'\bfn\s+', content)
    for fn_body in functions[1:]:
        lines = fn_body.split('\n')
        name_match = re.match(r'([a-zA-Z0-9_]+)', fn_body)
        fn_name = name_match.group(1) if name_match else "unknown"

        brace_count = 0
        started = False
        complex_score = 1
        for line in lines:
            if '{' in line:
                brace_count += line.count('{')
                started = True
            if '}' in line:
                brace_count -= line.count('}')

            complex_score += len(re.findall(r'\b(if|match|while|for|loop)\b', line))
            complex_score += line.count('?') + line.count('&&') + line.count('||')

            if started and brace_count <= 0:
                break

        if complex_score > 15:
            issues.append(f"Debt: High cyclomatic complexity ({complex_score}) in function `{fn_name}` in `{filename}`")
            score += (complex_score - 15)

    # 4. Error handling
    err_patterns = ['unwrap()', 'expect(', '?', 'match ']
    found_patterns = [p for p in err_patterns if p in content]
    if len(found_patterns) >= 3:
        issues.append(f"Debt: Inconsistent error handling in `{filename}` (mixes {', '.join(found_patterns)})")
        score += 2

    # 5. Duplicated structs / enums
    structs = re.findall(r'struct\s+([a-zA-Z0-9_]+)\s*\{([^}]+)\}', content)
    enums = re.findall(r'enum\s+([a-zA-Z0-9_]+)\s*\{([^}]+)\}', content)

    # 6. Magic numbers / strings
    magic_nums = re.findall(r'[^a-zA-Z0-9_]([2-9]|[1-9][0-9]+)[^a-zA-Z0-9_]', content)
    if len(magic_nums) > 10:
        issues.append(f"Debt: High number of magic numbers in `{filename}`")
        score += 1

    string_literals = re.findall(r'"([^"]{5,})"', content)
    if len(string_literals) > 15:
        issues.append(f"Debt: High number of string literals in `{filename}`")
        score += 1

    return score, issues, structs, enums

def analyze_repo(commit="HEAD"):
    files_output = run_cmd(f"git ls-tree -r --name-only {commit}")
    files = files_output.strip().split('\n')

    total_score = 0
    file_scores = {}
    all_issues = []

    all_types = []

    for f in files:
        if (f.endswith('.rs') and 'mythrax-core' in f) or (f.endswith('.py') or f.endswith('.sh') and 'scripts' in f):
            content = run_cmd(f"git show {commit}:{f}")
            if not content:
                continue
            score, issues, structs, enums = analyze_file(content, f)
            if score > 0:
                file_scores[f] = score
                total_score += score
                all_issues.extend(issues)
            for s_name, s_body in structs:
                all_types.append((f, s_name, s_body, "struct"))
            for e_name, e_body in enums:
                all_types.append((f, e_name, e_body, "enum"))

    type_bodies = defaultdict(list)
    for f, name, body, kind in all_types:
        normalized = re.sub(r'\s+', '', body)
        type_bodies[normalized].append((f, name, kind))

    for body, occurrences in type_bodies.items():
        if len(occurrences) > 1:
            names = [f"`{o[0]}::{o[1]}`" for o in occurrences]
            kind = occurrences[0][2]
            all_issues.append(f"Debt: Duplicated {kind} definition: {', '.join(names)}")
            total_score += 1
            for o in occurrences:
                file_scores[o[0]] = file_scores.get(o[0], 0) + 1

    return total_score, file_scores, all_issues

def main():
    commits_out = run_cmd("git log -n 6 --format=%H").strip().split('\n')
    commits = [c for c in commits_out if c]

    if not commits:
        print("No commits found.")
        sys.exit(0)

    current_commit = commits[0]
    past_commits = commits[1:6]

    current_score, current_file_scores, current_issues = analyze_repo(current_commit)

    history_scores = []
    history_file_scores = []
    for c in past_commits:
        s, fs, _ = analyze_repo(c)
        history_scores.append(s)
        history_file_scores.append(fs)

    history_scores.reverse()
    history_file_scores.reverse()

    history_scores.append(current_score)
    history_file_scores.append(current_file_scores)

    output = []
    output.append("## Sanitation Scorecard")
    output.append("\n### Debt Trajectory (Last 5 commits -> Current)")
    output.append(f"**Total Debt Score:** {' -> '.join(map(str, history_scores))}")

    improving = len(history_scores) > 1 and history_scores[-1] <= history_scores[-2]
    output.append(f"**Trajectory:** {'Improving or Stable 📈' if improving else 'Degrading 📉'}")

    output.append("\n### Files with Increasing Debt Density")
    increasing_files = []
    if len(history_file_scores) > 1:
        prev_fs = history_file_scores[-2]
        curr_fs = history_file_scores[-1]

        for f, score in curr_fs.items():
            prev_score = prev_fs.get(f, 0)
            if score > prev_score:
                increasing_files.append(f"{f} ({prev_score} -> {score})")

    if increasing_files:
        for f in increasing_files:
            output.append(f"- {f}")
    else:
        output.append("- None")

    output.append("\n### Current Debt Findings")
    if not current_issues:
        output.append("- No debt findings! 🎉")
    for issue in current_issues:
        output.append(f"- {issue}")

    final_text = "\n".join(output)
    print(final_text)

    with open("sanitation_scorecard.md", "w") as f:
        f.write(final_text)

if __name__ == '__main__':
    main()
