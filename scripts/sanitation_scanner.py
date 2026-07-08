#!/usr/bin/env python3

import os
import re
import subprocess
import glob
import json

def get_tracked_todos():
    todos = set()
    try:
        with open("TODO.md", "r") as f:
            for line in f:
                if line.strip().startswith("- [ ]") or line.strip().startswith("- [x]"):
                    todos.add(line.strip().lower())
                elif line.strip().startswith("#") or not line.strip():
                    continue
                else:
                    todos.add(line.strip().lower())
        return todos
    except Exception:
        return set()

def run_git_cmd(args):
    result = subprocess.run(args, capture_output=True, text=True)
    return result.stdout.strip()

def get_file_content_at_commit(commit, path):
    if commit is None:
        try:
            with open(path, "r", encoding="utf-8", errors="ignore") as f:
                return f.read()
        except Exception:
            return ""
    else:
        result = subprocess.run(["git", "show", f"{commit}:{path}"], capture_output=True, text=True, errors="ignore")
        if result.returncode == 0:
            return result.stdout
        return ""

def list_files_in_commit(commit):
    if commit is None:
        files = []
        for root, _, filenames in os.walk("mythrax-core/src"):
            for name in filenames:
                if name.endswith(".rs"):
                    files.append(os.path.join(root, name))
        for root, _, filenames in os.walk("scripts"):
            for name in filenames:
                if name.endswith(".py") or name.endswith(".sh"):
                    files.append(os.path.join(root, name))
        return files
    else:
        out = run_git_cmd(["git", "ls-tree", "-r", "--name-only", commit])
        files = []
        for line in out.splitlines():
            if (line.startswith("mythrax-core/src/") and line.endswith(".rs")) or \
               (line.startswith("scripts/") and (line.endswith(".py") or line.endswith(".sh"))):
                files.append(line)
        return files

def get_clippy_diagnostics(commit):
    diagnostics = {}

    # We can only reliably run cargo clippy on the current working directory,
    # checking out older commits to run clippy on them is too slow and intrusive.
    # Therefore, we will only run clippy for the HEAD (None commit), and for past
    # commits we will fall back to regex, or just keep previous scores 0.
    # But wait, the persona wants to see trajectory. We must parse previous versions.
    # Since checking out old code and running clippy takes ~30 seconds, running it 5 times = 2.5 minutes.
    # Let's try to run clippy for all commits.

    if commit is not None:
        # Checkout the commit temporarily
        subprocess.run(["git", "checkout", commit], capture_output=True)

    try:
        result = subprocess.run(
            ["cargo", "clippy", "--message-format=json"],
            cwd="mythrax-core",
            capture_output=True,
            text=True
        )

        for line in result.stdout.splitlines():
            try:
                msg = json.loads(line)
                if msg.get("reason") == "compiler-message":
                    message = msg.get("message", {})
                    code = message.get("code", {})
                    if not code:
                        continue
                    code_id = code.get("code", "")

                    # Mapping clippy/compiler codes to our debt
                    debt_type = None
                    if code_id in ["dead_code", "unused_variables", "unused_imports", "unused_mut", "unreachable_patterns", "unreachable_code"]:
                        debt_type = "dead_code"
                    elif "cyclomatic_complexity" in code_id or "cognitive_complexity" in code_id:
                        debt_type = "complex_functions"

                    if debt_type:
                        spans = message.get("spans", [])
                        for span in spans:
                            if span.get("is_primary"):
                                file_name = "mythrax-core/" + span.get("file_name", "")
                                if file_name not in diagnostics:
                                    diagnostics[file_name] = {"dead_code": 0, "complex_functions": 0}
                                diagnostics[file_name][debt_type] += 1
            except Exception:
                pass
    finally:
        if commit is not None:
            # Restore to main branch/head
            subprocess.run(["git", "checkout", "-"], capture_output=True)

    return diagnostics


def analyze_code_regex(content, path, tracked_todos):
    debt = {
        "dead_code": 0, # To be augmented by clippy
        "complex_functions": 0, # To be augmented by clippy
        "orphan_todos": 0,
        "mixed_errors": 0,
        "magic_numbers": 0,
        "duplicated_structs": [],
        "struct_definitions": {}
    }

    # 1. Dead code suppression
    debt["dead_code"] += len(re.findall(r'#\[allow\(dead_code\)\]', content))

    # 2. Todos
    todo_matches = re.findall(r'//.*(?:TODO|FIXME|HACK|TEMP).*', content, re.IGNORECASE)
    todo_matches += re.findall(r'#.*(?:TODO|FIXME|HACK|TEMP).*', content, re.IGNORECASE)

    for match in todo_matches:
        clean_match = match.replace("//", "").replace("#", "").strip().lower()
        found = False
        for tt in tracked_todos:
            if clean_match in tt or tt in clean_match:
                found = True
                break
        if not found:
            debt["orphan_todos"] += 1

    # 3. Mixed errors
    if path.endswith(".rs"):
        has_unwrap = "unwrap()" in content
        has_expect = "expect(" in content
        has_qmark = "?" in content
        has_match = "match " in content
        types = sum([has_unwrap or has_expect, has_qmark, has_match])
        if types > 1:
            debt["mixed_errors"] += 1

    # 4. Duplicated structs/enums (collect definitions)
    # Using more robust regex for structs, handling generics
    structs = re.findall(r'(?:struct|enum)\s+([A-Za-z0-9_]+)(?:<[^>]+>)?\s*\{([^}]+)\}', content)
    for name, body in structs:
        clean_body = re.sub(r'\s+', ' ', body).strip()
        debt["struct_definitions"][name] = clean_body

    # 5. Magic numbers and Magic Strings
    lines = content.splitlines()
    for line in lines:
        if "const" in line or "static" in line:
            continue
        # find numbers
        nums = re.findall(r'\b\d+\b', line)
        for num in nums:
            if num not in ["0", "1"]:
                debt["magic_numbers"] += 1

        # find magic strings (string literals not inside const/static and not empty or single char)
        strings = re.findall(r'"([^"]{2,})"', line)
        debt["magic_numbers"] += len(strings) # We count magic strings as magic numbers for debt

    return debt

def calculate_total_debt(files_debt, global_structs):
    total_score = 0
    dup_structs = 0
    seen_bodies = set()
    for struct_def in global_structs.values():
        if struct_def in seen_bodies:
            dup_structs += 1
        seen_bodies.add(struct_def)

    for path, debt in files_debt.items():
        total_score += debt["dead_code"]
        total_score += debt["orphan_todos"]
        total_score += debt["complex_functions"]
        total_score += debt["mixed_errors"]
        total_score += debt["magic_numbers"] // 10 # scale down magic numbers

    total_score += dup_structs * 5 # weight dup structs more
    return total_score, dup_structs

def main():
    tracked_todos = get_tracked_todos()

    commits_out = run_git_cmd(["git", "log", "-n", "5", "--format=%H"])
    commits = commits_out.splitlines()
    commits.reverse() # chronologically oldest to newest

    history = []

    # Save the original branch so we can restore properly
    original_branch = run_git_cmd(["git", "branch", "--show-current"])

    commits.append(None)

    current_files_debt = {}

    for i, commit in enumerate(commits):
        files = list_files_in_commit(commit)
        global_structs = {}
        files_debt = {}

        # Run clippy for this commit
        clippy_diagnostics = get_clippy_diagnostics(commit)

        for path in files:
            content = get_file_content_at_commit(commit, path)
            debt = analyze_code_regex(content, path, tracked_todos)

            # Merge clippy diagnostics
            if path in clippy_diagnostics:
                debt["dead_code"] += clippy_diagnostics[path]["dead_code"]
                debt["complex_functions"] += clippy_diagnostics[path]["complex_functions"]

            files_debt[path] = debt
            global_structs.update(debt["struct_definitions"])

        score, dup_structs = calculate_total_debt(files_debt, global_structs)
        history.append({
            "commit": commit[:7] if commit else "HEAD",
            "score": score,
            "files_debt": files_debt,
            "dup_structs": dup_structs
        })
        if commit is None:
            current_files_debt = files_debt

    # Restore to original branch just in case
    if original_branch:
        subprocess.run(["git", "checkout", original_branch], capture_output=True)

    with open("sanitation_scorecard.md", "w") as f:
        f.write("# Sanitation Scorecard\n\n")
        f.write("## Trajectory (Last 5 Commits)\n")
        f.write("| Commit | Debt Score |\n")
        f.write("|---|---|\n")
        for h in history:
            f.write(f"| {h['commit']} | {h['score']} |\n")

        if len(history) >= 2:
            prev = history[-2]["score"]
            curr = history[-1]["score"]
            if curr > prev:
                f.write("\n**Trajectory:** 🔴 Degrading (Debt increased)\n")
            elif curr < prev:
                f.write("\n**Trajectory:** 🟢 Improving (Debt decreased)\n")
            else:
                f.write("\n**Trajectory:** 🟡 Stable\n")

        f.write("\n## Current Debt Density Increases\n")

        if len(history) >= 2:
            prev_debt = history[-2]["files_debt"]
            curr_debt = history[-1]["files_debt"]

            flagged = False
            for path, debt in curr_debt.items():
                curr_file_score = debt["dead_code"] + debt["orphan_todos"] + debt["complex_functions"] + debt["mixed_errors"] + (debt["magic_numbers"]//10)
                prev_file_score = 0
                if path in prev_debt:
                    pd = prev_debt[path]
                    prev_file_score = pd["dead_code"] + pd["orphan_todos"] + pd["complex_functions"] + pd["mixed_errors"] + (pd["magic_numbers"]//10)

                if curr_file_score > prev_file_score:
                    if not flagged:
                        f.write("| File | Previous Score | Current Score |\n")
                        f.write("|---|---|---|\n")
                        flagged = True
                    f.write(f"| {path} | {prev_file_score} | {curr_file_score} |\n")

            if not flagged:
                f.write("No files have increasing debt density.\n")

if __name__ == "__main__":
    main()
