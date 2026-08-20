#!/usr/bin/env python3
import os
import re
import sys
import subprocess
import shlex
from collections import defaultdict

def run_cmd(cmd):
    import shlex
    if isinstance(cmd, str):
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    else:
        result = subprocess.run(cmd, capture_output=True, text=True)
    return result.stdout.strip()

def get_commits(n=6):
    subprocess.run(["git", "fetch", "--unshallow"], capture_output=True)
    subprocess.run(["git", "fetch", "--all"], capture_output=True)
    commits = run_cmd(["git", "log", "--format=%H", "-n", str(n)]).split("\n")
    return [c for c in commits if c]

def get_files_at_commit(commit, paths):
    out = run_cmd(["git", "ls-tree", "-r", "--name-only", commit])
    files = out.split("\n")
    matched = []
    for f in files:
        if not f: continue
        if any(f.startswith(p) for p in paths) and (f.endswith('.rs') or f.endswith('.py') or f.endswith('.sh')):
            matched.append(f)
    return matched

def get_file_content(commit, path):
    return run_cmd(["git", "show", f"{commit}:{path}"])

def get_todo_md(commit):
    res = run_cmd(["git", "show", f"{commit}:TODO.md"])
    return res if not res.startswith("fatal:") else ""

def analyze_code(content, filename, todo_md_content):
    issues = {
        'dead_code_allow': 0,
        'orphaned_todos': 0,
        'complex_functions': 0,
        'mixed_error_handling': 0,
        'magic_numbers': 0,
        'duplicated_structs': 0,
    }

    allows = re.findall(r'#!?\[allow\((.*?)\)\]', content)
    for allow in allows:
        if 'dead_code' in allow or 'unused_imports' in allow or 'unreachable_code' in allow:
            issues['dead_code_allow'] += 1

    comments = re.findall(r'(?://|#)\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE)
    for c_type, text in comments:
        text = text.strip()
        if text and text not in todo_md_content:
            issues['orphaned_todos'] += 1

    structs = []
    enums = []

    if filename.endswith('.rs'):
        lines = content.split('\n')
        in_fn = False
        brace_depth = 0
        complexity = 1

        for line in lines:
            if re.search(r'let\s+(mut\s+)?[a-zA-Z_0-9]+\s*(:\s*[a-zA-Z_0-9_<>]+)?\s*=\s*\d+;', line):
                issues['magic_numbers'] += 1
            if re.search(r'let\s+(mut\s+)?[a-zA-Z_0-9]+\s*(:\s*[a-zA-Z_0-9_<>]+)?\s*=\s*"[^"]+";', line):
                issues['magic_numbers'] += 1

            if not in_fn:
                if re.search(r'\bfn\s+[a-zA-Z_0-9]+\s*\(', line) or re.search(r'\bfn\s+[a-zA-Z_0-9]+\s*<[^>]+>\(', line):
                    in_fn = True
                    brace_depth = line.count('{') - line.count('}')
                    complexity = 1 + len(re.findall(r'\b(if|while|for|loop|match)\b', line)) + line.count('&&') + line.count('||') + line.count('?')
            else:
                brace_depth += line.count('{') - line.count('}')
                complexity += len(re.findall(r'\b(if|while|for|loop|match)\b', line))
                complexity += line.count('&&') + line.count('||') + line.count('?')

                if brace_depth <= 0:
                    if complexity > 15:
                        issues['complex_functions'] += 1
                    in_fn = False

        has_unwrap = '.unwrap()' in content or '.expect(' in content
        has_question = '?' in content
        has_match = 'match ' in content
        styles = sum([has_unwrap, has_question, has_match])
        if styles > 1:
            issues['mixed_error_handling'] = 1

        structs = re.findall(r'struct\s+([a-zA-Z_0-9]+)\s*\{', content)
        enums = re.findall(r'enum\s+([a-zA-Z_0-9]+)\s*\{', content)

    return issues, structs, enums

def main():
    commits = get_commits(6)
    if not commits:
        print("No commits found.")
        return

    paths_to_scan = ['mythrax-core/', 'scripts/']
    history = []

    for commit in reversed(commits):
        files = get_files_at_commit(commit, paths_to_scan)
        todo_content = get_todo_md(commit)

        commit_totals = {
            'dead_code_allow': 0,
            'orphaned_todos': 0,
            'complex_functions': 0,
            'mixed_error_handling': 0,
            'magic_numbers': 0,
            'duplicated_structs': 0,
        }

        all_structs = []
        all_enums = []
        file_densities = {}

        for f in files:
            content = get_file_content(commit, f)
            issues, structs, enums = analyze_code(content, f, todo_content)

            all_structs.extend(structs)
            all_enums.extend(enums)

            debt_sum = sum(issues.values())
            file_densities[f] = debt_sum

            for k, v in issues.items():
                commit_totals[k] += v

        struct_counts = defaultdict(int)
        for s in all_structs: struct_counts[s] += 1
        enum_counts = defaultdict(int)
        for e in all_enums: enum_counts[e] += 1

        duplicates = sum(1 for v in struct_counts.values() if v > 1) + sum(1 for v in enum_counts.values() if v > 1)
        commit_totals['duplicated_structs'] = duplicates

        history.append({
            'commit': commit,
            'totals': commit_totals,
            'densities': file_densities,
        })

    scorecard = "# Sanitation Scorecard\n\n"
    scorecard += "## Trajectory (Last 6 Commits)\n\n"

    metrics = ['dead_code_allow', 'orphaned_todos', 'complex_functions', 'mixed_error_handling', 'magic_numbers', 'duplicated_structs']

    scorecard += "| Commit | " + " | ".join(metrics) + " | Total Debt |\n"
    scorecard += "|---" * (len(metrics) + 2) + "|\n"

    for h in history:
        commit_hash = h['commit'][:7]
        row = [str(h['totals'][m]) for m in metrics]
        total_debt = sum(h['totals'].values())
        scorecard += f"| {commit_hash} | {' | '.join(row)} | {total_debt} |\n"

    scorecard += "\n## File Debt Density (Degrading Files)\n\n"
    if len(history) >= 2:
        curr = history[-1]['densities']
        prev = history[-2]['densities']

        degraded = False
        for f, curr_debt in curr.items():
            prev_debt = prev.get(f, 0)
            if curr_debt > prev_debt:
                scorecard += f"- **{f}**: Debt increased from {prev_debt} to {curr_debt}\n"
                degraded = True

        if not degraded:
            scorecard += "- No files show increasing debt density in the latest commit.\n"

    print(scorecard)

    github_event_name = os.environ.get('GITHUB_EVENT_NAME')
    if github_event_name == 'push':
        # Safely pass scorecard block
        subprocess.run(["gh", "pr", "comment", "-b", scorecard])

if __name__ == '__main__':
    main()
