import os
import re
import subprocess
from collections import defaultdict
import glob
import json

def get_git_commits():
    try:
        result = subprocess.run(['git', 'log', '-n', '5', '--format=%H'], capture_output=True, text=True, check=True)
        return result.stdout.strip().split('\n')
    except subprocess.CalledProcessError:
        return []

def get_file_content_at_commit(commit, filepath):
    try:
        result = subprocess.run(['git', 'show', f'{commit}:{filepath}'], capture_output=True, text=True, check=True)
        return result.stdout
    except subprocess.CalledProcessError:
        return ""

def load_todo_md():
    todo_words = set()
    try:
        with open('TODO.md', 'r') as f:
            content = f.read()
            # simple heuristic: grab alphanumeric words
            for word in re.findall(r'\b[a-zA-Z0-9_]+\b', content.lower()):
                if len(word) > 3:
                    todo_words.add(word)
    except FileNotFoundError:
        pass
    return todo_words

def estimate_cyclomatic_complexity(content):
    complexity_issues = []
    lines = content.split('\n')
    in_function = False
    current_func = ""
    current_complexity = 0
    bracket_level = 0

    for i, line in enumerate(lines):
        line = line.strip()
        if line.startswith('fn ') or line.startswith('pub fn ') or line.startswith('async fn ') or line.startswith('pub async fn '):
            in_function = True
            current_func = line.split('(')[0].split('fn ')[-1].strip()
            current_complexity = 1
            bracket_level = line.count('{') - line.count('}')
            continue

        if in_function:
            bracket_level += line.count('{') - line.count('}')
            if re.search(r'\b(if|else if|match|for|while|loop|\?)\b', line):
                current_complexity += 1

            if bracket_level <= 0:
                in_function = False
                if current_complexity > 15:
                    complexity_issues.append((current_func, current_complexity))

    return complexity_issues

def check_magic_numbers(content):
    magic_issues = []
    lines = content.split('\n')
    for i, line in enumerate(lines):
        if '//' in line:
            line = line.split('//')[0]
        if not re.search(r'\b(const|static|let)\b', line) and not 'test' in line.lower() and not 'assert' in line.lower():
            if re.search(r'\b\d{2,}\b', line): # Numbers >= 10
                magic_issues.append(i+1)
    return magic_issues

def find_struct_defs(content):
    structs = []
    matches = re.finditer(r'(pub\s+)?struct\s+([a-zA-Z0-9_]+)\s*\{([^}]*)\}', content, re.DOTALL)
    for match in matches:
        structs.append(match.group(2))
    return structs

def find_enum_defs(content):
    enums = []
    matches = re.finditer(r'(pub\s+)?enum\s+([a-zA-Z0-9_]+)\s*\{([^}]*)\}', content, re.DOTALL)
    for match in matches:
        enums.append(match.group(2))
    return enums

def run_clippy_for_unused():
    """Fallback if we want to run clippy, but it's expensive. Let's use regex for unreachable/unused macros too."""
    pass

def analyze_content(content, filename, todo_words):
    issues = {
        'dead_code': 0,
        'unused_imports': 0,
        'unreachable_branches': 0,
        'orphaned_todo': 0,
        'high_complexity': 0,
        'inconsistent_errors': 0,
        'magic_numbers': 0,
        'duplicated_structs': 0,
    }

    # 1. Dead code, unused imports, unreachable branches
    issues['dead_code'] += len(re.findall(r'#\[allow\(dead_code\)\]', content))
    issues['unused_imports'] += len(re.findall(r'#\[allow\(unused_imports\)\]', content))
    issues['unreachable_branches'] += len(re.findall(r'unreachable!\(', content))

    # 2. Orphaned TODOs
    todo_matches = re.finditer(r'\b(TODO|FIXME|HACK|TEMP)\b[^\n]*', content)
    for match in todo_matches:
        todo_text = match.group(0).lower()
        words = set(re.findall(r'\b[a-zA-Z0-9_]+\b', todo_text))
        overlap = words.intersection(todo_words)
        if len(overlap) < 2:
             issues['orphaned_todo'] += 1

    # 3. Cyclomatic complexity
    complex_funcs = estimate_cyclomatic_complexity(content)
    issues['high_complexity'] += len(complex_funcs)

    # 4. Inconsistent errors
    has_unwrap = 'unwrap()' in content
    has_expect = 'expect(' in content
    has_question = '?' in content
    has_match = 'match ' in content
    if sum([has_unwrap, has_expect, has_question, has_match]) >= 3:
        issues['inconsistent_errors'] = 1

    # 5. Magic numbers
    magic_nums = check_magic_numbers(content)
    issues['magic_numbers'] += len(magic_nums)

    # 6. Duplicated structs/enums (Simplified: just counting if they exist multiple times in the SAME file for this basic scanner,
    # global cross-file check requires multi-pass)
    structs = find_struct_defs(content)
    enums = find_enum_defs(content)
    all_types = structs + enums
    if len(all_types) != len(set(all_types)):
        issues['duplicated_structs'] += len(all_types) - len(set(all_types))

    score = sum(issues.values())
    return score, issues, structs, enums

def scan_tree(commit=None, todo_words=None):
    total_score = 0
    file_scores = {}

    files_to_scan = []
    if commit is None:
        files_to_scan.extend(glob.glob('mythrax-core/src/**/*.rs', recursive=True))
        files_to_scan.extend(glob.glob('scripts/**/*.py', recursive=True))
        files_to_scan.extend(glob.glob('scripts/**/*.sh', recursive=True))
    else:
        for f in glob.glob('mythrax-core/src/**/*.rs', recursive=True): files_to_scan.append(f)
        for f in glob.glob('scripts/**/*.py', recursive=True): files_to_scan.append(f)
        for f in glob.glob('scripts/**/*.sh', recursive=True): files_to_scan.append(f)

    global_types = defaultdict(list)

    for filepath in files_to_scan:
        if commit is None:
            with open(filepath, 'r') as f:
                content = f.read()
        else:
            content = get_file_content_at_commit(commit, filepath)
            if not content: continue

        score, issues, structs, enums = analyze_content(content, filepath, todo_words)
        total_score += score
        file_scores[filepath] = score

        for s in structs: global_types[s].append(filepath)
        for e in enums: global_types[e].append(filepath)

    # Add cross-file duplication penalty
    for type_name, locations in global_types.items():
        if len(locations) > 1:
            total_score += (len(locations) - 1)
            # Add back to the first file's score to represent the debt
            file_scores[locations[0]] += (len(locations) - 1)

    return total_score, file_scores

def main():
    todo_words = load_todo_md()

    print("Running Sanitation Scanner...")

    current_score, current_file_scores = scan_tree(None, todo_words)

    commits = get_git_commits()
    history = []

    for commit in commits:
        score, _ = scan_tree(commit, todo_words)
        history.append(score)

    history.reverse() # Oldest to newest
    history.append(current_score)

    degrading_files = []
    if commits:
        prev_score, prev_file_scores = scan_tree(commits[0], todo_words)
        for filepath, score in current_file_scores.items():
            if score > prev_file_scores.get(filepath, 0) and score > 0:
                degrading_files.append((filepath, prev_file_scores.get(filepath, 0), score))

    md = "## 🧹 Sanitation Scorecard\n\n"

    trajectory = "➡️ Stable"
    if len(history) > 1:
        if history[-1] > history[-2]: trajectory = "📉 Degrading (Debt Increased)"
        elif history[-1] < history[-2]: trajectory = "📈 Improving (Debt Decreased)"

    md += f"**Trajectory:** {trajectory}\n"
    md += f"**Current Total Debt Score:** {current_score}\n\n"

    md += "### Historical Trend (Last 5 commits + Working Tree)\n"
    md += " | ".join(map(str, history)) + "\n\n"

    if degrading_files:
        md += "### ⚠️ Files with Increasing Debt\n"
        for filepath, old, new in degrading_files:
            md += f"- `{filepath}`: {old} -> **{new}**\n"
    else:
        md += "### ✅ No files with increasing debt detected.\n"

    # We output to a file that the CI reads, but we don't commit it
    with open('sanitation_report.md', 'w') as f:
        f.write(md)

    print("Scan complete. Scorecard written to sanitation_report.md")

if __name__ == '__main__':
    main()
