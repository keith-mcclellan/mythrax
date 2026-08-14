import os
import subprocess
import json
import re
from collections import defaultdict
import sys

def run_cmd(cmd, cwd=None):
    res = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=cwd)
    return res.stdout

def get_commits():
    lines = run_cmd("git log -n 6 --format='%H'").strip().split('\n')
    return [l for l in lines if l]

def load_todo_md():
    try:
        with open("TODO.md", "r") as f:
            return f.read().lower()
    except FileNotFoundError:
        return ""

def is_tracked(comment_text, todo_text):
    words = [w for w in re.findall(r'\w+', comment_text.lower()) if len(w) > 3]
    if not words: return False
    for w in words:
        if w in todo_text:
            return True
    return False

def analyze_file(filepath, todo_text):
    debt_items = 0
    complexity_issues = []
    error_handling = {'unwrap': 0, 'expect': 0, 'question': 0, 'match': 0}
    structs = {}

    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
    except UnicodeDecodeError:
        return 0, [], {}

    debt_items += len(re.findall(r'#\[allow\((dead_code|unused|unused_imports|unreachable_code)\)\]', content))

    comments = re.findall(r'//.*(?:TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE)
    for c in comments:
        debt_items += 1
        if not is_tracked(c, todo_text):
            debt_items += 2

    functions = re.split(r'\bfn\s+', content)
    for func in functions[1:]:
        body_start = func.find('{')
        if body_start == -1: continue
        name_match = re.match(r'([a-zA-Z0-9_]+)', func)
        if not name_match: continue
        name = name_match.group(1)

        braces = 0
        body_end = body_start
        for i, char in enumerate(func[body_start:]):
            if char == '{': braces += 1
            elif char == '}': braces -= 1
            if braces == 0:
                body_end = body_start + i
                break

        body = func[body_start:body_end]
        cc = 1
        cc += len(re.findall(r'\bif\b', body))
        cc += len(re.findall(r'\bwhile\b', body))
        cc += len(re.findall(r'\bfor\b', body))
        cc += len(re.findall(r'\bloop\b', body))
        cc += len(re.findall(r'\bmatch\b', body))
        cc += len(re.findall(r'&&', body))
        cc += len(re.findall(r'\|\|', body))
        cc += len(re.findall(r'\?', body))

        if cc > 15:
            complexity_issues.append((name, cc))
            debt_items += (cc - 15)

    error_handling['unwrap'] = len(re.findall(r'\.unwrap\(\)', content))
    error_handling['expect'] = len(re.findall(r'\.expect\(', content))
    error_handling['question'] = len(re.findall(r'\?', content))
    error_handling['match'] = len(re.findall(r'\bmatch\b\s+.*?\{', content))

    types_used = sum(1 for v in error_handling.values() if v > 0)
    if types_used > 1:
        debt_items += types_used * 2

    for match in re.finditer(r'\b(?:struct|enum)\s+([a-zA-Z0-9_]+)\s*\{([^}]+)\}', content):
        name = match.group(1)
        body = re.sub(r'\s+', '', match.group(2))
        structs[name] = body

    lines = content.split('\n')
    for line in lines:
        if any(x in line for x in ['const ', 'static ', 'let ', 'println!', 'print!', 'format!', 'log::', 'tracing::', 'assert', 'test', 'fn ']):
            continue
        magic_nums = re.findall(r'\b(?:[2-9]|\d{2,})\b', line)
        magic_strs = re.findall(r'"[^"]{3,}"', line)
        if magic_nums or magic_strs:
            debt_items += len(magic_nums) + len(magic_strs)

    return debt_items, complexity_issues, structs

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        return

    original_commit = run_cmd("git rev-parse HEAD").strip()
    results = {}

    for commit in commits:
        sys.stderr.write(f"Analyzing {commit}...\n")
        run_cmd(f"git checkout {commit}")

        todo_text = load_todo_md()
        debt_by_file = defaultdict(int)
        all_structs = {}
        dup_structs = 0

        if os.path.exists("mythrax-core/Cargo.toml"):
            run_cmd("cd mythrax-core && cargo clean")
            clippy_out = run_cmd("cd mythrax-core && cargo clippy --message-format=json")
            for line in clippy_out.split('\n'):
                if not line: continue
                try:
                    msg = json.loads(line)
                    if msg.get('reason') == 'compiler-message':
                        code = msg.get('message', {}).get('code', {})
                        if code:
                            code_id = code.get('code', '')
                            if code_id in ['dead_code', 'unused_imports', 'unreachable_code']:
                                spans = msg.get('message', {}).get('spans', [])
                                if spans:
                                    file_name = spans[0].get('file_name', '')
                                    debt_by_file[file_name] += 5
                except:
                    pass

        files_to_scan = []
        for root, dirs, files in os.walk("."):
            dirs[:] = [d for d in dirs if d not in ['.git', 'target', 'node_modules', '.venv', 'issues']]
            for f in files:
                if f.endswith('.rs') or f.endswith('.py') or f.endswith('.sh'):
                    files_to_scan.append(os.path.join(root, f))

        for f in files_to_scan:
            if "sanitation_scorecard.py" in f:
                continue
            debt, comp, structs = analyze_file(f, todo_text)
            debt_by_file[f] += debt
            for name, body in structs.items():
                for ext_name, ext_body in all_structs.items():
                    if body == ext_body and name != ext_name:
                        dup_structs += 1
                        debt_by_file[f] += 10
                all_structs[name] = body

        total_debt = sum(debt_by_file.values())
        results[commit] = {
            'total_debt': total_debt,
            'files': dict(debt_by_file)
        }

    run_cmd(f"git checkout {original_commit}")

    print("# Sanitation Scorecard\n")
    print("## Trajectory")
    print("| Commit | Total Debt |")
    print("|---|---|")
    for commit in reversed(commits):
        print(f"| {commit[:8]} | {results[commit]['total_debt']} |")

    print("\n## Current Debt Density")
    curr_files = results[commits[0]]['files']
    sorted_files = sorted(curr_files.items(), key=lambda x: x[1], reverse=True)
    print("| File | Debt Score |")
    print("|---|---|")
    for file, score in sorted_files[:20]:
        if score > 0:
            print(f"| {file} | {score} |")

    if len(commits) > 1:
        print("\n## Degrading Files")
        print("Files where debt has increased compared to the previous commit:\n")
        prev_files = results[commits[1]]['files']
        for file, score in curr_files.items():
            prev_score = prev_files.get(file, 0)
            if score > prev_score:
                print(f"- {file}: {prev_score} -> {score}")

if __name__ == "__main__":
    main()
