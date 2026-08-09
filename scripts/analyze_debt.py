import subprocess
import re
import os
import sys
import shutil

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout.strip()

def get_commits():
    # Get last 6 commits (current + last 5)
    return run_cmd("git log -n 6 --format='%h|%s'").split('\n')

def analyze_commit(commit_hash):
    metrics = {
        'dead_code': 0,
        'orphaned_todos': 0,
        'high_complexity_funcs': 0,
        'inconsistent_errors': 0,
        'duplicated_types': 0,
        'magic_numbers': 0,
    }

    # Run clippy for complexity
    # We must clean cargo before running clippy on a different commit to avoid caching issues
    run_cmd("cd mythrax-core && cargo clean")

    # Backup existing clippy.toml if it exists
    clippy_backed_up = False
    if os.path.exists("mythrax-core/clippy.toml"):
        run_cmd("mv mythrax-core/clippy.toml mythrax-core/clippy.toml.bak")
        clippy_backed_up = True

    clippy_out = run_cmd(f"cd mythrax-core && echo 'cognitive-complexity-threshold = 15' > clippy.toml && cargo clippy --no-default-features --lib -- -W clippy::cognitive_complexity 2>&1")
    complexity_matches = re.findall(r'cognitive complexity of \(\d+/15\)', clippy_out)
    metrics['high_complexity_funcs'] = len(complexity_matches)

    # Clean up and restore clippy toml
    run_cmd("rm mythrax-core/clippy.toml")
    if clippy_backed_up:
        run_cmd("mv mythrax-core/clippy.toml.bak mythrax-core/clippy.toml")

    # Get dead code suppressions
    dead_code = run_cmd("grep -r '#\\[allow(dead_code)\\]' mythrax-core/src/ | wc -l")
    metrics['dead_code'] = int(dead_code) if dead_code else 0

    # Get TODOs - from rust AND scripts
    todos_raw_rust = run_cmd("grep -rE 'TODO|FIXME|HACK|TEMP' mythrax-core/src/")
    todos_raw_scripts = run_cmd("grep -rE 'TODO|FIXME|HACK|TEMP' scripts/")
    todos = [t for t in (todos_raw_rust + '\n' + todos_raw_scripts).split('\n') if t.strip()]

    todo_md = ""
    if os.path.exists("TODO.md"):
        with open("TODO.md", "r") as f:
            todo_md = f.read().lower()

    orphaned_count = 0
    for t in todos:
        m = re.search(r'(?:TODO|FIXME|HACK|TEMP)[:\s]*(.*)', t, re.IGNORECASE)
        if m:
            desc = m.group(1).strip().lower()
            # Extremely basic check: does the text of the comment appear in TODO.md?
            if desc and len(desc) > 5 and desc not in todo_md:
                orphaned_count += 1
            elif not desc:
                orphaned_count += 1
    metrics['orphaned_todos'] = orphaned_count

    # Inconsistent errors (.unwrap, .expect, ?, match)
    unwrap_count = run_cmd("grep -r '\\.unwrap()' mythrax-core/src/ | wc -l")
    expect_count = run_cmd("grep -r '\\.expect(' mythrax-core/src/ | wc -l")
    question_count = run_cmd("grep -r '\\?' mythrax-core/src/ | wc -l")
    match_count = run_cmd("grep -r 'match ' mythrax-core/src/ | wc -l")

    metrics['inconsistent_errors'] = (int(unwrap_count) if unwrap_count else 0) + (int(expect_count) if expect_count else 0) + (int(question_count) if question_count else 0) + (int(match_count) if match_count else 0)

    # Check magic numbers
    # A simple heuristic: variable assignments or function calls with literal numbers (excluding 0, 1)
    magic = run_cmd("grep -rE '[^a-zA-Z0-9_]([2-9]|[1-9][0-9]+)[^a-zA-Z0-9_\\.]' mythrax-core/src/ | grep -v 'test' | wc -l")
    metrics['magic_numbers'] = int(magic) if magic else 0

    # Check duplicated structs/enums
    # A simple heuristic to check duplicated names
    structs = run_cmd("grep -rohE 'struct [A-Za-z0-9_]+' mythrax-core/src/ | sort | uniq -d | wc -l")
    enums = run_cmd("grep -rohE 'enum [A-Za-z0-9_]+' mythrax-core/src/ | sort | uniq -d | wc -l")
    metrics['duplicated_types'] = (int(structs) if structs else 0) + (int(enums) if enums else 0)

    # Per-file debt density calculation (to flag increasing density)
    file_debt = {}
    file_debt_output = run_cmd("grep -rcE '#\\[allow(dead_code)\\]|TODO|FIXME|HACK|TEMP|\\.unwrap\\(\\)|\\.expect\\(' mythrax-core/src/ scripts/")
    for line in file_debt_output.split('\n'):
        if ':' in line:
            parts = line.split(':')
            filename = ':'.join(parts[:-1])
            count = int(parts[-1])
            file_debt[filename] = count

    metrics['file_debt'] = file_debt

    return metrics

def main():
    if not os.path.exists("mythrax-core/Cargo.toml"):
        print("Must be run from repo root.")
        return

    commits_info = get_commits()
    commits = [c.split('|')[0] for c in commits_info]
    messages = [c.split('|')[1] for c in commits_info]

    results = {}

    # Need to keep track of current branch
    current_branch = run_cmd("git rev-parse --abbrev-ref HEAD")
    if current_branch == "HEAD":
        current_branch = run_cmd("git rev-parse HEAD")

    for i, commit in enumerate(commits):
        print(f"Analyzing {commit} ({messages[i]})")
        # Ensure working tree is clean before checkout
        run_cmd("git restore .")
        run_cmd(f"git checkout {commit} --quiet")
        if os.path.exists("mythrax-core/Cargo.toml"):
            results[commit] = analyze_commit(commit)
        else:
            results[commit] = {k: 0 for k in ['dead_code', 'orphaned_todos', 'high_complexity_funcs', 'inconsistent_errors', 'magic_numbers', 'duplicated_types']}
            results[commit]['file_debt'] = {}

    run_cmd(f"git checkout {current_branch} --quiet")

    report = "## Architectural Sanitation Scorecard\n\n"
    report += "| Commit | Dead Code Suppressions | Orphaned TODOs | High Complexity (>15) | Error Handling Checks | Magic Numbers | Duplicated Types |\n"
    report += "|---|---|---|---|---|---|---|\n"

    for commit in commits:
        res = results[commit]
        report += f"| {commit} | {res['dead_code']} | {res['orphaned_todos']} | {res['high_complexity_funcs']} | {res['inconsistent_errors']} | {res['magic_numbers']} | {res['duplicated_types']} |\n"

    # Determine trajectory
    head_res = results[commits[0]]
    oldest_res = results[commits[-1]]

    degrading = []
    if head_res['dead_code'] > oldest_res['dead_code']:
         degrading.append("Dead code suppressions are increasing.")
    if head_res['high_complexity_funcs'] > oldest_res['high_complexity_funcs']:
         degrading.append("Cognitive complexity is increasing.")
    if head_res['orphaned_todos'] > oldest_res['orphaned_todos']:
         degrading.append("Orphaned TODOs are increasing.")
    if head_res['inconsistent_errors'] > oldest_res['inconsistent_errors']:
         degrading.append("Error handling complexity (unwrap/expect/?/match) is increasing.")
    if head_res['magic_numbers'] > oldest_res['magic_numbers']:
         degrading.append("Magic numbers are increasing.")
    if head_res['duplicated_types'] > oldest_res['duplicated_types']:
         degrading.append("Duplicated types are increasing.")

    # Check increasing file debt
    increasing_files = []
    head_files = head_res.get('file_debt', {})
    oldest_files = oldest_res.get('file_debt', {})
    for f, count in head_files.items():
        if count > oldest_files.get(f, 0):
            increasing_files.append(f)

    if increasing_files:
        degrading.append(f"Debt density is increasing in {len(increasing_files)} files: {', '.join(increasing_files[:3])}{'...' if len(increasing_files) > 3 else ''}")

    if degrading:
        report += "\n### ⚠️ Trajectory Warning\n"
        for d in degrading:
            report += f"- {d}\n"
    else:
        report += "\n### ✅ Trajectory Stable/Improving\n"

    print(report)

    with open("scorecard.md", "w") as f:
        f.write(report)

    # Post PR comment if we are on a PR and gh is available
    is_github_actions = os.environ.get("GITHUB_ACTIONS") == "true"
    if is_github_actions and shutil.which('gh'):
        pr_name = os.environ.get("GITHUB_REF_NAME", "")
        # Try to post to PR
        run_cmd(f"gh pr comment {pr_name} --body-file scorecard.md || echo 'Not a PR or gh failed'")

if __name__ == "__main__":
    main()
