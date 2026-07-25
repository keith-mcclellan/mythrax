#!/usr/bin/env python3
import os
import sys
import subprocess
import json
import re
from collections import defaultdict
import shlex

def run_cmd(cmd, cwd=None, capture_output=True):
    res = subprocess.run(cmd, cwd=cwd, shell=True, text=True, capture_output=capture_output)
    return res

def parse_todo_md():
    content = ""
    if os.path.exists("TODO.md"):
        with open("TODO.md", "r", encoding="utf-8") as f:
            content = f.read().lower()
    return content

def get_current_branch():
    res = run_cmd("git branch --show-current")
    branch = res.stdout.strip()
    if not branch:
        # Handle detached HEAD state (like in GitHub Actions)
        branch = os.environ.get("GITHUB_REF_NAME", "main")
    return branch

def analyze_commit(commit_hash):
    # Explicitly delete clippy.toml before checkout
    if os.path.exists("mythrax-core/clippy.toml"):
        os.remove("mythrax-core/clippy.toml")

    # Checkout commit
    res = run_cmd(f"git checkout {commit_hash}")
    if res.returncode != 0:
        print(f"Failed to checkout {commit_hash}")

    # Re-create clippy.toml
    os.makedirs("mythrax-core", exist_ok=True)
    with open("mythrax-core/clippy.toml", "w") as f:
        f.write("cognitive-complexity-threshold = 15\n")

    # Run clippy
    # Redirect output to file to avoid broken pipe issues with large output
    run_cmd("cargo clippy --message-format=json -A warnings -W clippy::cognitive_complexity > clippy_output.json", cwd="mythrax-core")

    complexity_warnings = defaultdict(int)
    if os.path.exists("mythrax-core/clippy_output.json"):
        with open("mythrax-core/clippy_output.json", "r") as f:
            for line in f:
                if not line.strip(): continue
                try:
                    msg = json.loads(line)
                    if msg.get("reason") == "compiler-message":
                        message = msg.get("message", {})
                        if message.get("code", {}).get("code") == "clippy::cognitive_complexity":
                            spans = message.get("spans", [])
                            if spans:
                                file_name = spans[0].get("file_name")
                                complexity_warnings[file_name] += 1
                except json.JSONDecodeError:
                    pass

    todo_content = parse_todo_md()

    file_scores = defaultdict(int)
    global_structs = set()

    # Walk through .rs, .py, .sh files excluding target/
    for root, dirs, files in os.walk("."):
        if "target" in dirs:
            dirs.remove("target")
        if ".git" in dirs:
            dirs.remove(".git")

        for file in files:
            if file.endswith((".rs", ".py", ".sh")):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, "r", encoding="utf-8") as f:
                        content = f.read()
                except:
                    continue

                score = 0

                # 1. Dead code (#[allow(dead_code)])
                score += len(re.findall(r'#\[allow\(dead_code\)\]', content))

                # 2. Orphaned TODO/FIXME/HACK/TEMP
                comments = re.findall(r'(?://|#)\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE)
                for tag, text in comments:
                    clean_text = text.strip().lower()
                    if clean_text and clean_text not in todo_content:
                        score += 1

                # 3. Inconsistent error handling
                has_unwrap_expect = bool(re.search(r'\.(unwrap|expect)\(', content))
                has_match_question = bool(re.search(r'(\?|match\s+)', content))
                if has_unwrap_expect and has_match_question:
                    score += 1

                # 4. Duplicated structs/enums
                structs_enums = re.findall(r'(?:struct|enum)\s+([A-Za-z0-9_]+)', content)
                for se in structs_enums:
                    if se in global_structs:
                        score += 1
                    else:
                        global_structs.add(se)

                # 5. Magic numbers/strings (simple heuristic)
                magic_numbers = len(re.findall(r'(?<![a-zA-Z0-9_])\b(?:[2-9]|\d{2,})\b(?![a-zA-Z0-9_])', content))
                score += magic_numbers * 0.1

                # Add complexity warnings from clippy
                # The path from clippy might be relative to mythrax-core, but filepath is from root (.)
                # So we make filepath relative to mythrax-core to match clippy keys
                if filepath.startswith("./mythrax-core/"):
                    rel_path = os.path.relpath(filepath, "./mythrax-core")
                    # Cargo clippy outputs relative to the workspace/manifest dir (mythrax-core)
                    score += complexity_warnings.get(rel_path, 0) * 2

                file_scores[filepath] = score

    return file_scores

def main():
    # Get last 6 commits
    res = run_cmd("git log -n 6 --format='%H'")
    commits = res.stdout.strip().split('\n')
    if not commits or not commits[0]:
        print("No commits found.")
        sys.exit(0)

    original_branch = get_current_branch()

    # Reverse to process oldest to newest
    commits.reverse()

    history_scores = []

    try:
        for commit in commits:
            print(f"Analyzing commit {commit}...")
            scores = analyze_commit(commit)
            history_scores.append((commit, scores))
    finally:
        # Restore state
        if os.path.exists("mythrax-core/clippy.toml"):
            os.remove("mythrax-core/clippy.toml")
        run_cmd(f"git checkout {shlex.quote(original_branch)}")

    # Compare oldest to newest
    oldest_scores = history_scores[0][1]
    newest_scores = history_scores[-1][1]

    scorecard = ["# Architecture Sanitation Scorecard\n"]
    scorecard.append("## Trajectory (Last 5 Commits to Current)\n")

    degrading_files = []

    all_files = set(oldest_scores.keys()).union(set(newest_scores.keys()))

    for f in sorted(all_files):
        old_score = oldest_scores.get(f, 0)
        new_score = newest_scores.get(f, 0)

        # Ensure we evaluate delta properly; only flag if debt is increasing
        if new_score > old_score:
            degrading_files.append((f, old_score, new_score))

    if degrading_files:
        scorecard.append("### ⚠️ Degrading Files (Debt Density Increasing)\n")
        scorecard.append("| File | Old Score | New Score | Delta |\n")
        scorecard.append("|---|---|---|---|\n")
        for f, old, new in degrading_files:
            scorecard.append(f"| {f} | {old:.1f} | {new:.1f} | +{new - old:.1f} |\n")
    else:
        scorecard.append("✅ No files are degrading in debt density.\n")

    scorecard.append("\n### Current Debt Summary (Top 10)\n")
    sorted_new = sorted(newest_scores.items(), key=lambda x: x[1], reverse=True)[:10]
    for f, score in sorted_new:
        if score > 0:
            scorecard.append(f"- **{f}**: {score:.1f}\n")

    scorecard_content = "".join(scorecard)

    with open("architect_scorecard.md", "w") as f:
        f.write(scorecard_content)

    print(scorecard_content)

    # PR Comment
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    ref_name = os.environ.get("GITHUB_REF_NAME")

    if event_name == "push" and ref_name:
        # Use GITHUB_REF_NAME to query PRs for this branch
        pr_check = run_cmd(f"gh pr list --head {shlex.quote(ref_name)} --json number --jq '.[0].number'")
        pr_number = pr_check.stdout.strip()
        if pr_number and pr_number.isdigit():
            # Robust shell quoting for the comment file path
            comment_cmd = f"gh pr comment {shlex.quote(pr_number)} --body-file architect_scorecard.md"
            run_cmd(comment_cmd)
        else:
            print("No PR found for this branch to comment on.")

if __name__ == "__main__":
    main()
