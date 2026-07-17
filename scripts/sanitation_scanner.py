#!/usr/bin/env python3
import subprocess
import json
import re
import os
import sys
import shutil

def run_cmd(cmd, cwd=None, fail_ok=False):
    if isinstance(cmd, str):
        result = subprocess.run(cmd, shell=True, cwd=cwd, capture_output=True, text=True)
    else:
        result = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)

    if result.returncode != 0 and not fail_ok:
        print(f"Command failed: {cmd}")
        print(result.stderr)
    return result.stdout.strip(), result.stderr.strip(), result.returncode

def get_recent_commits(n=6):
    out, _, _ = run_cmd(["git", "log", f"-n{n}", "--format=%H"])
    return out.splitlines()

def get_tracked_todos(repo_root):
    todo_path = os.path.join(repo_root, "TODO.md")
    if not os.path.exists(todo_path):
        return []
    with open(todo_path, "r") as f:
        content = f.read()
    return content.lower()

def scan_file_for_debt(filepath, tracked_todo_text):
    metrics = {
        'dead_code_allows': 0,
        'orphaned_todos': 0,
        'unwrap_count': 0,
        'expect_count': 0,
        'question_mark_count': 0,
        'magic_numbers': 0,
        'magic_strings': 0,
        'duplicated_structs_enums': 0,
    }

    try:
        with open(filepath, "r", encoding="utf-8") as f:
            content = f.read()
    except Exception:
        return metrics, {}

    if filepath.endswith(".rs"):
        metrics['dead_code_allows'] += len(re.findall(r'#\[allow\(dead_code\)\]', content))

        # Error handling
        metrics['unwrap_count'] += len(re.findall(r'\.unwrap\(\)', content))
        metrics['expect_count'] += len(re.findall(r'\.expect\(', content))
        metrics['question_mark_count'] += len(re.findall(r'\?', content))

        # Magic numbers: basic heuristic (numbers not in consts, lets, struct inits, array sizes)
        # We will simply flag bare numbers in method calls or mathematical ops roughly
        lines = content.splitlines()
        for line in lines:
            line = line.strip()
            if line.startswith("const ") or line.startswith("static ") or line.startswith("let "):
                continue
            # match standalone numbers that aren't 0, 1, or 2 (common)
            matches = re.findall(r'\b[3-9]\b|\b[1-9][0-9]+\b', line)
            if matches and not line.startswith("//"):
                 metrics['magic_numbers'] += len(matches)

            # Magic strings: bare strings not in consts or macros
            # Ignore log macros, format!, println! etc
            if "!(" not in line and "const " not in line:
                str_matches = re.findall(r'"([^"]+)"', line)
                metrics['magic_strings'] += len(str_matches)

    # TODOs, FIXMEs, HACKs, TEMPs (works for .rs, .py, .sh)
    todos = re.findall(r'(?:#|//)\s*(?:TODO|FIXME|HACK|TEMP)[:\s]*(.*)', content, flags=re.IGNORECASE)
    for todo in todos:
        todo_clean = todo.strip().lower()
        if todo_clean and todo_clean not in tracked_todo_text:
             metrics['orphaned_todos'] += 1

    file_score = (
        metrics['dead_code_allows'] * 3 +
        metrics['orphaned_todos'] * 2 +
        metrics['unwrap_count'] +
        metrics['expect_count'] +
        metrics['magic_numbers'] +
        metrics['magic_strings'] +
        metrics['duplicated_structs_enums'] * 5
    )

    return metrics, file_score

def find_duplicate_structs_enums(repo_root):
    # A naive way to find duplicated structs/enums
    # We will extract all struct and enum names and count them
    names = {}
    for root_dir in ["mythrax-core/src"]:
        base_dir = os.path.join(repo_root, root_dir)
        if not os.path.exists(base_dir): continue
        for root, _, files in os.walk(base_dir):
            for file in files:
                if file.endswith(".rs"):
                    path = os.path.join(root, file)
                    try:
                        with open(path, "r", encoding="utf-8") as f:
                            content = f.read()
                            # find struct/enum definitions
                            matches = re.findall(r'(?:struct|enum)\s+([A-Z][a-zA-Z0-9_]*)', content)
                            for match in matches:
                                if match not in names:
                                    names[match] = []
                                names[match].append(path)
                    except Exception:
                        pass

    duplicates = 0
    file_duplicates = {}
    for name, paths in names.items():
        if len(paths) > 1:
            duplicates += 1
            for path in paths:
                file_duplicates[path] = file_duplicates.get(path, 0) + 1

    return duplicates, file_duplicates

def run_clippy_and_parse(repo_root):
    # Ensure clippy.toml exists
    clippy_conf = os.path.join(repo_root, "mythrax-core", "clippy.toml")
    has_conf = os.path.exists(clippy_conf)
    if not has_conf:
        with open(clippy_conf, "w") as f:
            f.write("cognitive-complexity-threshold = 15\n")

    cmd = [
        "cargo", "clippy", "--message-format=json", "--",
        "-W", "clippy::cognitive_complexity",
        "-W", "clippy::unwrap_used",
        "-W", "clippy::expect_used",
        "-W", "dead_code",
        "-W", "unused_imports",
        "-W", "unreachable_code"
    ]

    out, _, _ = run_cmd(cmd, cwd=os.path.join(repo_root, "mythrax-core"), fail_ok=True)

    if not has_conf:
        os.remove(clippy_conf)

    metrics = {
        'cognitive_complexity': 0,
        'dead_code': 0,
        'unused_imports': 0,
        'unreachable_code': 0,
    }

    file_clippy_scores = {}

    for line in out.splitlines():
        if not line.strip(): continue
        try:
            msg = json.loads(line)
            if msg.get("reason") == "compiler-message":
                msg_data = msg.get("message", {})
                code = msg_data.get("code", {})

                # Try to extract file path
                file_path = None
                spans = msg_data.get("spans", [])
                if spans:
                    for span in spans:
                        if span.get("is_primary"):
                            file_path = span.get("file_name")
                            break

                if code:
                    code_id = code.get("code")
                    penalty = 0
                    if code_id == "clippy::cognitive_complexity":
                        metrics['cognitive_complexity'] += 1
                        penalty = 5
                    elif code_id == "dead_code":
                        metrics['dead_code'] += 1
                        penalty = 2
                    elif code_id == "unused_imports":
                        metrics['unused_imports'] += 1
                        penalty = 1
                    elif code_id == "unreachable_code":
                        metrics['unreachable_code'] += 1
                        penalty = 3

                    if penalty > 0 and file_path:
                        full_path = os.path.join(repo_root, "mythrax-core", file_path)
                        file_clippy_scores[full_path] = file_clippy_scores.get(full_path, 0) + penalty
        except Exception:
            pass

    return metrics, file_clippy_scores

def analyze_commit(commit, repo_root):
    print(f"Analyzing commit {commit}...")
    run_cmd(["git", "checkout", commit], cwd=repo_root)

    tracked_todo_text = get_tracked_todos(repo_root)

    clippy_metrics, file_clippy_scores = run_clippy_and_parse(repo_root)
    total_duplicates, file_duplicates = find_duplicate_structs_enums(repo_root)

    regex_metrics = {
        'dead_code_allows': 0,
        'orphaned_todos': 0,
        'unwrap_count': 0,
        'expect_count': 0,
        'question_mark_count': 0,
        'magic_numbers': 0,
        'magic_strings': 0,
        'duplicated_structs_enums': total_duplicates,
    }

    file_scores = {}

    dirs_to_scan = ["mythrax-core/src", "scripts"]
    for d in dirs_to_scan:
        base_dir = os.path.join(repo_root, d)
        if not os.path.exists(base_dir): continue
        for root, _, files in os.walk(base_dir):
            for file in files:
                if file.endswith((".rs", ".py", ".sh")):
                    path = os.path.join(root, file)
                    m, f_score = scan_file_for_debt(path, tracked_todo_text)

                    # Add duplicate penalty
                    dup_count = file_duplicates.get(path, 0)
                    m['duplicated_structs_enums'] += dup_count
                    f_score += dup_count * 5

                    # Add clippy penalty
                    f_score += file_clippy_scores.get(path, 0)

                    if f_score > 0:
                        rel_path = os.path.relpath(path, repo_root)
                        file_scores[rel_path] = f_score

                    for k in regex_metrics:
                        if k != 'duplicated_structs_enums': # already added total
                            regex_metrics[k] += m[k]

    # Combine
    total_metrics = {**clippy_metrics, **regex_metrics}

    # Calculate an overall "debt score" (lower is better)
    debt_score = (
        total_metrics['cognitive_complexity'] * 5 +
        total_metrics['dead_code'] * 2 +
        total_metrics['unused_imports'] +
        total_metrics['unreachable_code'] * 3 +
        total_metrics['dead_code_allows'] * 3 +
        total_metrics['orphaned_todos'] * 2 +
        total_metrics['unwrap_count'] +
        total_metrics['expect_count'] +
        total_metrics['magic_numbers'] +
        total_metrics['magic_strings'] +
        total_metrics['duplicated_structs_enums'] * 5
    )

    total_metrics['debt_score'] = debt_score
    return total_metrics, file_scores

def main():
    repo_root = os.getcwd()

    # Ensure working tree is clean before we start checking out stuff
    out, _, _ = run_cmd(["git", "status", "--porcelain"])
    if out:
        print("Working tree is not clean. Stashing changes...")
        run_cmd(["git", "stash"])

    original_branch, _, _ = run_cmd(["git", "rev-parse", "--abbrev-ref", "HEAD"])
    if original_branch == "HEAD":
        original_branch, _, _ = run_cmd(["git", "rev-parse", "HEAD"])

    commits = get_recent_commits(6)
    if not commits:
        print("No commits found.")
        sys.exit(0)

    results = {}
    file_scores_history = {}

    for commit in commits:
        metrics, f_scores = analyze_commit(commit, repo_root)
        results[commit] = metrics
        file_scores_history[commit] = f_scores

    # Restore original state
    run_cmd(["git", "checkout", original_branch], cwd=repo_root)
    run_cmd(["git", "stash", "pop"], fail_ok=True)

    # Generate scorecard
    current_commit = commits[0]
    previous_commits = commits[1:]

    current_metrics = results[current_commit]
    current_file_scores = file_scores_history[current_commit]

    scorecard = "## 🧹 Sanitation Scorecard\n\n"
    scorecard += "| Metric | Current | Avg (Last 5) | Trajectory |\n"
    scorecard += "|---|---|---|---|\n"

    metrics_to_display = [
        ('Debt Score', 'debt_score'),
        ('Cognitive Complexity Violations', 'cognitive_complexity'),
        ('Dead Code', 'dead_code'),
        ('Unused Imports', 'unused_imports'),
        ('Unreachable Code', 'unreachable_code'),
        ('Dead Code Allows', 'dead_code_allows'),
        ('Orphaned TODOs/FIXMEs', 'orphaned_todos'),
        ('Unwraps', 'unwrap_count'),
        ('Expects', 'expect_count'),
        ('Magic Numbers', 'magic_numbers'),
        ('Magic Strings', 'magic_strings'),
        ('Duplicated Structs/Enums', 'duplicated_structs_enums'),
    ]

    for label, key in metrics_to_display:
        curr_val = current_metrics[key]

        if previous_commits:
            prev_vals = [results[c][key] for c in previous_commits]
            avg_prev = sum(prev_vals) / len(prev_vals)

            if curr_val < avg_prev:
                traj = "📉 Improving"
            elif curr_val > avg_prev:
                traj = "📈 Degrading"
            else:
                traj = "➖ Stable"
        else:
            avg_prev = 0
            traj = "N/A"

        scorecard += f"| {label} | {curr_val} | {avg_prev:.1f} | {traj} |\n"

    # File level trajectory
    scorecard += "\n### 🚨 Files with Increasing Debt Density\n\n"
    degrading_files = []

    for path, curr_score in current_file_scores.items():
        if previous_commits:
            prev_scores = [file_scores_history[c].get(path, 0) for c in previous_commits]
            avg_prev = sum(prev_scores) / len(prev_scores)
            if curr_score > avg_prev:
                degrading_files.append((path, curr_score, avg_prev))

    if degrading_files:
        scorecard += "| File | Current Score | Avg (Last 5) |\n"
        scorecard += "|---|---|---|\n"
        for path, curr, avg in sorted(degrading_files, key=lambda x: x[1], reverse=True):
            scorecard += f"| `{path}` | {curr} | {avg:.1f} |\n"
    else:
        scorecard += "No files showed an increase in debt density compared to the last 5 commits. 🎉\n"

    print("\n" + scorecard)

    # Post to PR if applicable
    if os.environ.get("GITHUB_ENV") and os.environ.get("GITHUB_EVENT_NAME") == "push":
        pr_url, _, _ = run_cmd(["gh", "pr", "list", "--commit", current_commit, "--json", "url", "--jq", ".[0].url"], fail_ok=True)
        if pr_url:
            with open("scorecard.md", "w") as f:
                f.write(scorecard)
            run_cmd(["gh", "pr", "comment", pr_url, "-F", "scorecard.md"])

    if os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        pr_number = os.environ.get("GITHUB_REF").split('/')[2] if os.environ.get("GITHUB_REF") else None
        if pr_number:
            with open("scorecard.md", "w") as f:
                f.write(scorecard)
            run_cmd(["gh", "pr", "comment", pr_number, "-F", "scorecard.md"])

if __name__ == "__main__":
    main()
