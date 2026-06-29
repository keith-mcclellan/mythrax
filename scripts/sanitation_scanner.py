#!/usr/bin/env python3

import os
import re
import subprocess
import sys
from collections import defaultdict
import json

def get_commits(n=6):
    try:
        output = subprocess.check_output(['git', 'rev-list', 'HEAD', '-n', str(n)], text=True)
        return output.strip().split('\n')
    except Exception:
        return []

def get_file_content(commit, filepath):
    try:
        return subprocess.check_output(['git', 'show', f"{commit}:{filepath}"], text=True, stderr=subprocess.DEVNULL)
    except Exception:
        return ""

def get_files(commit):
    try:
        output = subprocess.check_output(['git', 'ls-tree', '-r', commit, '--name-only'], text=True)
        return [f for f in output.strip().split('\n') if (f.startswith('mythrax-core/') or f.startswith('scripts/')) and (f.endswith('.rs') or f.endswith('.sh') or f.endswith('.py'))]
    except Exception:
        return []

def get_todo_md(commit):
    return get_file_content(commit, 'TODO.md')

def analyze_file(content, filename, todo_words):
    metrics = {
        'dead_code_suppressions': 0,
        'orphaned_todos': 0,
        'complexity_violations': 0, # functions > 15
        'unwraps_expects': 0,
        'magic_numbers': 0,
        'duplicate_structs': [],
        'debt_score': 0
    }

    structs = []

    lines = content.split('\n')
    in_function = False
    func_complexity = 0

    for i, line in enumerate(lines):
        if '#[allow(dead_code)]' in line:
            metrics['dead_code_suppressions'] += 1
            metrics['debt_score'] += 2

        todo_match = re.search(r'//\s*(TODO|FIXME|HACK|TEMP):?(.*)', line)
        if todo_match:
            comment_text = todo_match.group(2).lower()
            words = set(re.findall(r'[a-z]{4,}', comment_text))
            if not words.intersection(todo_words) and len(words) > 0:
                metrics['orphaned_todos'] += 1
                metrics['debt_score'] += 1

        metrics['unwraps_expects'] += line.count('.unwrap()') + line.count('.expect(')
        metrics['debt_score'] += line.count('.unwrap()') + line.count('.expect(')

        struct_match = re.search(r'^\s*(?:pub\s+)?(?:struct|enum)\s+([A-Z][a-zA-Z0-9_]*)', line)
        if struct_match:
            structs.append(struct_match.group(1))

        if not line.strip().startswith('const') and not line.strip().startswith('static'):
            if re.search(r'(?:==|!=|\+|-|\*|/|<=|>=|<|>)\s*\d{2,}', line):
                metrics['magic_numbers'] += 1
                metrics['debt_score'] += 0.5

        if re.search(r'fn\s+[a-z_]+\s*\(', line):
            in_function = True
            func_complexity = 1
        elif in_function:
            func_complexity += len(re.findall(r'\b(if|else|while|for|match|loop)\b', line))
            func_complexity += line.count('&&') + line.count('||') + line.count('?')
            if func_complexity > 15:
                metrics['complexity_violations'] += 1
                metrics['debt_score'] += 5
                in_function = False
                func_complexity = 0
            if line.strip() == '}':
                in_function = False
                func_complexity = 0

    metrics['duplicate_structs'] = structs
    return metrics

def analyze_commit(commit):
    files = get_files(commit)
    todo_content = get_todo_md(commit)
    todo_words = set(re.findall(r'[a-z]{4,}', todo_content.lower()))

    commit_metrics = {
        'total_debt': 0,
        'file_debts': defaultdict(float),
        'structs': defaultdict(list)
    }

    for f in files:
        content = get_file_content(commit, f)
        metrics = analyze_file(content, f, todo_words)

        commit_metrics['total_debt'] += metrics['debt_score']
        commit_metrics['file_debts'][f] = metrics['debt_score']

        for s in metrics['duplicate_structs']:
            commit_metrics['structs'][s].append(f)

    duplicate_score = 0
    for s, flist in commit_metrics['structs'].items():
        if len(flist) > 1:
            duplicate_score += (len(flist) - 1) * 3

    commit_metrics['total_debt'] += duplicate_score
    return commit_metrics

def main():
    commits = get_commits(6)
    if not commits:
        print("No commits found.")
        sys.exit(0)

    results = []
    for c in commits:
        results.append((c, analyze_commit(c)))

    results.reverse()

    scorecard = "## Sanitation Scorecard\n\n"
    scorecard += "| Commit | Total Debt Score | Trend |\n"
    scorecard += "|---|---|---|\n"

    prev_score = None
    for c, metrics in results:
        trend = "N/A"
        if prev_score is not None:
            if metrics['total_debt'] > prev_score:
                trend = "📉 Degrading"
            elif metrics['total_debt'] < prev_score:
                trend = "📈 Improving"
            else:
                trend = "➖ Stable"
        scorecard += f"| {c[:7]} | {metrics['total_debt']} | {trend} |\n"
        prev_score = metrics['total_debt']

    scorecard += "\n### Debt Density Increasing Files (Current Commit)\n"
    current_metrics = results[-1][1]

    if len(results) > 1:
        prev_metrics = results[-2][1]
        increasing_files = []
        for f, debt in current_metrics['file_debts'].items():
            prev_debt = prev_metrics['file_debts'].get(f, 0)
            if debt > prev_debt:
                increasing_files.append((f, prev_debt, debt))

        if increasing_files:
            for f, p, c in increasing_files:
                scorecard += f"- `{f}`: {p} -> {c} (+{c-p})\n"
        else:
            scorecard += "- No files with increasing debt density.\n"
    else:
        scorecard += "- Insufficient history to calculate trends.\n"

    scorecard += "\n### Duplicate Structs/Enums (Current Commit)\n"
    dupes = [s for s, flist in current_metrics['structs'].items() if len(flist) > 1]
    if dupes:
        for d in dupes:
            flist = current_metrics['structs'][d]
            scorecard += f"- `{d}` found in: {', '.join(flist)}\n"
    else:
        scorecard += "- No duplicates found.\n"

    print(scorecard)

    with open("sanitation_scorecard.md", "w") as f:
        f.write(scorecard)

if __name__ == "__main__":
    main()
