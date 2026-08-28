#!/usr/bin/env python3
import re
import os
import subprocess

def get_tracked_todos(commit=None):
    if commit:
        try:
            res = subprocess.run(["git", "show", f"{commit}:TODO.md"], capture_output=True, text=True)
            if res.returncode == 0:
                content = res.stdout
            else:
                return []
        except:
            return []
    else:
        if not os.path.exists('TODO.md'):
            return []
        with open('TODO.md', 'r') as f:
            content = f.read()
    todos = []
    for line in content.split('\n'):
        if line.strip().startswith('- [ ] '):
            todos.append(line.strip()[6:].lower())
    return todos

def analyze_content(filepath, content, tracked_todos):
    metrics = {
        'dead_code_allows': 0,
        'unused_imports': 0,
        'unreachable_code': 0,
        'orphaned_todos': 0,
        'complex_functions': 0,
        'unwrap_expect': 0,
        'magic_numbers': 0,
        'total_debt': 0
    }

    # Annotations
    metrics['dead_code_allows'] = len(re.findall(r'#\[allow\(dead_code\)\]', content))
    metrics['unused_imports'] = len(re.findall(r'#\[allow\(unused_imports\)\]', content))
    metrics['unreachable_code'] = len(re.findall(r'#\[allow\(unreachable_code\)\]', content))

    # Orphaned TODOs
    todo_matches = re.findall(r'(?:#|//)\s*(?:TODO|FIXME|HACK|TEMP):?\s*(.*)', content, re.IGNORECASE)
    for todo in todo_matches:
        todo_text = todo.strip().lower()
        is_tracked = False
        for tracked in tracked_todos:
            if tracked[:20] in todo_text or todo_text[:20] in tracked:
                is_tracked = True
                break
        if not is_tracked:
            metrics['orphaned_todos'] += 1

    # Cyclomatic complexity approximation
    if filepath.endswith('.rs'):
        # Just count control flow keywords per function
        # A more robust regex that doesn't stop at first '}'
        func_blocks = re.findall(r'fn\s+\w+\s*(?:<[^>]+>)?\s*\([^)]*\)\s*(?:->\s*[^{]+)?\s*\{((?:[^{}]*|\{[^{}]*\})*)\}', content)
        for block in func_blocks:
            complexity = 1 + len(re.findall(r'\b(if|match|while|for|loop|\?)\b', block))
            if complexity > 15:
                metrics['complex_functions'] += 1

    # Error handling inconsistency
    unwraps = len(re.findall(r'\.unwrap\(\)', content))
    expects = len(re.findall(r'\.expect\(', content))
    questions = len(re.findall(r'\?', content))
    matches = len(re.findall(r'\bmatch\b', content))
    if (unwraps + expects) > 0 and (questions + matches) > 0:
        metrics['unwrap_expect'] = unwraps + expects

    # Magic numbers and string literals
    if filepath.endswith('.rs'):
        magic = len(re.findall(r'(?<![a-zA-Z0-9_])\d+(?:\.\d+)?(?![a-zA-Z0-9_])', content))
        strings = len(re.findall(r'"[^"]*"', content))
        zeros = len(re.findall(r'(?<![a-zA-Z0-9_])0(?![a-zA-Z0-9_])', content))
        ones = len(re.findall(r'(?<![a-zA-Z0-9_])1(?![a-zA-Z0-9_])', content))
        metrics['magic_numbers'] = max(0, magic + strings - zeros - ones)

    metrics['total_debt'] = sum(metrics.values())
    return metrics

def get_commit_files_metrics(commit):
    try:
        res = subprocess.run(["git", "ls-tree", "-r", "--name-only", commit], capture_output=True, text=True, check=True)
        files = res.stdout.strip().split('\n')
    except:
        return {}, {}

    metrics_by_file = {}
    structs_by_file = {}
    tracked_todos = get_tracked_todos(commit)

    for f in files:
        if not f.endswith(('.rs', '.py', '.sh')):
            continue
        # Restrict to mythrax-core and scripts
        if not (f.startswith('mythrax-core/') or f.startswith('scripts/')):
            continue

        try:
            content_res = subprocess.run(["git", "show", f"{commit}:{f}"], capture_output=True, text=True)
            if content_res.returncode == 0:
                content = content_res.stdout
                metrics = analyze_content(f, content, tracked_todos)
                metrics_by_file[f] = metrics

                # Extract structs/enums for global duplication check
                structs = re.findall(r'(?:struct|enum)\s+(\w+)', content)
                if structs:
                    structs_by_file[f] = structs
        except:
            pass

    # Calculate global duplication
    all_structs = []
    for f, structs in structs_by_file.items():
        all_structs.extend(structs)

    dup_counts = {}
    for s in all_structs:
        dup_counts[s] = dup_counts.get(s, 0) + 1

    for f, structs in structs_by_file.items():
        dups = sum(1 for s in structs if dup_counts[s] > 1)
        if dups > 0:
            metrics_by_file[f]['duplication_flags'] = dups
            metrics_by_file[f]['total_debt'] += dups
        else:
            metrics_by_file[f]['duplication_flags'] = 0

    return metrics_by_file

def run():
    try:
        res = subprocess.run(["git", "log", "--format=%H", "-n", "6"], capture_output=True, text=True, check=True)
        commits = res.stdout.strip().split('\n')
    except Exception as e:
        print("Error getting commits:", e)
        return

    commit_metrics = {}
    for commit in commits:
        commit_metrics[commit] = get_commit_files_metrics(commit)

    history = []
    for commit in commits:
        total = sum(f['total_debt'] for f in commit_metrics[commit].values())
        history.append({'commit': commit[:7], 'total_debt': total})

    current_commit = commits[0]
    prev_commit = commits[1] if len(commits) > 1 else None

    current_files = commit_metrics[current_commit]
    prev_files = commit_metrics[prev_commit] if prev_commit else {}

    increasing_debt_files = []
    for f, metrics in current_files.items():
        curr_debt = metrics['total_debt']
        prev_debt = prev_files.get(f, {}).get('total_debt', 0)
        if curr_debt > prev_debt:
            increasing_debt_files.append((f, prev_debt, curr_debt))

    with open('scorecard.md', 'w') as out:
        out.write("# Code Sanitation Scorecard\n\n")
        out.write("## Debt Trajectory (Last 6 Commits)\n\n")
        out.write("| Commit | Total Debt Density |\n")
        out.write("|--------|--------------------|\n")
        for h in history:
            out.write(f"| `{h['commit']}` | {h['total_debt']} |\n")

        out.write("\n## Current Top Debt Files\n\n")
        sorted_files = sorted(current_files.items(), key=lambda x: x[1]['total_debt'], reverse=True)
        out.write("| File | Total Debt | Dead Code | Orphaned TODOs | Complex Fns | Unwrap/Expect | Magic Numbers | Duplication |\n")
        out.write("|------|------------|-----------|----------------|-------------|---------------|---------------|-------------|\n")
        for f, m in sorted_files[:10]:
            if m['total_debt'] > 0:
                out.write(f"| `{f}` | {m['total_debt']} | {m['dead_code_allows']} | {m['orphaned_todos']} | {m['complex_functions']} | {m['unwrap_expect']} | {m['magic_numbers']} | {m.get('duplication_flags', 0)} |\n")

        if increasing_debt_files:
            out.write("\n## Files with Increasing Debt\n\n")
            for f, prev, curr in increasing_debt_files:
                out.write(f"- `{f}`: Debt increased from {prev} to {curr}\n")

if __name__ == '__main__':
    run()
