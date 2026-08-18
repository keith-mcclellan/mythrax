#!/usr/bin/env python3
import os
import sys
import re
import subprocess

def get_commits():
    result = subprocess.run(['git', 'log', '-n', '6', '--format=%H'], capture_output=True, text=True)
    if result.returncode != 0:
        return []
    return result.stdout.strip().split('\n')

def get_files_at_commit(commit):
    result = subprocess.run(['git', 'ls-tree', '-r', '--name-only', commit], capture_output=True, text=True)
    if result.returncode != 0:
        return []
    files = result.stdout.strip().split('\n')
    valid_files = []
    for f in files:
        if (f.startswith('mythrax-core/') and f.endswith('.rs')) or (f.startswith('scripts/') and (f.endswith('.py') or f.endswith('.sh'))):
            valid_files.append(f)
    return valid_files

def get_file_content(commit, path):
    result = subprocess.run(['git', 'show', f'{commit}:{path}'], capture_output=True, text=True)
    if result.returncode == 0:
        return result.stdout
    return ""

def get_todo_md(commit):
    result = subprocess.run(['git', 'show', f'{commit}:TODO.md'], capture_output=True, text=True)
    if result.returncode == 0:
        return result.stdout
    return ""

def analyze_code(content, todo_md_content, path):
    metrics = {
        'debt_suppression': 0,
        'orphaned_todos': 0,
        'complex_functions': 0,
        'inconsistent_error_handling': 0,
        'structs': [],
        'enums': [],
        'magic_numbers': 0
    }

    # 1. Debt suppression
    suppressions = re.findall(r'#\[allow\([^\]]*(dead_code|unused_imports|unreachable_code)[^\]]*\)\]', content)
    metrics['debt_suppression'] = len(suppressions)

    # 2. TODOs
    todos = re.findall(r'//\s*(TODO|FIXME|HACK|TEMP):?\s*(.*)', content, re.IGNORECASE)
    todo_lines = [line.strip().lower() for line in todo_md_content.split('\n') if line.strip()]
    for tag, desc in todos:
        desc_lower = desc.strip().lower()
        found = False
        if desc_lower:
            for t_line in todo_lines:
                if desc_lower in t_line:
                    found = True
                    break
        if not found:
            metrics['orphaned_todos'] += 1

    # 3. Cyclomatic complexity
    in_function = False
    brace_count = 0
    current_func = ""
    lines = content.split('\n')
    for line in lines:
        if not in_function:
            clean_line = re.sub(r'//.*', '', line)
            if re.search(r'\bfn\s+', clean_line):
                in_function = True
                brace_count = clean_line.count('{') - clean_line.count('}')
                current_func = line + '\n'
                if brace_count <= 0 and '{' in clean_line:
                    if _eval_complexity(current_func) > 15:
                        metrics['complex_functions'] += 1
                    in_function = False
                    current_func = ""
        else:
            clean_line = re.sub(r'//.*', '', line)
            current_func += line + '\n'
            brace_count += clean_line.count('{') - clean_line.count('}')
            if brace_count <= 0 and '{' in current_func:
                if _eval_complexity(current_func) > 15:
                    metrics['complex_functions'] += 1
                in_function = False
                current_func = ""

    # 4. Inconsistent error handling
    if path.endswith('.rs'):
        has_unwrap = 1 if '.unwrap()' in content else 0
        has_expect = 1 if '.expect(' in content else 0
        has_qmark = 1 if '?' in content else 0
        has_match = 1 if ('match ' in content and ('Ok(' in content or 'Err(' in content)) else 0

        if (has_unwrap + has_expect + has_qmark + has_match) > 1:
            metrics['inconsistent_error_handling'] = 1

    # 5. Struct/Enum definitions
    if path.endswith('.rs'):
        structs = re.findall(r'\bstruct\s+([A-Z]\w*)', content)
        enums = re.findall(r'\benum\s+([A-Z]\w*)', content)
        metrics['structs'].extend(structs)
        metrics['enums'].extend(enums)

    # 6. Magic numbers / strings
    magic_nums = re.findall(r'(?:==|!=|<|>|<=|>=|\+|-|\*|/)\s*(\d{2,})\b', content)
    metrics['magic_numbers'] = len(magic_nums)

    return metrics

def _eval_complexity(func_str):
    c = 1
    c += len(re.findall(r'\bif\b', func_str))
    c += len(re.findall(r'\bwhile\b', func_str))
    c += len(re.findall(r'\bfor\b', func_str))
    c += len(re.findall(r'\bmatch\b', func_str))
    c += len(re.findall(r'&&', func_str))
    c += len(re.findall(r'\|\|', func_str))
    c += len(re.findall(r'\?', func_str))
    return c

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        return

    report = []
    report.append("# Sanitation Scorecard")
    report.append("")
    report.append("Comparing the current commit to the last 5 commits to show trajectory.")
    report.append("")

    history = []

    for commit in commits:
        files = get_files_at_commit(commit)
        todo_md = get_todo_md(commit)

        commit_metrics = {
            'debt_suppression': 0,
            'orphaned_todos': 0,
            'complex_functions': 0,
            'inconsistent_error_handling': 0,
            'magic_numbers': 0,
            'file_metrics': {}
        }

        all_structs = []
        all_enums = []

        for f in files:
            content = get_file_content(commit, f)
            metrics = analyze_code(content, todo_md, f)

            commit_metrics['debt_suppression'] += metrics['debt_suppression']
            commit_metrics['orphaned_todos'] += metrics['orphaned_todos']
            commit_metrics['complex_functions'] += metrics['complex_functions']
            commit_metrics['inconsistent_error_handling'] += metrics['inconsistent_error_handling']
            commit_metrics['magic_numbers'] += metrics['magic_numbers']

            all_structs.extend(metrics['structs'])
            all_enums.extend(metrics['enums'])

            debt_score = (
                metrics['debt_suppression'] +
                metrics['orphaned_todos'] +
                metrics['complex_functions'] +
                metrics['inconsistent_error_handling'] +
                metrics['magic_numbers']
            )
            commit_metrics['file_metrics'][f] = debt_score

        struct_counts = {}
        for s in all_structs:
            struct_counts[s] = struct_counts.get(s, 0) + 1
        enum_counts = {}
        for e in all_enums:
            enum_counts[e] = enum_counts.get(e, 0) + 1

        duplicates = sum(1 for v in struct_counts.values() if v > 1) + sum(1 for v in enum_counts.values() if v > 1)
        commit_metrics['duplicated_types'] = duplicates

        history.append({
            'commit': commit[:7],
            'metrics': commit_metrics
        })

    report.append("## Trajectory")
    report.append("| Commit | Debt Suppressions | Orphaned TODOs | Complex Functions (>15) | Inconsistent Error Handling | Duplicated Types | Magic Numbers | Total Debt |")
    report.append("|--------|-------------------|----------------|-------------------------|-----------------------------|------------------|---------------|------------|")

    for h in history:
        m = h['metrics']
        total = (m['debt_suppression'] + m['orphaned_todos'] + m['complex_functions'] +
                 m['inconsistent_error_handling'] + m['duplicated_types'] + m['magic_numbers'])
        report.append(f"| {h['commit']} | {m['debt_suppression']} | {m['orphaned_todos']} | {m['complex_functions']} | {m['inconsistent_error_handling']} | {m['duplicated_types']} | {m['magic_numbers']} | {total} |")

    report.append("")
    report.append("## Increasing Debt Files")
    report.append("Files where debt density is increasing compared to previous commits:")

    if len(history) > 1:
        current_file_metrics = history[0]['metrics']['file_metrics']
        previous_file_metrics = history[1]['metrics']['file_metrics']

        increasing_files = []
        for f, score in current_file_metrics.items():
            prev_score = previous_file_metrics.get(f, 0)
            if score > prev_score:
                increasing_files.append((f, prev_score, score))

        if increasing_files:
            for f, p, c in increasing_files:
                report.append(f"- `{f}` (Debt increased from {p} to {c})")
        else:
            report.append("- No files showed an increase in debt density.")

    report_text = "\n".join(report)
    print(report_text)

    with open('scorecard.md', 'w') as f:
        f.write(report_text)

    if os.environ.get('GITHUB_ACTIONS') == 'true' or os.environ.get('GITHUB_ACTIONS') == '1':
        ref_name = os.environ.get('GITHUB_REF_NAME', '')
        if ref_name:
            print(f"Posting PR comment to {ref_name}...")
            subprocess.run(['gh', 'pr', 'comment', ref_name, '--body-file', 'scorecard.md'])
        else:
            print("GITHUB_REF_NAME not found, unable to post PR comment.")

if __name__ == '__main__':
    main()
