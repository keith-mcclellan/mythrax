#!/usr/bin/env python3
import os
import subprocess
import re

def get_commits():
    subprocess.run(["git", "fetch", "--unshallow"], capture_output=True)
    subprocess.run(["git", "fetch", "--all"], capture_output=True)
    out = subprocess.run(["git", "log", "-n", "6", "--format=%H"], capture_output=True, text=True)
    if out.returncode != 0:
        return []
    commits = out.stdout.strip().split('\n')
    return list(reversed(commits))

def get_file_content(commit, path):
    out = subprocess.run(["git", "show", f"{commit}:{path}"], capture_output=True, text=True)
    if out.returncode == 0:
        return out.stdout
    return ""

def calculate_complexity(body):
    cc = 1
    cc += len(re.findall(r'\b(if|while|for|match)\b', body))
    cc += body.count('?')
    cc += body.count('&&')
    cc += body.count('||')
    return cc

def extract_functions(content):
    functions = []
    # robust heuristic that ignores strings and comments inside functions is very complex.
    # we will use a simpler approach.
    for match in re.finditer(r'fn\s+\w+[^\{]*\{', content):
        start = match.end() - 1 # start at the '{'
        braces = 0
        i = start
        body = ""
        in_string = False
        in_comment = False
        while i < len(content):
            char = content[i]

            if in_string:
                if char == '"' and content[i-1] != '\\':
                    in_string = False
            elif in_comment:
                if char == '\n':
                    in_comment = False
            else:
                if char == '"':
                    in_string = True
                elif char == '/' and i + 1 < len(content) and content[i+1] == '/':
                    in_comment = True
                elif char == '{':
                    braces += 1
                elif char == '}':
                    braces -= 1

            body += char

            if braces == 0 and not in_string and not in_comment:
                break
            i += 1
        functions.append(body)
    return functions

def extract_structs_enums(content):
    structs = re.findall(r'(?:struct|enum)\s+\w+[^\{]*\{([^}]+)\}', content)
    return [re.sub(r'\s+', '', s) for s in structs]

def extract_strings(content):
    return re.findall(r'"([^"]{4,})"', content)

def extract_numbers(content):
    return re.findall(r'\b([1-9][0-9]{2,})\b', content)

def count_inconsistent_errors(content):
    # Only penalize if we mix unwrap/expect with ?.
    # We will exclude match because match is common.
    has_unwrap_expect = 'unwrap(' in content or 'expect(' in content
    has_question_mark = '?' in content
    return 1 if has_unwrap_expect and has_question_mark else 0

def analyze_commit(commit):
    out = subprocess.run(["git", "ls-tree", "-r", "--name-only", commit], capture_output=True, text=True)
    files = out.stdout.strip().split('\n')
    target_files = [f for f in files if (f.startswith('mythrax-core/') and f.endswith('.rs')) or (f.startswith('scripts/') and f.endswith(('.py', '.sh')))]

    todo_content = get_file_content(commit, 'TODO.md').lower()

    scorecard = {}
    total_debt = 0

    all_structs = {}
    all_strings = {}
    all_numbers = {}

    for f in target_files:
        content = get_file_content(commit, f)
        if not content: continue

        file_debt = 0

        # Dead code suppression
        suppressions = len(re.findall(r'#\[allow\((dead_code|unused_imports|unreachable_code)\)\]', content))
        file_debt += suppressions

        # Orphaned TODOs
        todos = re.findall(r'(?:#|//)\s*(TODO|FIXME|HACK|TEMP)\b(.*)', content, re.IGNORECASE)
        for _, t_text in todos:
            text_clean = t_text.strip().lower()
            if text_clean and text_clean not in todo_content:
                file_debt += 1

        # Cyclomatic complexity > 15
        if f.endswith('.rs'):
            for body in extract_functions(content):
                if calculate_complexity(body) > 15:
                    file_debt += 1

        # Inconsistent error handling
        if f.endswith('.rs'):
            file_debt += count_inconsistent_errors(content)

        # Global deduplication lists
        if f.endswith('.rs'):
            for s in extract_structs_enums(content):
                all_structs[s] = all_structs.get(s, 0) + 1

        if f.endswith('.rs') or f.endswith('.py'):
            for s in extract_strings(content):
                all_strings[s] = all_strings.get(s, 0) + 1
            for n in extract_numbers(content):
                all_numbers[n] = all_numbers.get(n, 0) + 1

        scorecard[f] = file_debt
        total_debt += file_debt

    # Add cross-file duplication penalties
    # We'll just add them globally since we don't have file mapping for duplicates
    # Alternatively, attribute them to the file where they appeared first
    global_debt = 0
    for s, c in all_structs.items():
        if c > 1: global_debt += (c - 1)

    for s, c in all_strings.items():
        if c > 3: global_debt += 1

    for n, c in all_numbers.items():
        if c > 2: global_debt += 1

    total_debt += global_debt

    return total_debt, scorecard

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        return

    history = []
    for c in commits:
        total, scores = analyze_commit(c)
        history.append((c, total, scores))

    current_commit, current_total, current_scores = history[-1]
    prev_commit, prev_total, prev_scores = history[-2] if len(history) > 1 else (None, 0, {})

    improving = current_total <= prev_total
    traj_str = "Improving or Stable" if improving else "Degrading"

    report = f"## Sanitation Scorecard\n\n"
    report += f"**Trajectory:** {traj_str}\n"
    report += f"**Current Debt Score:** {current_total}\n"
    if prev_commit:
        report += f"**Previous Debt Score:** {prev_total}\n\n"

    report += "### Commit History (Oldest to Newest)\n"
    for c, tot, _ in history:
        report += f"- {c[:7]}: {tot}\n"

    report += "\n### Files with Increasing Debt Density\n"
    increasing_files = []
    for f, c_score in current_scores.items():
        p_score = prev_scores.get(f, 0)
        if c_score > p_score:
            increasing_files.append(f"- {f}: {p_score} -> {c_score}")

    if increasing_files:
        report += "\n".join(increasing_files) + "\n"
    else:
        report += "No files with increasing debt density.\n"

    print(report)

    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if os.environ.get("GITHUB_ACTIONS") == "true" and event_name == "push":
        branch = os.environ.get("GITHUB_REF_NAME")
        if branch:
            out = subprocess.run(["gh", "pr", "list", "--head", branch, "--json", "number", "-q", ".[0].number"], capture_output=True, text=True)
            pr_num = out.stdout.strip()
            if pr_num and pr_num.isdigit():
                subprocess.run(["gh", "pr", "comment", pr_num, "-b", report])
                print(f"Commented on PR #{pr_num}")

if __name__ == "__main__":
    main()
