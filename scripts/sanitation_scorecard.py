#!/usr/bin/env python3
import os
import re
import sys
import subprocess

def get_git_commits(n=6):
    res = subprocess.run(["git", "log", "--format=%H", f"-n{n}"], capture_output=True, text=True, check=True)
    commits = res.stdout.strip().split('\n')
    return [c for c in commits if c]

def get_file_content_at_commit(commit, path):
    res = subprocess.run(["git", "show", f"{commit}:{path}"], capture_output=True, text=True)
    if res.returncode == 0:
        return res.stdout
    return None

def analyze_commit(commit, repo_root, todo_content):
    metrics = {
        "dead_code": 0,
        "orphaned_todos": 0,
        "high_complexity_funcs": 0,
        "mixed_error_handling_files": 0,
        "magic_numbers": 0,
        "duplicated_structs": 0,
        "debt_density_per_file": {}
    }

    global_structs = {}

    # Get all files tracked at this commit
    res = subprocess.run(["git", "ls-tree", "-r", "--name-only", commit], capture_output=True, text=True)
    if res.returncode != 0:
        return metrics

    files = res.stdout.strip().split('\n')

    for rel_path in files:
        if not (rel_path.startswith('mythrax-core') or rel_path.startswith('scripts') or rel_path == 'sanitation_scorecard.py'):
            continue

        if not (rel_path.endswith(".rs") or rel_path.endswith(".py") or rel_path.endswith(".sh")):
            continue

        content = get_file_content_at_commit(commit, rel_path)
        if content is None:
            continue

        file_debt = 0

        # Dead code allows, unused imports, unreachable code (allow attributes)
        dead_code = len(re.findall(r'#\[allow\((?:dead_code|unused_imports|unreachable_code)\)\]', content))
        metrics["dead_code"] += dead_code
        file_debt += dead_code

        # Orphaned TODOs
        todos = re.findall(r'(?:#|//)\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE)
        for tag, text in todos:
            text = text.strip()
            if text and text not in todo_content:
                metrics["orphaned_todos"] += 1
                file_debt += 1

        if rel_path.endswith(".rs"):
            # Complexity
            functions = re.split(r'\bfn\s+', content)
            for func in functions[1:]:
                score = 1 + len(re.findall(r'\bif\b', func)) + len(re.findall(r'\bwhile\b', func)) + \
                        len(re.findall(r'\bfor\b', func)) + len(re.findall(r'\bmatch\b', func)) + \
                        len(re.findall(r'\?', func)) + len(re.findall(r'&&', func)) + \
                        len(re.findall(r'\|\|', func))
                if score > 15:
                    metrics["high_complexity_funcs"] += 1
                    file_debt += 1

            # Error handling
            unwraps = len(re.findall(r'\.unwrap\(\)', content))
            expects = len(re.findall(r'\.expect\(', content))
            questions = len(re.findall(r'\?', content))
            matches = len(re.findall(r'\bmatch\b.*?\{', content, re.DOTALL))
            if (unwraps > 0 or expects > 0) and (questions > 0 or matches > 0):
                metrics["mixed_error_handling_files"] += 1
                file_debt += 1

            # Duplicated structs
            struct_matches = re.finditer(r'\b(?:struct|enum)\s+(\w+)\s*\{([^}]+)\}', content)
            for m in struct_matches:
                body = re.sub(r'\s+', ' ', m.group(2).strip())
                if body not in global_structs:
                    global_structs[body] = []
                global_structs[body].append(m.group(1))

            # Magic numbers and strings
            for line in content.split('\n'):
                if 'const ' in line or 'static ' in line or 'test' in line or 'assert' in line or 'print' in line:
                    continue
                nums = re.findall(r'\b([2-9]|\d{2,})\b', line)
                strings = re.findall(r'"([^"]+)"', line)
                # Ignore empty strings or single chars
                strings = [s for s in strings if len(s) > 1 and not re.match(r'^[\w\d]+$', s)]
                metrics["magic_numbers"] += len(nums) + len(strings)
                file_debt += len(nums) + len(strings)

        metrics["debt_density_per_file"][rel_path] = file_debt

    # Compute duplicated structs
    for body, names in global_structs.items():
        if len(names) > 1:
            metrics["duplicated_structs"] += (len(names) - 1)

    metrics["total_debt"] = sum([
        metrics["dead_code"] * 5,
        metrics["orphaned_todos"] * 2,
        metrics["high_complexity_funcs"] * 10,
        metrics["mixed_error_handling_files"] * 5,
        metrics["magic_numbers"] * 1,
        metrics["duplicated_structs"] * 10
    ])

    return metrics

def main():
    repo_root = os.getcwd()

    try:
        commits = get_git_commits(6)
    except Exception as e:
        print(f"Error getting commits: {e}")
        return

    history = []
    commits.reverse()

    todo_path = os.path.join(repo_root, "TODO.md")
    todo_content = ""
    # We should get TODO.md at each commit to be accurate, but reading from current HEAD is close enough.
    # Alternatively, get it dynamically:

    for commit in commits:
        print(f"Analyzing commit {commit[:7]}...")
        todo_content_commit = get_file_content_at_commit(commit, "TODO.md")
        if todo_content_commit is None:
            todo_content_commit = ""

        metrics = analyze_commit(commit, repo_root, todo_content_commit)
        history.append((commit, metrics))

    report_lines = []
    report_lines.append("# Architecture Sanitation Scorecard")
    report_lines.append("")
    report_lines.append("## Trajectory (Last 5 Commits + Current)")
    report_lines.append("| Commit | Total Debt | Dead Code/Unused | Orphaned TODOs | High Complexity | Mixed Error Handling | Duplicated Structs | Magic Numbers/Strings |")
    report_lines.append("|---|---|---|---|---|---|---|---|")

    for commit, metrics in history:
        short_commit = commit[:7]
        report_lines.append(f"| {short_commit} | {metrics['total_debt']} | {metrics['dead_code']} | {metrics['orphaned_todos']} | {metrics['high_complexity_funcs']} | {metrics['mixed_error_handling_files']} | {metrics['duplicated_structs']} | {metrics['magic_numbers']} |")

    report_lines.append("")
    report_lines.append("## Increasing Debt Density")
    current_metrics = history[-1][1]
    prev_metrics = history[-2][1] if len(history) > 1 else current_metrics

    increasing_files = []
    for filepath, debt in current_metrics["debt_density_per_file"].items():
        prev_debt = prev_metrics["debt_density_per_file"].get(filepath, 0)
        if debt > prev_debt:
            increasing_files.append((filepath, prev_debt, debt))

    if increasing_files:
        report_lines.append("The following files have increasing debt density:")
        for fp, p_debt, c_debt in increasing_files:
            report_lines.append(f"- `{fp}`: {p_debt} -> {c_debt}")
    else:
        report_lines.append("No files show increasing debt density. Great job!")

    report_content = "\n".join(report_lines)

    with open("scorecard.md", "w") as f:
        f.write(report_content)

    print(report_content)

    github_event_name = os.environ.get("GITHUB_EVENT_NAME")
    if github_event_name == "push":
        github_ref_name = os.environ.get("GITHUB_REF_NAME")
        if github_ref_name:
            print(f"Push event detected on {github_ref_name}, posting comment to PR...")
            subprocess.run(["gh", "pr", "comment", github_ref_name, "--body-file", "scorecard.md"])

if __name__ == "__main__":
    main()
