import os
import subprocess
import re

def run_cmd(cmd_list):
    result = subprocess.run(cmd_list, capture_output=True, text=True)
    return result.stdout.strip()

def get_commits():
    # Last 6 commits (current + 5 previous)
    out = run_cmd(["git", "log", "-n", "6", "--format=%H"])
    return out.split('\n') if out else []

def get_files_at_commit(commit):
    out = run_cmd(["git", "ls-tree", "-r", commit, "--name-only"])
    files = []
    for line in out.split('\n'):
        if line.startswith("mythrax-core/") and line.endswith(".rs"):
            files.append(line)
        elif line.startswith("scripts/") and (line.endswith(".rs") or line.endswith(".py") or line.endswith(".sh")):
            files.append(line)
    return files

def get_file_content(commit, filepath):
    # Using git show to avoid working directory pollution
    result = subprocess.run(["git", "show", f"{commit}:{filepath}"], capture_output=True, text=True)
    if result.returncode == 0:
        return result.stdout
    return ""

def extract_functions(code):
    funcs = []
    # Find start of fn, ignore where to skip false positives
    for match in re.finditer(r'\bfn\s+([a-zA-Z0-9_]+)', code):
        name = match.group(1)
        start_idx = match.end()
        brace_idx = code.find('{', start_idx)
        semi_idx = code.find(';', start_idx)

        if brace_idx == -1:
            continue

        # We need to make sure the semi colon isn't inside a type declaration like [u8; 32]
        # before the {
        is_impl = True
        if semi_idx != -1 and semi_idx < brace_idx:
            # Check if it's inside brackets []
            before_semi = code[start_idx:semi_idx]
            if before_semi.count('[') > before_semi.count(']'):
                pass # Semicolon is inside an array type, not a trait definition
            elif before_semi.count('(') > before_semi.count(')'):
                pass
            elif code.find('where', start_idx, brace_idx) != -1:
               pass
            else:
               is_impl = False

        if not is_impl:
            continue

        depth = 0
        body_start = brace_idx
        body_end = -1
        in_string = False
        in_char = False
        escape = False

        for i in range(brace_idx, len(code)):
            c = code[i]
            if escape:
                escape = False
                continue
            if c == '\\':
                escape = True
                continue
            if c == '"' and not in_char:
                in_string = not in_string
                continue
            if c == "'" and not in_string:
                in_char = not in_char
                continue

            if not in_string and not in_char:
                if c == '{':
                    depth += 1
                elif c == '}':
                    depth -= 1
                    if depth == 0:
                        body_end = i
                        break
        if body_end != -1:
            funcs.append((name, code[body_start:body_end+1]))
    return funcs

def extract_structs_enums(code):
    items = []
    # struct/enum Name <Optional Generics> {
    # struct/enum Name (Optional Tuple) ;
    for match in re.finditer(r'\b(?:struct|enum)\s+([a-zA-Z0-9_]+)\s*(?:<[^>]*>)?\s*(?:where[^\{;]*)?(\{)', code):
        name = match.group(1)
        start_idx = match.start(2)
        depth = 0
        body_start = start_idx
        body_end = -1

        for i in range(start_idx, len(code)):
            c = code[i]
            if c == '{':
                depth += 1
            elif c == '}':
                depth -= 1
                if depth == 0:
                    body_end = i
                    break
        if body_end != -1:
            body = code[body_start:body_end+1]
            # normalize whitespace
            body_norm = re.sub(r'\s+', ' ', body).strip()
            items.append((name, body_norm))

    # Tuple structs
    for match in re.finditer(r'\bstruct\s+([a-zA-Z0-9_]+)\s*(?:<[^>]*>)?\s*\(.*?\)\s*;', code):
         name = match.group(1)
         body = match.group(0)
         body_norm = re.sub(r'\s+', ' ', body).strip()
         items.append((name, body_norm))
    return items

def analyze_commit(commit):
    files = get_files_at_commit(commit)
    todo_content = get_file_content(commit, "TODO.md")

    metrics = {
        'dead_code': 0,
        'orphaned_todos': 0,
        'high_complexity_fns': 0,
        'inconsistent_error_handling': 0,
        'duplicated_structs': 0,
        'magic_numbers': 0,
        'file_debt': {}
    }

    all_structs = {}

    for f in files:
        content = get_file_content(commit, f)
        if not content:
            continue

        file_debt = 0

        # Dead code annotations
        dead_codes = len(re.findall(r'#\[allow\([^)]*(dead_code|unused_imports|unreachable_code)[^)]*\)\]', content))
        metrics['dead_code'] += dead_codes
        file_debt += dead_codes

        # Orphaned TODOs
        todos = re.findall(r'//\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', content, re.IGNORECASE)
        orphaned = 0
        for tag, text in todos:
            text = text.strip()
            if text and text not in todo_content:
                orphaned += 1
            elif not text:
                orphaned += 1
        metrics['orphaned_todos'] += orphaned
        file_debt += orphaned

        # Structs / Enums
        structs = extract_structs_enums(content)
        for name, body in structs:
            if body not in all_structs:
                all_structs[body] = []
            all_structs[body].append(name)

        # Functions
        funcs = extract_functions(content)
        for name, body in funcs:
            # Complexity
            complexity = 1
            complexity += len(re.findall(r'\bif\b', body))
            complexity += len(re.findall(r'\bwhile\b', body))
            complexity += len(re.findall(r'\bfor\b', body))
            complexity += len(re.findall(r'\bmatch\b', body))
            complexity += len(re.findall(r'\?', body))
            complexity += len(re.findall(r'&&', body))
            complexity += len(re.findall(r'\|\|', body))
            if complexity > 15:
                metrics['high_complexity_fns'] += 1
                file_debt += 1

            # Error handling (unwrap/expect)
            unwraps = body.count('.unwrap()') + body.count('.expect(')
            metrics['inconsistent_error_handling'] += unwraps
            file_debt += unwraps

            # Magic numbers and strings
            numbers = 0
            for m in re.finditer(r'\b\d+\b', body):
                if m.group() not in ['0', '1']:
                    numbers += 1
            strings = len(re.findall(r'"[^"]+"', body))
            metrics['magic_numbers'] += (numbers + strings)
            file_debt += (numbers + strings)

        metrics['file_debt'][f] = file_debt

    # Calculate struct duplicates
    for body, names in all_structs.items():
        if len(names) > 1:
            metrics['duplicated_structs'] += (len(names) - 1)

    # sum total
    metrics['total_debt'] = (
        metrics['dead_code'] +
        metrics['orphaned_todos'] +
        metrics['high_complexity_fns'] +
        metrics['inconsistent_error_handling'] +
        metrics['duplicated_structs'] +
        metrics['magic_numbers']
    )

    return metrics

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        return

    history = []
    for commit in commits:
        metrics = analyze_commit(commit)
        history.append((commit, metrics))

    history.reverse() # chronological order (oldest to newest)

    report = []
    report.append("## Sanitation Scorecard")
    report.append("")
    report.append("### Trajectory")
    report.append("| Commit | Total Debt | Dead Code | Orphaned TODOs | High Complexity | Inconsistent Err | Duplicated Structs | Magic Nums |")
    report.append("|---|---|---|---|---|---|---|---|")

    for commit, metrics in history:
        report.append(f"| {commit[:7]} | {metrics['total_debt']} | {metrics['dead_code']} | {metrics['orphaned_todos']} | {metrics['high_complexity_fns']} | {metrics['inconsistent_error_handling']} | {metrics['duplicated_structs']} | {metrics['magic_numbers']} |")

    report.append("")
    report.append("### File Debt Changes (Newest vs Previous)")

    if len(history) >= 2:
        prev_metrics = history[-2][1]
        curr_metrics = history[-1][1]

        prev_file_debt = prev_metrics['file_debt']
        curr_file_debt = curr_metrics['file_debt']

        increasing_files = []
        for f, debt in curr_file_debt.items():
            prev_debt = prev_file_debt.get(f, 0)
            if debt > prev_debt:
                increasing_files.append((f, prev_debt, debt))

        if increasing_files:
            report.append("**Files with increasing debt density:**")
            for f, prev, curr in increasing_files:
                report.append(f"- `{f}`: {prev} -> {curr}")
        else:
            report.append("No files are increasing in debt density. Great job!")

    report_text = "\n".join(report)
    print(report_text)

    # Save the report temporarily so it can be uploaded or viewed
    with open('scorecard.md', 'w') as f:
        f.write(report_text)

    if os.environ.get("GITHUB_ACTIONS"):
        event_name = os.environ.get("GITHUB_EVENT_NAME")
        if event_name == "push":
            ref_name = os.environ.get("GITHUB_REF_NAME")
            if ref_name:
                pr_num = run_cmd(["gh", "pr", "list", "--head", ref_name, "--json", "number", "-q", ".[0].number"])
                if pr_num and pr_num.isdigit():
                    run_cmd(["gh", "pr", "comment", pr_num, "-F", "scorecard.md"])
                    print(f"Commented on PR #{pr_num}")
                else:
                    print("No associated PR found for this push.")

if __name__ == "__main__":
    main()
