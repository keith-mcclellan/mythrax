import os
import re
import sys
import subprocess
import urllib.request
import json
from dataclasses import dataclass

@dataclass
class ScanResult:
    dead_code_count: int = 0
    orphaned_todos: int = 0
    high_complexity_funcs: int = 0
    inconsistent_error_handling: int = 0
    duplicate_structs: int = 0
    magic_numbers: int = 0
    total_debt: int = 0

def load_tracked_todos(commit_hash):
    try:
        todo_content = subprocess.run(["git", "show", f"{commit_hash}:TODO.md"], capture_output=True, text=True, check=True).stdout
        return [line.strip().lower() for line in todo_content.split("\n") if line.strip().startswith("- [ ]")]
    except subprocess.CalledProcessError:
        return []

def count_complexity(func_body):
    complexity = 1
    complexity += len(re.findall(r'\b(if|while|for|match|loop)\b', func_body))
    complexity += len(re.findall(r'(\&\&|\|\|)', func_body))
    return complexity

def extract_functions(content):
    # A more robust approach to finding function blocks to avoid regex nested braces issues
    functions = []
    in_function = False
    brace_level = 0
    current_func = []

    lines = content.split('\n')
    for line in lines:
        if not in_function and re.search(r'\bfn\s+\w+\s*\(', line):
            in_function = True
            brace_level = 0
            current_func = []

        if in_function:
            current_func.append(line)
            brace_level += line.count('{')
            brace_level -= line.count('}')

            if brace_level == 0 and '{' in ''.join(current_func):
                functions.append('\n'.join(current_func))
                in_function = False
                current_func = []

    return functions

def scan_codebase_at_commit(commit_hash):
    files_out = subprocess.run(["git", "ls-tree", "-r", "--name-only", commit_hash], capture_output=True, text=True).stdout
    # Include Rust files in mythrax-core and supporting scripts (python/sh in scripts/ or root)
    target_files = []
    for f in files_out.split("\n"):
        if not f: continue
        if f.startswith("mythrax-core") and f.endswith(".rs"):
            target_files.append(f)
        elif f.startswith("scripts/") or f.endswith(".py") or f.endswith(".sh"):
            target_files.append(f)

    tracked_todos = load_tracked_todos(commit_hash)
    result = ScanResult()
    file_debts = {}
    struct_enum_defs = {}

    for file in target_files:
        try:
            content = subprocess.run(["git", "show", f"{commit_hash}:{file}"], capture_output=True, text=True, check=True).stdout
        except:
            continue

        file_debt = 0

        # 1. Dead code (Rust specific)
        if file.endswith(".rs"):
            dead_code = len(re.findall(r'#\[allow\(dead_code\)\]', content))
            dead_code += len(re.findall(r'#\[allow\(unused_imports\)\]', content))
            dead_code += len(re.findall(r'#\[allow\(unreachable_code\)\]', content))
            result.dead_code_count += dead_code
            file_debt += dead_code

        # 2. Orphaned TODOs
        todos = 0
        for match in re.finditer(r'#?\s*//\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE):
            # For python # TODO or rust // TODO
            todo_text = match.group(2).strip().lower()
            tracked = False
            words = set(re.findall(r'\w+', todo_text))
            for t in tracked_todos:
                t_words = set(re.findall(r'\w+', t))
                if len(words.intersection(t_words)) > 2:
                    tracked = True
                    break
            if not tracked:
                todos += 1

        # Also check python comments
        for match in re.finditer(r'#\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE):
            todo_text = match.group(2).strip().lower()
            tracked = False
            words = set(re.findall(r'\w+', todo_text))
            for t in tracked_todos:
                t_words = set(re.findall(r'\w+', t))
                if len(words.intersection(t_words)) > 2:
                    tracked = True
                    break
            if not tracked:
                todos += 1

        result.orphaned_todos += todos
        file_debt += todos

        # 3. Cyclomatic complexity (Rust specific for now, or naive check for python)
        funcs = []
        if file.endswith(".rs"):
            funcs = extract_functions(content)
        elif file.endswith(".py"):
            # Simple indentation-based function extraction for python could be done,
            # but let's just do a naive check if there are many ifs in the file overall,
            # or just rely on the same keywords since python uses if, for, while.
            # For simplicity, we just check python functions roughly by splitting by 'def '
            py_funcs = content.split('def ')
            funcs = py_funcs[1:] if len(py_funcs) > 1 else []

        complexity_debt = 0
        for func_body in funcs:
            if count_complexity(func_body) > 15:
                complexity_debt += 1
        result.high_complexity_funcs += complexity_debt
        file_debt += complexity_debt

        # 4. Error handling (Rust specific)
        if file.endswith(".rs"):
            unwraps = len(re.findall(r'\.unwrap\(\)', content))
            expects = len(re.findall(r'\.expect\(', content))
            questions = len(re.findall(r'\?', content))
            matches = len(re.findall(r'match\s+', content))
            counts = [c for c in [unwraps, expects, questions, matches] if c > 0]
            if len(counts) >= 3 and (unwraps > 0 or expects > 0):
                result.inconsistent_error_handling += 1
                file_debt += 1

        # 5. Duplicated structs/enums (Rust specific)
        if file.endswith(".rs"):
            defs = re.findall(r'(?:struct|enum)\s+(\w+)\s*[{<]', content)
            for d in defs:
                struct_enum_defs[d] = struct_enum_defs.get(d, 0) + 1

        # 6. Magic numbers and strings
        # Look for literal numbers > 1 or strings not assigned to const/static
        # Heuristic: variable = "string" or call("string") or x + 42
        # Exclude common small numbers like 0, 1.
        # This is basic and not an AST parser, so we will match numbers in operations or function calls
        # We will match "literal" or 'literal'
        magic_debt = 0

        # Numbers: standalone digits not part of variable names, not 0 or 1, not part of `const` declaration line
        # Strings: double quotes, not in `const` line.
        for line in content.split('\n'):
            if line.strip().startswith('const ') or line.strip().startswith('static '):
                continue
            if file.endswith(".py") and line.strip().startswith('def ') or line.strip().startswith('class '):
                continue

            # Naive number match
            numbers = re.findall(r'\b([2-9]|\d{2,})\b', line)
            # Naive string match (excluding imports which often use strings in python, or print)
            if 'import' in line or 'print' in line:
                continue
            strings = re.findall(r'"([^"]{4,})"', line) # Only flag strings longer than 3 chars to avoid noise

            # Reduce noise by ensuring it's not a log message or format string
            if 'log::' in line or 'format!' in line or 'println!' in line:
                continue

            magic_debt += len(numbers) + len(strings)

        # Divide magic_debt by a factor to not overwhelm the scorecard, or cap it per file.
        # Let's cap at 10 per file to indicate presence but not blow up the total.
        magic_capped = min(magic_debt, 10)
        result.magic_numbers += magic_capped
        file_debt += magic_capped

        result.total_debt += file_debt
        file_debts[file] = file_debt

    for name, count in struct_enum_defs.items():
        if count > 1:
            result.duplicate_structs += count - 1
            result.total_debt += count - 1

    return result, file_debts

def main():
    subprocess.run(["git", "fetch", "--unshallow"], capture_output=True)
    subprocess.run(["git", "fetch", "--all"], capture_output=True)

    log = subprocess.run(["git", "log", "-6", "--format=%H"], capture_output=True, text=True).stdout.strip().split("\n")
    if len(log) == 0 or log[0] == "":
        print("No commits found.")
        return

    current_commit = log[0]
    previous_commits = log[1:] if len(log) > 1 else []

    current_res, current_file_debts = scan_codebase_at_commit(current_commit)

    scorecard = "## Chief Architect Sanitation Scorecard\n\n"
    scorecard += "| Commit | Total Debt | Dead Code | Orphaned TODOs | Inconsistent Err Handling | High Complexity | Duplicate Structs | Magic Literals |\n"
    scorecard += "|---|---|---|---|---|---|---|---|\n"

    scorecard += f"| `{current_commit[:7]}` (Current) | **{current_res.total_debt}** | {current_res.dead_code_count} | {current_res.orphaned_todos} | {current_res.inconsistent_error_handling} | {current_res.high_complexity_funcs} | {current_res.duplicate_structs} | {current_res.magic_numbers} |\n"

    prev_file_debts = {}
    for i, commit in enumerate(previous_commits):
        res, f_debts = scan_codebase_at_commit(commit)
        scorecard += f"| `{commit[:7]}` (Current-{i+1}) | **{res.total_debt}** | {res.dead_code_count} | {res.orphaned_todos} | {res.inconsistent_error_handling} | {res.high_complexity_funcs} | {res.duplicate_structs} | {res.magic_numbers} |\n"
        if i == 0:
            prev_file_debts = f_debts

    scorecard += "\n### Trajectory Analysis\n"

    if prev_file_debts:
        increasing_files = []
        for file, current_debt in current_file_debts.items():
            prev_debt = prev_file_debts.get(file, 0)
            if current_debt > prev_debt:
                increasing_files.append(f"- `{file}`: {prev_debt} -> {current_debt} debt points")

        if increasing_files:
            scorecard += "**WARNING: Debt density is increasing in the following files:**\n"
            scorecard += "\n".join(increasing_files) + "\n"
        else:
            scorecard += "Good job. Debt density is not increasing in any files.\n"
    else:
        scorecard += "Not enough history to determine trajectory.\n"

    print(scorecard)

    # Check if we should post to PR
    github_event_name = os.environ.get("GITHUB_EVENT_NAME", "")
    if github_event_name in ["push", "pull_request"]:
        post_to_pr(scorecard)

def post_to_pr(scorecard):
    # Retrieve PR number from API since this might be a push event
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    sha = os.environ.get("GITHUB_SHA")

    if not token or not repo or not sha:
        print("Missing GitHub environment variables for posting PR comment.")
        return

    # Get PRs associated with this commit
    url = f"https://api.github.com/repos/{repo}/commits/{sha}/pulls"
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json"
    })

    try:
        with urllib.request.urlopen(req) as response:
            prs = json.loads(response.read().decode())
            if not prs:
                print("No PRs found for this commit.")
                return

            pr_number = prs[0]["number"]

            # Post comment
            comment_url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
            data = json.dumps({"body": scorecard}).encode()
            comment_req = urllib.request.Request(comment_url, data=data, headers={
                "Authorization": f"Bearer {token}",
                "Accept": "application/vnd.github.v3+json",
                "Content-Type": "application/json"
            }, method="POST")

            with urllib.request.urlopen(comment_req) as comment_response:
                print("Successfully posted scorecard to PR.")

    except Exception as e:
        print(f"Failed to post to PR: {e}")

if __name__ == "__main__":
    main()
