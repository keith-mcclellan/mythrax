import os
import subprocess
import re

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout.strip()

def get_commits():
    out = run_cmd('git log --format="%H" -n 6')
    return out.split('\n') if out else []

def get_files_at_commit(commit):
    out = run_cmd(f'git ls-tree -r --name-only {commit}')
    files = []
    for line in out.split('\n'):
        line = line.strip()
        if (line.startswith('mythrax-core/') and line.endswith('.rs')) or (line.startswith('scripts/') and line.endswith('.py')) or (line.endswith('.py') and '/' not in line):
            files.append(line)
    return files

def get_file_content(commit, filepath):
    return run_cmd(f'git show {commit}:{filepath}')

def read_todo_md(commit):
    content = get_file_content(commit, 'TODO.md')
    words = set(re.findall(r'\b[a-zA-Z]{4,}\b', content.lower()))
    return words

def calculate_complexity(func_body):
    complexity = 1
    complexity += len(re.findall(r'\bif\b', func_body))
    complexity += len(re.findall(r'\bwhile\b', func_body))
    complexity += len(re.findall(r'\bfor\b', func_body))
    complexity += len(re.findall(r'\bmatch\b', func_body))
    complexity += len(re.findall(r'\?', func_body))
    complexity += len(re.findall(r'&&', func_body))
    complexity += len(re.findall(r'\|\|', func_body))
    return complexity

def extract_functions(code):
    code = re.sub(r'".*?(?<!\\)"', '""', code, flags=re.DOTALL)
    code = re.sub(r'//.*', '', code)
    code = re.sub(r'/\*.*?\*/', '', code, flags=re.DOTALL)

    functions = []
    idx = 0
    while True:
        match = re.search(r'\bfn\s+', code[idx:])
        if not match:
            break
        start_idx = idx + match.start()

        i = start_idx
        in_angles = 0
        in_parens = 0
        in_brackets = 0
        found_body = False
        end = -1

        while i < len(code):
            c = code[i]
            if c == '<': in_angles += 1
            elif c == '>': in_angles -= 1
            elif c == '(': in_parens += 1
            elif c == ')': in_parens -= 1
            elif c == '[': in_brackets += 1
            elif c == ']': in_brackets -= 1
            elif c == '{':
                if in_angles == 0 and in_parens == 0 and in_brackets == 0:
                    found_body = True
                    braces = 0
                    for j in range(i, len(code)):
                        if code[j] == '{': braces += 1
                        elif code[j] == '}':
                            braces -= 1
                            if braces == 0:
                                end = j
                                break
                    break
            elif c == ';':
                if in_angles == 0 and in_parens == 0 and in_brackets == 0:
                    break
            i += 1

        if found_body and end != -1:
            functions.append(code[start_idx:end+1])
            idx = end + 1
        else:
            idx = start_idx + 2
    return functions

def extract_structs_enums(code):
    code = re.sub(r'".*?(?<!\\)"', '""', code, flags=re.DOTALL)
    code = re.sub(r'//.*', '', code)
    code = re.sub(r'/\*.*?\*/', '', code, flags=re.DOTALL)

    items = []
    for match in re.finditer(r'\b(?:struct|enum)\s+(\w+)\s*\{', code):
        start = match.end() - 1
        braces = 0
        end = -1
        for i in range(start, len(code)):
            if code[i] == '{':
                braces += 1
            elif code[i] == '}':
                braces -= 1
                if braces == 0:
                    end = i
                    break
        if end != -1:
            items.append((match.group(1), code[start:end+1]))
    return items

def analyze_file(filepath, content, todo_words):
    debt = 0
    findings = []

    suppressions = len(re.findall(r'#\[allow\((?:dead_code|unused_imports|unreachable_code)\)\]', content))
    if suppressions > 0:
        debt += suppressions * 2
        findings.append(f"{suppressions} suppression annotations (dead_code/unused_imports/unreachable_code)")

    comments = re.findall(r'//\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE)
    for tag, text in comments:
        text_words = set(re.findall(r'\b[a-zA-Z]{4,}\b', text.lower()))
        if not text_words.intersection(todo_words) and todo_words:
            debt += 3
            findings.append(f"Orphaned {tag}: {text.strip()[:30]}")
        else:
            debt += 1
            findings.append(f"Tracked {tag}")

    funcs = extract_functions(content)
    for f in funcs:
        c = calculate_complexity(f)
        if c > 15:
            debt += (c - 15) * 2
            findings.append(f"High complexity ({c}) in function")

    unwraps = len(re.findall(r'\.unwrap\(\)', content))
    expects = len(re.findall(r'\.expect\(', content))
    question_marks = len(re.findall(r'\?', content))
    matches = len(re.findall(r'\bmatch\b', content))

    if (unwraps > 0 or expects > 0) and question_marks > 0:
        debt += 5
        findings.append(f"Inconsistent error handling: mixing unwrap/expect with ? operator")
    elif unwraps > 0:
        debt += unwraps
        findings.append(f"{unwraps} unwrap() calls")
    elif expects > 0:
        debt += expects
        findings.append(f"{expects} expect() calls")

    magic_count = 0
    for f in funcs:
        magic_count += len(re.findall(r'(?<![a-zA-Z0-9_])(?:[2-9]|\d{2,})(?![a-zA-Z0-9_])', f))
        magic_count += len(re.findall(r'"[a-zA-Z0-9_ -]{5,}"', f))
    if magic_count > 0:
        debt += magic_count * 0.5
        findings.append(f"~{magic_count} magic numbers/strings")

    return debt, findings, extract_structs_enums(content)

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        return

    print(f"Analyzing {len(commits)} commits...")
    history = []

    for commit in reversed(commits):
        todo_words = read_todo_md(commit)
        files = get_files_at_commit(commit)

        commit_debt = 0
        file_stats = {}
        all_structs = []

        for f in files:
            content = get_file_content(commit, f)
            debt, findings, structs = analyze_file(f, content, todo_words)
            if debt > 0:
                commit_debt += debt
                file_stats[f] = {'debt': debt, 'findings': findings}
            all_structs.extend(structs)

        struct_bodies = {}
        for name, body in all_structs:
            norm = re.sub(r'\s+', '', body)
            if norm in struct_bodies:
                commit_debt += 5
                if struct_bodies[norm] != name:
                    file_stats.setdefault("Global", {'debt':0, 'findings':[]})['findings'].append(f"Duplicated struct/enum: {name} and {struct_bodies[norm]}")
            else:
                struct_bodies[norm] = name

        history.append({
            'commit': commit,
            'debt': commit_debt,
            'files': file_stats
        })

    current = history[-1]
    previous = history[-2] if len(history) > 1 else None

    scorecard = ["# Sanitation Scorecard\n"]
    scorecard.append("## Trajectory\n")

    for h in history:
        scorecard.append(f"- Commit `{h['commit'][:7]}`: **{h['debt']}** debt points")

    scorecard.append("\n## Analysis of Current Commit\n")

    if previous:
        diff = current['debt'] - previous['debt']
        if diff > 0:
            scorecard.append(f"**DEGRADING**: Total debt increased by {diff} points.\n")
        elif diff < 0:
            scorecard.append(f"**IMPROVING**: Total debt decreased by {abs(diff)} points.\n")
        else:
            scorecard.append("**STABLE**: No change in total debt.\n")

        scorecard.append("### Files with Increasing Debt Density\n")
        for f, stats in current['files'].items():
            prev_debt = previous['files'].get(f, {}).get('debt', 0)
            if stats['debt'] > prev_debt:
                scorecard.append(f"- **{f}**: {prev_debt} -> {stats['debt']} points")
                for finding in stats['findings']:
                    scorecard.append(f"  - {finding}")

    scorecard.append("\n## Current Debt Findings\n")
    sorted_files = sorted(current['files'].items(), key=lambda x: x[1]['debt'], reverse=True)
    for f, stats in sorted_files[:15]:
        scorecard.append(f"### {f} (Debt: {stats['debt']})")
        for finding in stats['findings']:
            scorecard.append(f"- {finding}")

    report = "\n".join(scorecard)
    with open('scorecard.md', 'w') as f:
        f.write(report)

    print(report)

    if os.environ.get('GITHUB_ACTIONS') == 'true' and os.environ.get('GITHUB_EVENT_NAME') == 'push':
        ref_name = os.environ.get('GITHUB_REF_NAME', '')
        if ref_name:
            print(f"Posting comment to PR for branch {ref_name}...")
            cmd = f'gh pr comment {ref_name} -F scorecard.md'
            subprocess.run(cmd, shell=True)

if __name__ == "__main__":
    main()
