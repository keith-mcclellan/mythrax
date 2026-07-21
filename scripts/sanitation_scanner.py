import json
import subprocess
import os
import sys

def run_command(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout, result.stderr

def count_occurrences(pattern, directory):
    cmd = f"grep -rnE '{pattern}' {directory} | wc -l"
    out, _ = run_command(cmd)
    try:
        return int(out.strip())
    except:
        return 0

def get_cognitive_complexity():
    # Use clippy to get functions > 15 cognitive complexity
    # We wrote clippy.toml in mythrax-core so it uses threshold 15
    cmd = "cargo clippy --manifest-path=mythrax-core/Cargo.toml --message-format=json --lib -- -A warnings -W clippy::cognitive_complexity -D clippy::cognitive_complexity"
    out, err = run_command(cmd)
    count = 0
    for line in out.splitlines():
        if "the function has a cognitive complexity of" in line and '"level":"error"' in line:
            count += 1
    return count

def get_unwrap_expect_counts():
    cmd = "grep -rnE '\\.unwrap\\(\\)' mythrax-core/src | wc -l"
    unwrap_count = run_command(cmd)[0].strip()
    cmd = "grep -rnE '\\.expect\\(' mythrax-core/src | wc -l"
    expect_count = run_command(cmd)[0].strip()
    return int(unwrap_count), int(expect_count)

def get_todo_counts():
    todos = run_command("grep -rnE 'TODO|FIXME|HACK|TEMP' mythrax-core/src")[0].strip().splitlines()
    clean_todos = [t for t in todos if 'tasks_todo' not in t and 'temp' not in t.lower() and 'temporal' not in t.lower()]
    return len(clean_todos)

def run_audit(commit):
    print(f"Checking commit: {commit}")
    run_command(f"git checkout {commit} --quiet")

    # Ensure clippy.toml is present for complexity threshold check in older commits
    run_command("echo 'cognitive-complexity-threshold = 15' > mythrax-core/clippy.toml")

    dead_code = count_occurrences("allow\\(dead_code\\)", "mythrax-core/src")
    todos = get_todo_counts()
    complexity = get_cognitive_complexity()
    unwrap_count, expect_count = get_unwrap_expect_counts()

    return {
        "dead_code": dead_code,
        "todos": todos,
        "complexity": complexity,
        "unwraps": unwrap_count,
        "expects": expect_count
    }

def main():
    out, _ = run_command("git log --format='%H' -n 6")
    commits = out.strip().splitlines()

    if len(commits) == 0:
        print("No commits found.")
        sys.exit(0)

    results = {}
    original_commit = commits[0]

    for commit in commits:
        results[commit] = run_audit(commit)

    run_command(f"git checkout {original_commit} --quiet")

    scorecard = "## Chief Architect Sanitation Scorecard\n\n"
    scorecard += "| Metric | " + " | ".join([f"Commit {c[:7]}" for c in reversed(commits)]) + " |\n"
    scorecard += "|---|" + "---|".join(["" for _ in range(len(commits))]) + "\n"

    metrics = [
        ("Dead Code (`allow(dead_code)`)", "dead_code"),
        ("Orphaned Debt (TODO/FIXME/HACK)", "todos"),
        ("High Complexity (score > 15)", "complexity"),
        ("Unsafe `unwrap()`", "unwraps"),
        ("Unsafe `expect()`", "expects")
    ]

    for label, key in metrics:
        row = f"| {label} | "
        values = [str(results[c][key]) for c in reversed(commits)]
        row += " | ".join(values) + " |\n"
        scorecard += row

    # Analyze trajectory
    scorecard += "\n### Trajectory Analysis\n\n"

    recent_commit = commits[0]
    oldest_commit = commits[-1]

    total_debt_recent = sum(results[recent_commit].values())
    total_debt_oldest = sum(results[oldest_commit].values())

    if total_debt_recent > total_debt_oldest:
        scorecard += f"**Overall Trajectory: DEGRADING.** Total codebase debt increased from {total_debt_oldest} to {total_debt_recent}.\n"
    elif total_debt_recent < total_debt_oldest:
        scorecard += f"**Overall Trajectory: IMPROVING.** Total codebase debt decreased from {total_debt_oldest} to {total_debt_recent}.\n"
    else:
        scorecard += f"**Overall Trajectory: STAGNANT.** Total codebase debt remains unacceptably high at {total_debt_recent}.\n"

    scorecard += "\n### Action Required\n"
    scorecard += "1. Refactor functions exceeding cognitive complexity of 15.\n"
    scorecard += "2. Remove dead code instead of suppressing it with `#[allow(dead_code)]`.\n"
    scorecard += "3. Standardize error handling using `?` operator and typed `Result` instead of `unwrap()` and `expect()`.\n"

    print(scorecard)

    # Check if we should post to a PR
    branch_name_env = os.environ.get("GITHUB_REF_NAME")
    if branch_name_env:
        # We are in GitHub Actions, try to find the PR for this branch and post the comment
        pr_list_cmd = f"gh pr list --head {branch_name_env} --json number --jq '.[0].number'"
        pr_number, _ = run_command(pr_list_cmd)
        pr_number = pr_number.strip()

        if pr_number and pr_number != 'null':
            # Post the comment
            with open("scorecard_comment.md", "w") as f:
                f.write(scorecard)
            run_command(f"gh pr comment {pr_number} --body-file scorecard_comment.md")
            print(f"Posted scorecard to PR #{pr_number}")
        else:
            print(f"No PR found for branch {branch_name_env}")

if __name__ == '__main__':
    main()
