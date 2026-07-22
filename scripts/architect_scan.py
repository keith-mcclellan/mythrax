import os
import subprocess
import json
import re

def run_cmd(cmd, cwd=None):
    try:
        return subprocess.check_output(cmd, shell=True, cwd=cwd, text=True, stderr=subprocess.DEVNULL)
    except subprocess.CalledProcessError as e:
        return e.output

def get_allow_dead_code_count():
    try:
        out = run_cmd("grep -rn 'allow(dead_code)' mythrax-core/src scripts 2>/dev/null")
        return len([line for line in out.split('\n') if line.strip()])
    except:
        return 0

def get_orphaned_todos():
    try:
        with open("TODO.md", "r") as f:
            todo_content = f.read().lower()
    except:
        todo_content = ""

    out = run_cmd("grep -rnE '(TODO|FIXME|HACK|TEMP)' mythrax-core/src scripts 2>/dev/null")
    orphaned_count = 0

    for line in out.split('\n'):
        if not line.strip(): continue
        # very simple heuristic: extract the text after TODO/FIXME/etc.
        match = re.search(r'(TODO|FIXME|HACK|TEMP)(:)?(.*)', line, re.IGNORECASE)
        if match:
            comment_text = match.group(3).strip().lower()
            if not comment_text:
                orphaned_count += 1
            else:
                words = comment_text.split()
                # Use a sliding window of 3 words to match against TODO.md
                if len(words) < 3:
                    if comment_text not in todo_content:
                        orphaned_count += 1
                else:
                    chunk = " ".join(words[:3])
                    if chunk not in todo_content:
                        orphaned_count += 1
    return orphaned_count

def run_clippy():
    # Ensure configuration exists for 15 limit
    with open("mythrax-core/clippy.toml", "w") as f:
        f.write("cognitive-complexity-threshold = 15\n")

    cmd = "cd mythrax-core && cargo clippy --message-format=json --lib -- -W clippy::cognitive_complexity -W clippy::unwrap_used -W clippy::expect_used -W dead_code -W unused_imports -W unreachable_code 2>/dev/null"
    out = run_cmd(cmd)

    stats = {"complexity": 0, "unwrap_expect": 0, "dead_code": 0}
    file_debt = {}

    for line in out.split('\n'):
        if not line.strip(): continue
        try:
            msg = json.loads(line)
            if msg.get("reason") == "compiler-message":
                code_id = msg.get("message", {}).get("code", {}).get("code", "")

                spans = msg.get("message", {}).get("spans", [])
                file_name = spans[0].get("file_name", "unknown") if spans else "unknown"
                if "registry" in file_name or "unknown" in file_name:
                    continue

                if file_name not in file_debt:
                    file_debt[file_name] = 0

                if code_id == "clippy::cognitive_complexity":
                    stats["complexity"] += 1
                    file_debt[file_name] += 1
                elif code_id in ("clippy::unwrap_used", "clippy::expect_used"):
                    stats["unwrap_expect"] += 1
                    file_debt[file_name] += 1
                elif code_id in ("dead_code", "unused_imports", "unreachable_code"):
                    stats["dead_code"] += 1
                    file_debt[file_name] += 1
        except:
            pass

    return stats, file_debt

def get_duplicates_and_magic():
    # Magic numbers (heuristically any let binding to a numeric literal > 1 or < -1 that isn't simple)
    # We'll just run a regex across src
    magic_out = run_cmd("grep -rnE 'let [a-zA-Z0-9_]+ = ([2-9]|[1-9][0-9]+);' mythrax-core/src 2>/dev/null")
    magic_count = len([x for x in magic_out.split('\n') if x.strip()])

    # Duplicated structs
    structs_out = run_cmd("grep -hE '^\\s*(pub )?struct ' $(find mythrax-core/src -name '*.rs') 2>/dev/null")
    struct_names = []
    for line in structs_out.split('\n'):
        if not line.strip(): continue
        parts = line.strip().split('struct ')
        if len(parts) > 1:
            name = parts[1].split()[0].split('<')[0].split('{')[0].strip()
            struct_names.append(name)

    duplicates = len(struct_names) - len(set(struct_names))
    return duplicates, magic_count

def analyze_commit(commit_hash):
    run_cmd(f"git checkout {commit_hash} --quiet")
    with open("mythrax-core/clippy.toml", "w") as f:
        f.write("cognitive-complexity-threshold = 15\n")

    allow_dead_code = get_allow_dead_code_count()
    orphaned_todos = get_orphaned_todos()
    clippy_stats, file_debt = run_clippy()
    dupes, magic = get_duplicates_and_magic()

    # Cleanup git state before we checkout the next commit
    run_cmd("git reset --hard HEAD && git clean -fd")

    return {
        "commit": commit_hash[:7],
        "dead_code": allow_dead_code + clippy_stats["dead_code"],
        "orphaned_todos": orphaned_todos,
        "complexity": clippy_stats["complexity"],
        "unwrap_expect": clippy_stats["unwrap_expect"],
        "duplicates": dupes,
        "magic_numbers": magic,
        "file_debt": file_debt
    }

def main():
    # Detect branch and PR context
    orig_branch = run_cmd("git branch --show-current").strip()
    if not orig_branch:
        orig_branch = os.environ.get("GITHUB_REF_NAME", "HEAD")

    commits = run_cmd("git log -n 6 --format='%H'").strip().split('\n')
    results = []

    try:
        for c in reversed(commits): # Oldest to newest
            print(f"Analyzing {c[:7]}...")
            res = analyze_commit(c)
            results.append(res)
    finally:
        run_cmd(f"git checkout {orig_branch} --quiet")

    # Write Scorecard
    with open("architect_scorecard.md", "w") as f:
        f.write("# Chief Architect Sanitation Scorecard\n\n")
        f.write("## Trajectory (Last 6 Commits)\n\n")
        f.write("| Commit | Dead Code | Orphaned TODOs | Complexity (>15) | `unwrap`/`expect` | Duplicated Structs | Magic Numbers |\n")
        f.write("|---|---|---|---|---|---|---|\n")

        for r in results:
            f.write(f"| {r['commit']} | {r['dead_code']} | {r['orphaned_todos']} | {r['complexity']} | {r['unwrap_expect']} | {r['duplicates']} | {r['magic_numbers']} |\n")

        first, last = results[0], results[-1]

        def diff_str(f_val, l_val):
            d = l_val - f_val
            return f"+{d}" if d > 0 else str(d)

        f.write("\n## Trajectory Analysis\n")
        f.write(f"- **Dead Code**: {diff_str(first['dead_code'], last['dead_code'])}\n")
        f.write(f"- **Orphaned TODOs**: {diff_str(first['orphaned_todos'], last['orphaned_todos'])}\n")
        f.write(f"- **Complexity Violations**: {diff_str(first['complexity'], last['complexity'])}\n")
        f.write(f"- **Error Handling Debt**: {diff_str(first['unwrap_expect'], last['unwrap_expect'])}\n")
        f.write(f"- **Duplicated Structs**: {diff_str(first['duplicates'], last['duplicates'])}\n")
        f.write(f"- **Magic Numbers**: {diff_str(first['magic_numbers'], last['magic_numbers'])}\n")

        f.write("\n## Files with Increasing Debt Density\n")
        increasing = []
        for fname, c_val in last['file_debt'].items():
            p_val = first['file_debt'].get(fname, 0)
            if c_val > p_val:
                increasing.append(f"- `{fname}`: {p_val} -> {c_val} (+{c_val - p_val})")

        if increasing:
            for item in increasing:
                f.write(f"{item}\n")
        else:
            f.write("- No files showed an increase in debt density.\n")

    # If triggered by a push event, append findings as PR comments.
    is_github_actions = os.environ.get("GITHUB_ACTIONS") == "true"
    github_event_name = os.environ.get("GITHUB_EVENT_NAME")

    # We will simulate appending PR comments by creating mock issue files
    # if it's a "push" event (which we assume for this task)
    os.makedirs("issues", exist_ok=True)
    with open("issues/architect_pr_comment.md", "w") as f:
        f.write("### Chief Architect Review 🚨\n\n")
        f.write("I have reviewed this PR for code sanitation and technical debt. As a reminder, there is **zero tolerance** for code that becomes a maintenance liability.\n\n")
        f.write("#### Debt Trajectory\n")
        f.write(f"- **Dead Code**: {diff_str(first['dead_code'], last['dead_code'])}\n")
        f.write(f"- **Orphaned TODOs**: {diff_str(first['orphaned_todos'], last['orphaned_todos'])}\n")
        f.write(f"- **Complexity**: {diff_str(first['complexity'], last['complexity'])}\n")
        f.write(f"- **Error Handling**: {diff_str(first['unwrap_expect'], last['unwrap_expect'])}\n\n")

        if increasing:
            f.write("#### Files with Increasing Debt Density\n")
            for item in increasing:
                f.write(f"{item}\n")
            f.write("\n**Action Required:** Please address the increasing technical debt in these files before merging. Temporary hacks and ignored warnings are not solutions.\n")
        else:
            f.write("\nThank you for keeping the codebase clean. No increasing debt density detected.\n")

if __name__ == "__main__":
    main()
