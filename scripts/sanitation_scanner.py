import os
import sys
import re

def get_files_to_scan():
    files_to_scan = []
    for root, _, files in os.walk('mythrax-core'):
        for file in files:
            if file.endswith('.rs'):
                files_to_scan.append(os.path.join(root, file))
    for root, _, files in os.walk('scripts'):
        for file in files:
            if file.endswith(('.py', '.sh')):
                # don't scan self to prevent self-referential debt
                if 'sanitation_scanner.py' not in file:
                    files_to_scan.append(os.path.join(root, file))
    return files_to_scan

def load_tracked_todos():
    try:
        with open('TODO.md', 'r', encoding='utf-8') as f:
            return f.read().lower()
    except FileNotFoundError:
        return ""

def is_comment(line, filepath):
    line = line.strip()
    if filepath.endswith('.rs'):
        return line.startswith('//')
    elif filepath.endswith(('.py', '.sh')):
        return line.startswith('#')
    return False

# Global sets for cross-file duplication checking
global_structs = set()
global_enums = set()

def scan_file(filepath, tracked_todos_text, original_filepath=None):
    # Use original_filepath if provided (e.g. for historical scans where filepath is a temp file)
    actual_filepath = original_filepath if original_filepath else filepath

    debt_score = 0
    findings = []

    with open(filepath, 'r', encoding='utf-8') as f:
        try:
            lines = f.readlines()
        except UnicodeDecodeError:
            return 0, []

    # For cyclomatic complexity in Rust
    in_function = False
    func_complexity = 0
    func_name = ""
    func_start_line = 0
    brace_depth = 0
    func_start_brace_depth = 0

    # For error handling in Rust
    has_unwrap = False
    has_expect = False
    has_try_operator = False
    has_match_err = False

    for i, line in enumerate(lines):
        line_num = i + 1

        # Check for dead code allowances
        if actual_filepath.endswith('.rs'):
            if '#[allow(dead_code)]' in line:
                debt_score += 1
                findings.append(f"Line {line_num}: #[allow(dead_code)] suppression found")

            # Detect duplicated structs and enums
            struct_match = re.search(r'\bstruct\s+([A-Z][a-zA-Z0-9_]*)', line)
            if struct_match:
                s_name = struct_match.group(1)
                if s_name in global_structs:
                    debt_score += 1
                    findings.append(f"Line {line_num}: Struct '{s_name}' may be duplicated or redefined.")
                else:
                    global_structs.add(s_name)

            enum_match = re.search(r'\benum\s+([A-Z][a-zA-Z0-9_]*)', line)
            if enum_match:
                e_name = enum_match.group(1)
                if e_name in global_enums:
                    debt_score += 1
                    findings.append(f"Line {line_num}: Enum '{e_name}' may be duplicated or redefined.")
                else:
                    global_enums.add(e_name)

            # Detect magic numbers / strings in specific logic context (not declarations, simple heuristic)
            if in_function and not is_comment(line, actual_filepath) and 'tests/' not in actual_filepath:
                # Magic numbers in expressions like if x > 42
                # excluding 0, 1, simple array indexes
                if re.search(r'(?:==|!=|>|<|>=|<=|\+|-|\*|/)\s*(?:[2-9]|[1-9][0-9]+)\b', line):
                    debt_score += 1
                    findings.append(f"Line {line_num}: Magic number found in expression.")

                # Magic strings in expressions like if s == "magic"
                if re.search(r'(?:==|!=)\s*"[^"]+"', line):
                    debt_score += 1
                    findings.append(f"Line {line_num}: Magic string literal found in comparison.")

            # Error handling patterns
            if '.unwrap()' in line and not is_comment(line, actual_filepath):
                has_unwrap = True
            if '.expect(' in line and not is_comment(line, actual_filepath):
                has_expect = True
            if '?' in line and not is_comment(line, actual_filepath):
                # Basic check for try operator, avoiding lifetimes like &'a
                if re.search(r'\?\s*[;,\)\]}]', line) or line.strip().endswith('?'):
                    has_try_operator = True
            if 'Err(' in line and 'match ' in line and not is_comment(line, actual_filepath):
                has_match_err = True

            # Cyclomatic complexity approximation
            if not is_comment(line, actual_filepath):
                # Count braces to track function scope
                open_braces = line.count('{')
                close_braces = line.count('}')

                fn_match = re.search(r'\bfn\s+([a-zA-Z0-9_]+)\s*\(', line)
                if fn_match:
                    in_function = True
                    func_complexity = 1
                    func_name = fn_match.group(1)
                    func_start_line = line_num
                    func_start_brace_depth = brace_depth

                brace_depth += open_braces
                brace_depth -= close_braces

                if in_function:
                    # Keywords that increase complexity
                    complexity_keywords = ['if ', 'while ', 'for ', 'match ', '&&', '||', '?', 'loop ']
                    for kw in complexity_keywords:
                        if kw in line:
                            func_complexity += line.count(kw)

                # Function ends when brace depth returns to the level it was at before the function's open brace
                if in_function and brace_depth <= func_start_brace_depth:
                    in_function = False
                    if func_complexity > 15:
                        debt_score += 1
                        findings.append(f"Line {func_start_line}: Function '{func_name}' has excessive cyclomatic complexity ({func_complexity} > 15)")

        # Check for TODO, FIXME, HACK, TEMP comments
        if is_comment(line, actual_filepath):
            comment_match = re.search(r'\b(TODO|FIXME|HACK|TEMP)\b', line, re.IGNORECASE)
            if comment_match:
                keyword = comment_match.group(1).upper()

                if actual_filepath.endswith('.rs'):
                    snippet_match = re.search(r'//\s*(?:TODO|FIXME|HACK|TEMP)?\s*:?\s*(.*)', line, re.IGNORECASE)
                else:
                    snippet_match = re.search(r'#\s*(?:TODO|FIXME|HACK|TEMP)?\s*:?\s*(.*)', line, re.IGNORECASE)

                snippet = snippet_match.group(1).strip().lower() if snippet_match else line.strip().lower()
                snippet = re.sub(r'^(todo|fixme|hack|temp)\b:?\s*', '', snippet).strip()

                if len(snippet) > 5 and snippet not in tracked_todos_text:
                    debt_score += 1
                    findings.append(f"Line {line_num}: Orphaned {keyword} comment not tracked in TODO.md")
                elif len(snippet) <= 5:
                    debt_score += 1
                    findings.append(f"Line {line_num}: Orphaned {keyword} comment (too short to track properly)")

    # Error handling consistency check (only for rust source, not tests)
    if actual_filepath.endswith('.rs') and 'tests/' not in actual_filepath:
        error_patterns = sum([1 for p in [has_unwrap, has_expect, has_try_operator, has_match_err] if p])
        if error_patterns > 1:
            debt_score += 1
            patterns_found = []
            if has_unwrap: patterns_found.append("unwrap")
            if has_expect: patterns_found.append("expect")
            if has_try_operator: patterns_found.append("?")
            if has_match_err: patterns_found.append("match err")
            findings.append(f"File level: Inconsistent error handling. Mix of: {', '.join(patterns_found)}")

    return debt_score, findings

import subprocess
import tempfile

def get_git_commits(limit=6):
    try:
        result = subprocess.run(['git', 'log', f'-n{limit}', '--format=%H'], capture_output=True, text=True, check=True)
        return result.stdout.strip().split('\n')
    except subprocess.CalledProcessError:
        return []

def calculate_current_debts():
    files = get_files_to_scan()
    tracked_todos_text = load_tracked_todos()

    total_debt = 0
    file_debts = {}

    for f in files:
        debt, findings = scan_file(f, tracked_todos_text)
        if debt > 0:
            file_debts[f] = {'score': debt, 'findings': findings}
            total_debt += debt

    return total_debt, file_debts

def calculate_historical_debts(commit_hash):
    files = get_files_to_scan()
    tracked_todos_text = ""
    try:
        result = subprocess.run(['git', 'show', f'{commit_hash}:TODO.md'], capture_output=True, text=True)
        if result.returncode == 0:
            tracked_todos_text = result.stdout.lower()
    except Exception:
        pass

    total_debt = 0
    file_debts = {}

    # Reset globals for historical runs
    global global_structs, global_enums
    global_structs = set()
    global_enums = set()

    for f in files:
        try:
            result = subprocess.run(['git', 'show', f'{commit_hash}:{f}'], capture_output=True, text=True)
            if result.returncode == 0:
                ext = '.rs' if f.endswith('.rs') else ('.py' if f.endswith('.py') else '.sh')
                with tempfile.NamedTemporaryFile(mode='w', suffix=ext, delete=False) as tmp:
                    tmp.write(result.stdout)
                    tmp_path = tmp.name

                # Pass original filepath for correct extension and path-based logic
                debt, _ = scan_file(tmp_path, tracked_todos_text, original_filepath=f)
                os.remove(tmp_path)

                if debt > 0:
                    file_debts[f] = debt
                    total_debt += debt
        except Exception:
            pass

    return total_debt, file_debts

def main():
    current_total_debt, current_file_debts = calculate_current_debts()
    commits = get_git_commits(6)

    print(f"# Sanitation Scorecard\n")

    if len(commits) > 1:
        print("## Debt Trajectory (Last 5 Commits)")
        historical_totals = []
        for c in commits[1:]:
            hist_total, _ = calculate_historical_debts(c)
            historical_totals.append(hist_total)

        history_str = " -> ".join([str(x) for x in reversed(historical_totals)])
        print(f"Past: {history_str} -> **Current: {current_total_debt}**\n")

        if current_total_debt > historical_totals[0]:
            print("🚨 **WARNING: Overall debt is increasing!** 🚨\n")
        elif current_total_debt < historical_totals[0]:
            print("✅ **Great job! Overall debt is decreasing.**\n")
        else:
            print("➡️ **Debt is stable.**\n")

        # Check for files with increasing debt compared to immediate previous commit
        _, prev_file_debts = calculate_historical_debts(commits[1])
        increasing_files = []
        for f, data in current_file_debts.items():
            prev_debt = prev_file_debts.get(f, 0)
            if data['score'] > prev_debt:
                increasing_files.append((f, prev_debt, data['score']))

        if increasing_files:
            print("### Files with Increasing Debt:")
            for f, prev, curr in increasing_files:
                print(f"- `{f}`: {prev} -> {curr}")
            print("\n")

    print(f"## Current Debt Score: {current_total_debt}\n")

    # Sort files by highest debt
    sorted_files = sorted(current_file_debts.items(), key=lambda item: item[1]['score'], reverse=True)

    for f, data in sorted_files:
        print(f"### `{f}` (Debt: {data['score']})")
        for finding in data['findings']:
            print(f"- {finding}")
        print()

if __name__ == "__main__":
    main()
