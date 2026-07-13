#!/usr/bin/env python3
import os
import json
import subprocess
import re
import sys
from collections import defaultdict
from pathlib import Path

# Required by constraints:
# - Run clippy --message-format=json to parse dead code, unused imports, unreachable branches, and cognitive complexity.
# - Scan for: `#[allow(dead_code)]`
# - Scan for: TODO, FIXME, HACK, TEMP comments -> check if in TODO.md
# - Inconsistent error handling in the same file: mixing unwrap, expect, ?, match.
# - Duplicate structs/enums.
# - Magic numbers / string literals.
# - Compare to 5 commits ago.

MYTHRAX_DIR = "mythrax-core"
SCRIPTS_DIR = "scripts"
TODO_MD_PATH = "TODO.md"
REPORT_PATH = "sanitation_scorecard.md"

def run_cmd(cmd, cwd=None):
    try:
        return subprocess.check_output(cmd, shell=True, cwd=cwd, stderr=subprocess.STDOUT).decode('utf-8')
    except subprocess.CalledProcessError as e:
        return e.output.decode('utf-8')

def get_git_file_content(filepath, commit="HEAD"):
    try:
        return subprocess.check_output(f"git show {commit}:{filepath}", shell=True, stderr=subprocess.DEVNULL).decode('utf-8')
    except Exception:
        return None

def extract_clippy_metrics():
    # Run clippy
    cmd = "cargo clippy --message-format=json"
    output = run_cmd(cmd, cwd=MYTHRAX_DIR)

    issues = defaultdict(int)
    files_with_issues = defaultdict(list)

    for line in output.splitlines():
        if not line.startswith('{'):
            continue
        try:
            msg = json.loads(line)
            if msg.get('reason') != 'compiler-message':
                continue

            msg_code = msg.get('message', {}).get('code')
            if not msg_code:
                continue

            code_id = msg_code.get('code')
            if not code_id:
                continue

            # Filter for specific debt types
            if code_id in ['dead_code', 'unused_imports', 'unreachable_code', 'clippy::cognitive_complexity']:
                issues[code_id] += 1

                spans = msg.get('message', {}).get('spans', [])
                if spans:
                    file_name = spans[0].get('file_name')
                    if file_name:
                        files_with_issues[file_name].append(code_id)
        except json.JSONDecodeError:
            pass

    return issues, files_with_issues

def parse_todos_from_md():
    todos = set()
    if os.path.exists(TODO_MD_PATH):
        with open(TODO_MD_PATH, 'r') as f:
            for line in f:
                # Naive word extraction, could be improved.
                words = re.findall(r'\b\w+\b', line.lower())
                todos.update(words)
    return todos

def analyze_file_content(content, filename):
    metrics = {
        'allow_dead_code': 0,
        'orphaned_todos': 0,
        'error_patterns': set(),
        'structs': [],
        'magic_numbers_or_strings': 0,
    }

    if not content:
        return metrics

    # 1. Allow dead code
    metrics['allow_dead_code'] = len(re.findall(r'#\[allow\(dead_code\)\]', content))

    # 2. TODO/FIXME/HACK/TEMP
    todo_matches = re.findall(r'(?i)\b(todo|fixme|hack|temp)\b', content)

    orphaned_count = 0
    # To satisfy prompt: cross-reference against TODO.md
    for line in content.splitlines():
        if re.search(r'(?i)\b(todo|fixme|hack|temp)\b', line):
            # Strip comments and check if in TODO.md
            clean_line = re.sub(r'^.*?//\s*', '', line).strip().lower()
            if clean_line and clean_line not in open(TODO_MD_PATH).read().lower() if os.path.exists(TODO_MD_PATH) else "":
                orphaned_count += 1
    metrics['orphaned_todos'] = orphaned_count

    # 3. Inconsistent error handling
    if '.unwrap()' in content: metrics['error_patterns'].add('unwrap')
    if '.expect(' in content: metrics['error_patterns'].add('expect')
    if '?' in content: metrics['error_patterns'].add('question_mark')
    if 'match ' in content and 'Err(' in content: metrics['error_patterns'].add('match_err')

    # 4. Duplicate structs/enums (we'll collect names)
    structs = re.findall(r'(?:struct|enum)\s+([A-Z][a-zA-Z0-9_]*)', content)
    metrics['structs'].extend(structs)

    # 5. Magic numbers and string literals
    # Look for numbers assigned or used in expressions that aren't 0, 1, or in const definitions
    # Simplistic regex for magic numbers and strings in code (not complete AST parsing)
    lines_without_const = [l for l in content.splitlines() if not l.strip().startswith('const ')]
    for line in lines_without_const:
        nums = re.findall(r'\b\d+\b', line)
        for num in nums:
            if num not in ['0', '1', '2']: # common acceptable numbers
                metrics['magic_numbers_or_strings'] += 1

        # very naive magic strings checker - strings not assigned to const
        strings = re.findall(r'"([^"]+)"', line)
        if strings and not line.strip().startswith('fn ') and not "!(" in line and not "#[" in line: # exclude println!, assert!, etc and attributes
           metrics['magic_numbers_or_strings'] += len(strings)

    return metrics

def analyze_codebase(commit="HEAD"):
    file_metrics = {}
    all_structs = []

    # Get files to analyze
    if commit == "HEAD":
        cmd = "find mythrax-core/src scripts -type f -name '*.rs' -o -name '*.py'"
        files = run_cmd(cmd).splitlines()
    else:
        cmd = f"git ls-tree -r {commit} --name-only | grep -E '^(mythrax-core/src|scripts)/.*\\.(rs|py)$'"
        try:
            files = run_cmd(cmd).splitlines()
        except Exception:
            files = []

    for f in files:
        if not f: continue

        # normalize path
        f = f.strip()

        if commit == "HEAD":
            with open(f, 'r') as file:
                content = file.read()
        else:
            content = get_git_file_content(f, commit)

        if not content: continue

        m = analyze_file_content(content, f)
        file_metrics[f] = m
        all_structs.extend(m['structs'])

    # Calculate duplicates
    struct_counts = defaultdict(int)
    for s in all_structs:
        struct_counts[s] += 1
    duplicates = {k: v for k, v in struct_counts.items() if v > 1}

    total_debt = 0
    inconsistent_files = 0

    # Compute base debt from file contents
    for f, m in file_metrics.items():
        debt = m['allow_dead_code'] + m['orphaned_todos'] + m['magic_numbers_or_strings']
        if len(m['error_patterns']) > 2:
            inconsistent_files += 1
            debt += 10 # penalty
        m['total_debt'] = debt
        total_debt += debt

    # Add duplicate struct penalties
    total_debt += sum(count - 1 for count in duplicates.values()) * 5

    return {
        'file_metrics': file_metrics,
        'duplicates': duplicates,
        'total_debt': total_debt,
        'inconsistent_error_files': inconsistent_files
    }

def main():
    print("Running current clippy scan...")
    current_clippy_issues, current_clippy_files = extract_clippy_metrics()

    print("Analyzing current codebase metrics...")
    current_metrics = analyze_codebase("HEAD")

    # Incorporate current clippy metrics into the total debt
    for f, issues in current_clippy_files.items():
        full_path = f"mythrax-core/{f}" # adjust path
        if full_path in current_metrics['file_metrics']:
            current_metrics['file_metrics'][full_path]['total_debt'] += len(issues)
        current_metrics['total_debt'] += len(issues)

    print("Analyzing HEAD~5 codebase metrics...")
    past_metrics = analyze_codebase("HEAD~5")
    # Note: we can't easily run clippy on HEAD~5 in a lightweight script without a full checkout
    # but we can try to checkout HEAD~5, run clippy, then checkout back.
    # To accurately calculate HEAD~5 debt trajectory including clippy:
    try:
        run_cmd("git checkout HEAD~5", cwd=MYTHRAX_DIR) # this works because mythrax-core is part of the git repo
        past_clippy_issues, past_clippy_files = extract_clippy_metrics()
        for f, issues in past_clippy_files.items():
            full_path = f"mythrax-core/{f}"
            if full_path in past_metrics['file_metrics']:
                past_metrics['file_metrics'][full_path]['total_debt'] += len(issues)
            past_metrics['total_debt'] += len(issues)
    finally:
        run_cmd("git checkout -", cwd=MYTHRAX_DIR) # revert to previous branch

    # Generate Report
    report = ["# 🧹 Sanitation Scorecard\n"]

    # Trend
    trend = "DEGRADING 📉" if current_metrics['total_debt'] > past_metrics['total_debt'] else "IMPROVING 📈"
    report.append(f"**Trajectory:** {trend}")
    report.append(f"- Current Debt Score: {current_metrics['total_debt']}")
    if past_metrics['total_debt'] > 0 or current_metrics['total_debt'] > 0:
        report.append(f"- Previous Debt Score (HEAD~5): {past_metrics['total_debt']}")

    report.append("\n## 🦀 Clippy Findings (Current)")
    for issue, count in current_clippy_issues.items():
        report.append(f"- **{issue}**: {count}")

    report.append("\n## 🚩 Code Debt Analysis")
    report.append(f"- **Inconsistent Error Handling Files**: {current_metrics['inconsistent_error_files']}")
    if current_metrics['duplicates']:
        report.append(f"- **Potentially Duplicated Structs/Enums**: {len(current_metrics['duplicates'])}")

    report.append("\n## 🚨 Files with Increasing Debt Density")
    increasing_files = []
    for f, c_m in current_metrics['file_metrics'].items():
        p_m = past_metrics['file_metrics'].get(f, {'total_debt': 0})
        if c_m['total_debt'] > p_m['total_debt']:
            increasing_files.append((f, p_m['total_debt'], c_m['total_debt']))

    if increasing_files:
        for f, old, new in increasing_files:
            report.append(f"- `{f}`: {old} -> {new}")
    else:
        report.append("- *No files with increasing debt density.*")

    with open(REPORT_PATH, 'w') as f:
        f.write('\n'.join(report))

    print(f"Report generated at {REPORT_PATH}")

if __name__ == "__main__":
    main()
