#!/usr/bin/env python3
import os
import re
import json
import subprocess
from collections import defaultdict
import argparse
import shutil

def run_cmd(cmd, cwd=None):
    result = subprocess.run(cmd, shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=cwd)
    return result.stdout

def scan_commit():
    debt = {
        'dead_code_suppressions': 0,
        'unsuppressed_dead_code': 0,
        'orphaned_todos': 0,
        'complexity_violations': 0,
        'error_handling_inconsistencies': 0,
        'duplicated_structs': 0,
        'magic_numbers': 0,
        'total': 0,
        'files': defaultdict(int)
    }

    # 1. Dead code suppressions
    out = run_cmd("grep -rnw -e 'allow(dead_code)' mythrax-core/src/ scripts/ || true")
    for line in out.splitlines():
        if line.strip():
            debt['dead_code_suppressions'] += 1
            filename = line.split(':')[0]
            debt['files'][filename] += 1
            debt['total'] += 1

    # 2. Orphaned TODOs
    todo_content = ""
    if os.path.exists("TODO.md"):
        with open("TODO.md", "r") as f:
            todo_content = f.read().lower()

    out = run_cmd("grep -rinE 'TODO|FIXME|HACK|TEMP' mythrax-core/src/ scripts/ || true")
    for line in out.splitlines():
        if line.strip():
            parts = line.split(':', 2)
            if len(parts) >= 3:
                filename, line_no, comment = parts[0], parts[1], parts[2]
                simplified = re.sub(r'[^a-zA-Z0-9]', ' ', comment).lower().strip()
                words = [w for w in simplified.split() if w not in ['todo', 'fixme', 'hack', 'temp', 'slash', 'comment']]
                if not words: continue
                # We check the entire snippet instead of discarding short ones.
                snippet = " ".join(words[:5])
                if snippet not in todo_content:
                    debt['orphaned_todos'] += 1
                    debt['files'][filename] += 1
                    debt['total'] += 1

    # 3. Clippy based checks
    clippy_cmd = "cargo clippy --message-format=json -- -W clippy::cognitive_complexity -W clippy::unwrap_used -W clippy::expect_used"
    clippy_out = run_cmd(clippy_cmd, cwd="mythrax-core")

    for line in clippy_out.splitlines():
        if not line.strip() or not line.startswith('{'): continue
        try:
            msg = json.loads(line)
            if msg.get('reason') == 'compiler-message':
                warn = msg['message']
                code = warn.get('code')
                if code:
                    code_id = code.get('code', '')
                    span = warn.get('spans', [{}])
                    if span:
                        filename = span[0].get('file_name', '')
                        if filename:
                            filename = f"mythrax-core/{filename}"
                        if code_id == 'clippy::cognitive_complexity':
                            debt['complexity_violations'] += 1
                            if filename: debt['files'][filename] += 1
                            debt['total'] += 1
                        elif code_id in ['clippy::unwrap_used', 'clippy::expect_used']:
                            debt['error_handling_inconsistencies'] += 1
                            if filename: debt['files'][filename] += 1
                            debt['total'] += 1
                        elif code_id in ['dead_code', 'unused_imports', 'unreachable_code']:
                            debt['unsuppressed_dead_code'] += 1
                            if filename: debt['files'][filename] += 1
                            debt['total'] += 1
        except json.JSONDecodeError:
            pass

    # 4. Duplicated structs
    # Skip common generic names like Error, Config, Result, Message, Response, Request
    skip_names = {'Error', 'Config', 'Result', 'Message', 'Response', 'Request', 'State', 'Event'}
    structs = defaultdict(list)
    out = run_cmd("grep -rnE '^(pub )?(struct|enum) [A-Z][a-zA-Z0-9_]*' mythrax-core/src/ || true")
    for line in out.splitlines():
        match = re.search(r'(struct|enum)\s+([A-Z][a-zA-Z0-9_]*)', line)
        if match:
            name = match.group(2)
            if name in skip_names:
                continue
            filename = line.split(':')[0]
            if filename not in structs[name]:
                structs[name].append(filename)

    for name, files in structs.items():
        if len(files) > 1:
            debt['duplicated_structs'] += (len(files) - 1)
            for f in files[1:]:
                debt['files'][f] += 1
                debt['total'] += 1

    # 5. Magic numbers and strings
    out = run_cmd("grep -rnE '(=|\\(|,)\\s*([2-9]|[1-9][0-9]+)(\\.[0-9]+)?' mythrax-core/src/ || true")
    for line in out.splitlines():
        if line.strip() and not line.strip().startswith('//'):
            debt['magic_numbers'] += 1
            filename = line.split(':')[0]
            debt['files'][filename] += 1
            debt['total'] += 1

    out = run_cmd("grep -rnE '(=|\\(|,)\\s*\"[^\"]+\"' mythrax-core/src/ | grep -vE '(println!|print!|format!|panic!|assert|log::|debug!|info!|warn!|error!|trace!)' || true")
    for line in out.splitlines():
        if line.strip() and not line.strip().startswith('//'):
            debt['magic_numbers'] += 1
            filename = line.split(':')[0]
            debt['files'][filename] += 1
            debt['total'] += 1

    return debt

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--commits', type=int, default=5)
    args = parser.parse_args()

    commits_out = run_cmd(f"git rev-list HEAD -n {args.commits + 1}")
    commits = commits_out.splitlines()

    if not commits:
        print("No commits found.")
        return

    history = []
    current_head = run_cmd("git rev-parse HEAD").strip()

    # Save a copy of clippy.toml so we can enforce the rule on older commits
    has_clippy = os.path.exists("mythrax-core/clippy.toml")
    if has_clippy:
        shutil.copy("mythrax-core/clippy.toml", "/tmp/clippy.toml.bak")

    # We will use stashing instead of hard reset to preserve dev local changes when run locally
    run_cmd("git stash --include-untracked")

    try:
        for commit in commits:
            run_cmd(f"git checkout {commit} --quiet")

            # Enforce current clippy rules in historical scan
            if has_clippy:
                shutil.copy("/tmp/clippy.toml.bak", "mythrax-core/clippy.toml")

            print(f"Scanning commit {commit[:7]}...")
            debt = scan_commit()
            history.append({
                'commit': commit[:7],
                'debt': debt
            })

            # Cleanup restored clippy before next checkout
            if has_clippy:
                run_cmd("git checkout mythrax-core/clippy.toml || rm -f mythrax-core/clippy.toml")
    finally:
        run_cmd(f"git checkout {current_head} --quiet")
        run_cmd("git stash pop || true")

    current = history[0]
    older = history[1:]

    report = ["## 🧹 Codebase Sanitation Scorecard\n"]
    report.append(f"**Current Commit:** `{current['commit']}`\n")
    report.append(f"**Total Debt Score:** {current['debt']['total']}\n")
    report.append("### Debt Breakdown\n")
    report.append(f"- Dead code suppressions: {current['debt']['dead_code_suppressions']}")
    report.append(f"- Unsuppressed dead code/unused: {current['debt']['unsuppressed_dead_code']}")
    report.append(f"- Orphaned TODOs: {current['debt']['orphaned_todos']}")
    report.append(f"- Complexity violations: {current['debt']['complexity_violations']}")
    report.append(f"- Inconsistent error handling: {current['debt']['error_handling_inconsistencies']}")
    report.append(f"- Duplicated structs/enums: {current['debt']['duplicated_structs']}")
    report.append(f"- Magic numbers/strings: {current['debt']['magic_numbers']}\n")

    if older:
        oldest = older[-1]
        diff = current['debt']['total'] - oldest['debt']['total']
        trajectory = "📉 Improving" if diff < 0 else ("📈 Degrading" if diff > 0 else "➡️ Stable")
        report.append(f"### Trajectory (Last {len(older)} commits)\n")
        report.append(f"Trend: {trajectory} (Total debt changed by {diff:+} from `{oldest['commit']}`)\n")

        increasing_files = []
        for f, count in current['debt']['files'].items():
            old_count = oldest['debt']['files'].get(f, 0)
            if count > old_count:
                increasing_files.append((f, count - old_count))

        if increasing_files:
            report.append("### 🚩 Files with Increasing Debt Density\n")
            for f, inc in increasing_files:
                report.append(f"- `{f}` (+{inc} violations)")

    report_text = "\n".join(report)
    print(report_text)

    with open("sanitation_report.md", "w") as f:
        f.write(report_text)

if __name__ == "__main__":
    main()
