#!/usr/bin/env python3
import os
import re
import sys
import subprocess
import json
import collections

def get_todo_list():
    todos = set()
    try:
        with open("TODO.md", "r") as f:
            for line in f:
                if "- [" in line:
                    match = re.search(r'- \[[ x]\] (.*)', line)
                    if match:
                        todos.add(match.group(1).lower().strip())
    except FileNotFoundError:
        pass
    return todos

def is_in_todo_list(comment_text, todo_list):
    text = comment_text.lower().strip()
    for todo in todo_list:
        if text in todo or todo in text:
            return True
    return False

def analyze_rust_file(filepath, todo_list):
    results = {
        'dead_code_allows': 0,
        'orphaned_comments': 0,
        'unwrap_expect': 0,
        'magic_numbers': 0,
        'structs_enums': []
    }

    try:
        with open(filepath, 'r') as f:
            content = f.read()

        lines = content.split('\n')

        # very basic struct/enum extraction for duplication check
        # This will miss complex cases, but finds basic ones
        in_struct = False
        in_enum = False
        current_def = []

        for i, line in enumerate(lines):
            line_stripped = line.strip()

            # Dead code allows
            if '#[allow(dead_code)]' in line:
                results['dead_code_allows'] += 1

            # TODO, FIXME, HACK, TEMP
            match = re.search(r'//\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', line)
            if match:
                comment_text = match.group(2)
                if not is_in_todo_list(comment_text, todo_list):
                    results['orphaned_comments'] += 1

            # Inconsistent error handling (unwrap/expect vs ?)
            # Basic metric: counting unwrap/expect
            if '.unwrap(' in line or '.expect(' in line:
                results['unwrap_expect'] += 1

            # Magic numbers (very basic check, excluding 0, 1, and inside strings)
            # Find numbers that are not part of variable names, not 0 or 1, and not inside strings
            # This is a very rough heuristic
            # Filter out things that look like version numbers, hex, etc to reduce noise
            if not line_stripped.startswith('//') and not line_stripped.startswith('/*'):
                # strip out strings
                no_strings = re.sub(r'".*?"', '""', line)
                # Look for standalone numbers
                matches = re.findall(r'\b(?<![a-zA-Z_])([2-9]|\d{2,})\b', no_strings)
                # Filter out numbers used in common patterns like indexing, etc if possible
                results['magic_numbers'] += len(matches)

            # Struct/Enum duplication detection (very simple)
            if line_stripped.startswith('pub struct ') or line_stripped.startswith('struct '):
                in_struct = True
                current_def = [line_stripped]
            elif line_stripped.startswith('pub enum ') or line_stripped.startswith('enum '):
                in_enum = True
                current_def = [line_stripped]
            elif (in_struct or in_enum) and '{' in line_stripped:
                current_def.append(line_stripped)
            elif (in_struct or in_enum) and '}' in line_stripped:
                current_def.append(line_stripped)
                results['structs_enums'].append('\n'.join(current_def))
                in_struct = False
                in_enum = False
                current_def = []
            elif in_struct or in_enum:
                current_def.append(line_stripped)

    except Exception as e:
        print(f"Error analyzing {filepath}: {e}", file=sys.stderr)

    return results

def run_clippy():
    # Only check mythrax-core
    cwd = "mythrax-core"
    if not os.path.isdir(cwd):
        return 0, 0, 0

    metrics = {
        'complexity': 0,
        'dead_code': 0,
        'unused_imports': 0,
        'unreachable': 0
    }

    try:
        # Run clippy with json output to catch complexity and unused items
        result = subprocess.run(
            ['cargo', 'clippy', '--message-format=json', '--',
             '-W', 'clippy::cognitive_complexity',
             '-W', 'unused_imports',
             '-W', 'unreachable_code',
             '-W', 'dead_code'
            ],
            cwd=cwd,
            capture_output=True,
            text=True
        )

        for line in result.stdout.splitlines():
            if not line.strip():
                continue
            try:
                msg = json.loads(line)
                if msg.get('reason') == 'compiler-message':
                    message = msg.get('message', {})
                    code = message.get('code', {})
                    if code:
                        code_id = code.get('code', '')
                        if code_id == 'clippy::cognitive_complexity':
                            metrics['complexity'] += 1
                        elif code_id == 'dead_code':
                            metrics['dead_code'] += 1
                        elif code_id == 'unused_imports':
                            metrics['unused_imports'] += 1
                        elif code_id == 'unreachable_code':
                            metrics['unreachable'] += 1
            except json.JSONDecodeError:
                pass

    except Exception as e:
        print(f"Error running clippy: {e}", file=sys.stderr)

    return metrics['complexity'], metrics['dead_code'] + metrics['unreachable'], metrics['unused_imports']

def get_current_metrics():
    todo_list = get_todo_list()

    total_metrics = {
        'dead_code_allows': 0,
        'orphaned_comments': 0,
        'unwrap_expect': 0,
        'magic_numbers': 0,
        'complexity_violations': 0,
        'dead_unreachable_code': 0,
        'unused_imports': 0,
        'duplicate_structs_enums': 0
    }

    file_metrics = {}
    all_structs_enums = collections.defaultdict(list)

    if os.path.isdir("mythrax-core/src"):
        for root, dirs, files in os.walk("mythrax-core/src"):
            for file in files:
                if file.endswith(".rs"):
                    filepath = os.path.join(root, file)
                    metrics = analyze_rust_file(filepath, todo_list)

                    file_totals = {
                        'dead_code_allows': metrics['dead_code_allows'],
                        'orphaned_comments': metrics['orphaned_comments'],
                        'unwrap_expect': metrics['unwrap_expect'],
                        'magic_numbers': metrics['magic_numbers']
                    }
                    file_metrics[filepath] = file_totals

                    total_metrics['dead_code_allows'] += metrics['dead_code_allows']
                    total_metrics['orphaned_comments'] += metrics['orphaned_comments']
                    total_metrics['unwrap_expect'] += metrics['unwrap_expect']
                    total_metrics['magic_numbers'] += metrics['magic_numbers']

                    for item in metrics['structs_enums']:
                        # very basic normalization for duplicate detection
                        normalized = re.sub(r'\s+', ' ', item).strip()
                        all_structs_enums[normalized].append(filepath)

    # Count duplicates
    duplicates = 0
    for item, paths in all_structs_enums.items():
        if len(paths) > 1:
            duplicates += len(paths) - 1

    total_metrics['duplicate_structs_enums'] = duplicates

    # Run clippy for complexity and unused items
    complexity, dead, unused = run_clippy()
    total_metrics['complexity_violations'] = complexity
    total_metrics['dead_unreachable_code'] = dead
    total_metrics['unused_imports'] = unused

    return total_metrics, file_metrics

def get_commit_hash(offset):
    try:
        result = subprocess.run(['git', 'rev-parse', f'HEAD~{offset}'], capture_output=True, text=True, check=True)
        return result.stdout.strip()
    except subprocess.CalledProcessError:
        return None

def checkout_commit(commit_hash):
    subprocess.run(['git', 'checkout', '-q', commit_hash], check=True)

def calculate_debt_score(metrics):
    # A simple weighted sum for a "debt score"
    score = (
        metrics['dead_code_allows'] * 5 +
        metrics['orphaned_comments'] * 2 +
        metrics['unwrap_expect'] * 1 +
        metrics['magic_numbers'] * 1 +
        metrics['complexity_violations'] * 10 +
        metrics['dead_unreachable_code'] * 5 +
        metrics['unused_imports'] * 2 +
        metrics['duplicate_structs_enums'] * 5
    )
    return score

def main():
    if not os.path.isdir("mythrax-core"):
        print("Please run this script from the repository root.", file=sys.stderr)
        sys.exit(1)

    print("Gathering metrics for current commit...", file=sys.stderr)
    current_metrics, current_file_metrics = get_current_metrics()
    current_score = calculate_debt_score(current_metrics)

    history_scores = []

    # Try to get history if we're in a git repo
    is_git = os.path.isdir(".git")
    original_commit = None

    if is_git:
        try:
            original_commit_result = subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True)
            if original_commit_result.returncode == 0:
                original_commit = original_commit_result.stdout.strip()

                print("Gathering metrics for previous commits...", file=sys.stderr)
                for i in range(1, 6):
                    commit_hash = get_commit_hash(i)
                    if commit_hash:
                        checkout_commit(commit_hash)
                        metrics, _ = get_current_metrics()
                        history_scores.append(calculate_debt_score(metrics))

                # Restore original commit
                checkout_commit(original_commit)
        except Exception as e:
            print(f"Error gathering history: {e}", file=sys.stderr)
            if original_commit:
                checkout_commit(original_commit)

    # Generate report
    report = ["## Sanitation Scorecard", ""]

    report.append("### Current Metrics")
    report.append(f"- **Debt Score:** {current_score}")
    report.append(f"- `#[allow(dead_code)]`: {current_metrics['dead_code_allows']}")
    report.append(f"- Orphaned TODO/FIXME: {current_metrics['orphaned_comments']}")
    report.append(f"- Inconsistent Error Handling (`unwrap`/`expect`): {current_metrics['unwrap_expect']}")
    report.append(f"- Magic Numbers (approx): {current_metrics['magic_numbers']}")
    report.append(f"- Complexity Violations (>15): {current_metrics['complexity_violations']}")
    report.append(f"- Dead/Unreachable Code: {current_metrics['dead_unreachable_code']}")
    report.append(f"- Unused Imports: {current_metrics['unused_imports']}")
    report.append(f"- Duplicate Structs/Enums: {current_metrics['duplicate_structs_enums']}")
    report.append("")

    if history_scores:
        report.append("### Trajectory (Last 5 Commits)")
        # Reverse history so it's oldest to newest
        history_scores.reverse()
        history_scores.append(current_score)

        trajectory = " -> ".join(str(s) for s in history_scores)
        report.append(f"Score progression (oldest to newest): `{trajectory}`")

        if current_score > history_scores[-2]:
            report.append("⚠️ **Warning: Debt is increasing!**")
        elif current_score < history_scores[-2]:
            report.append("✅ **Good job: Debt is decreasing!**")
        else:
            report.append("⏸️ **Debt is stable.**")
        report.append("")

    # Flag files with high debt density (simple heuristic: score > 20)
    high_debt_files = []
    for filepath, metrics in current_file_metrics.items():
        file_score = (
            metrics['dead_code_allows'] * 5 +
            metrics['orphaned_comments'] * 2 +
            metrics['unwrap_expect'] * 1 +
            metrics['magic_numbers'] * 1
        )
        if file_score > 20:
            high_debt_files.append((filepath, file_score))

    if high_debt_files:
        report.append("### High Debt Files")
        high_debt_files.sort(key=lambda x: x[1], reverse=True)
        for filepath, score in high_debt_files[:10]:
            report.append(f"- `{filepath}`: Score {score}")

    report_text = "\n".join(report)
    print(report_text)

    # If running in a PR, append the comment
    if os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        try:
            # We can use gh pr comment in an action
            pr_number = os.environ.get("PR_NUMBER")
            if not pr_number:
                # Try to extract it from GITHUB_REF (refs/pull/123/merge)
                ref = os.environ.get("GITHUB_REF", "")
                parts = ref.split('/')
                if len(parts) >= 3 and parts[1] == "pull":
                    pr_number = parts[2]

            if pr_number:
                # Write report to a temp file to pass to gh
                with open("sanitation_report.md", "w") as f:
                    f.write(report_text)

                subprocess.run(
                    ["gh", "pr", "comment", pr_number, "-F", "sanitation_report.md"],
                    check=True
                )
                print("Successfully commented on PR.", file=sys.stderr)
            else:
                print("Could not determine PR number.", file=sys.stderr)
        except Exception as e:
            print(f"Error commenting on PR: {e}", file=sys.stderr)

if __name__ == "__main__":
    main()
