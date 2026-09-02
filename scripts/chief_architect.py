import os
import sys
import subprocess
import re
import urllib.request
import json

def run_command(cmd, cwd=None):
    try:
        result = subprocess.run(cmd, cwd=cwd, shell=True, check=True, capture_output=True, text=True)
        return result.stdout
    except subprocess.CalledProcessError as e:
        return e.stdout

def get_cyclomatic_complexity(file_path):
    with open(file_path, 'r') as f:
        content = f.read()

    content = re.sub(r'//.*', '', content)
    content = re.sub(r'"(?:\\.|[^"\\])*"', '""', content)

    functions = []

    fn_pattern = re.compile(r'\bfn\s+([a-zA-Z_][a-zA-Z0-9_]*)[^{]*?\{', re.MULTILINE)

    for match in fn_pattern.finditer(content):
        fn_name = match.group(1)
        start_idx = match.end() - 1

        line_num = content.count('\n', 0, start_idx) + 1

        brace_depth = 1
        curr_idx = start_idx + 1
        while curr_idx < len(content) and brace_depth > 0:
            if content[curr_idx] == '{':
                brace_depth += 1
            elif content[curr_idx] == '}':
                brace_depth -= 1
            curr_idx += 1

        body = content[start_idx:curr_idx]
        functions.append((fn_name, line_num, body))

    high_complexity_funcs = []
    for name, line_num, body in functions:
        complexity = 1 + len(re.findall(r'\b(if|match|for|while|loop)\b', body)) + len(re.findall(r'\?', body))
        if complexity > 15:
            high_complexity_funcs.append(f"{file_path}:{line_num} `{name}` (complexity: {complexity})")

    return high_complexity_funcs

def extract_structs_and_enums(file_path):
    with open(file_path, 'r') as f:
        content = f.read()

    content = re.sub(r'//.*', '', content)
    content = re.sub(r'/\*.*?\*/', '', content, flags=re.DOTALL)
    content = re.sub(r'"(?:\\.|[^"\\])*"', '""', content)

    structs = re.findall(r'\bstruct\s+([A-Z][a-zA-Z0-9_]*)\b', content)
    enums = re.findall(r'\benum\s+([A-Z][a-zA-Z0-9_]*)\b', content)

    return structs, enums

def extract_magic_numbers_and_strings(file_path):
    with open(file_path, 'r') as f:
        lines = f.readlines()

    magic = []
    for i, line in enumerate(lines):
        if "const " in line or "static " in line:
            continue

        line = re.sub(r'//.*', '', line)
        strings = re.findall(r'"([a-zA-Z0-9_ -]{5,})"', line)
        numbers = re.findall(r'\b([0-9]{3,})\b', line)

        for s in strings:
            magic.append(f"{file_path}:{i+1} String '{s}'")
        for n in numbers:
            magic.append(f"{file_path}:{i+1} Number {n}")

    return magic

def get_dead_code_checks(git_cmd=""):
    out = run_command(f"{git_cmd}rg \"#\\[allow\\(dead_code\\)\\]\" mythrax-core/src/ scripts/ || true")
    return [l for l in out.splitlines() if l.strip()]

def calculate_debt_metrics(commit_hash=""):
    git_cmd = ""
    if commit_hash:
        git_cmd = f"git checkout {commit_hash} > /dev/null 2>&1 && "
        run_command(f"git checkout {commit_hash} > /dev/null 2>&1")

    dead_code_lines = get_dead_code_checks(git_cmd)

    todos_out = run_command(f"{git_cmd}rg -i '(TODO|FIXME|HACK|TEMP)' mythrax-core/src/ scripts/ || true")

    filtered_todos = []
    for l in todos_out.splitlines():
        if not re.search(r'tempdir|temporal|tempfile|TempDir|temp_dir|temp_file|temperature|temp_total|chief_architect\.py', l, re.IGNORECASE) and l.strip():
             if re.search(r'(?://|#).*?\b(TODO|FIXME|HACK|TEMP)\b', l, re.IGNORECASE):
                 filtered_todos.append(l)

    try:
        with open("TODO.md", "r") as f:
            todo_md = f.read().lower()
    except FileNotFoundError:
        todo_md = ""

    orphaned_todos = []
    for line in filtered_todos:
        comment_text = ""
        match = re.search(r'(?://|#).*?\b(?:TODO|FIXME|HACK|TEMP)\b(.*?)$', line, re.IGNORECASE)
        if match:
            comment_text = match.group(1).lower().strip()

        words = [w for w in re.split(r'\W+', comment_text) if len(w) > 4]

        is_tracked = False
        if len(words) > 0:
            for w in words:
                if w in todo_md:
                    is_tracked = True
                    break

        if not is_tracked:
            orphaned_todos.append(line)

    high_comp_funcs = []
    all_structs = []
    all_enums = []
    all_magic = []

    file_debt = {}

    for root, _, files in os.walk("mythrax-core/src"):
        for file in files:
            if file.endswith(".rs"):
                filepath = os.path.join(root, file)

                # Complexity
                funcs = get_cyclomatic_complexity(filepath)
                high_comp_funcs.extend(funcs)

                # Duplicated Structs/Enums
                s, e = extract_structs_and_enums(filepath)
                all_structs.extend(s)
                all_enums.extend(e)

                # Magic numbers/strings
                m = extract_magic_numbers_and_strings(filepath)
                all_magic.extend(m)

                if filepath not in file_debt:
                    file_debt[filepath] = 0
                file_debt[filepath] += len(funcs) + len(m)

    # Process ripgrep results to get file counts
    for line in dead_code_lines:
        filepath = line.split(':')[0]
        if filepath not in file_debt:
            file_debt[filepath] = 0
        file_debt[filepath] += 1

    for line in orphaned_todos:
        filepath = line.split(':')[0]
        if filepath not in file_debt:
            file_debt[filepath] = 0
        file_debt[filepath] += 1

    struct_counts = {}
    for s in all_structs:
        struct_counts[s] = struct_counts.get(s, 0) + 1

    enum_counts = {}
    for e in all_enums:
        enum_counts[e] = enum_counts.get(e, 0) + 1

    duplicated_types = []
    for k, v in struct_counts.items():
        if v > 1:
            duplicated_types.append(f"Struct `{k}` defined {v} times")
    for k, v in enum_counts.items():
        if v > 1:
            duplicated_types.append(f"Enum `{k}` defined {v} times")

    unwrap_out = run_command(f"{git_cmd}rg \"\\.unwrap\\(\\)\" mythrax-core/src/ || true")
    expect_out = run_command(f"{git_cmd}rg \"\\.expect\\(\" mythrax-core/src/ || true")

    unwrap_lines = [l for l in unwrap_out.splitlines() if l.strip()]
    expect_lines = [l for l in expect_out.splitlines() if l.strip()]

    for line in unwrap_lines:
        filepath = line.split(':')[0]
        if filepath not in file_debt:
            file_debt[filepath] = 0
        file_debt[filepath] += 1

    for line in expect_lines:
        filepath = line.split(':')[0]
        if filepath not in file_debt:
            file_debt[filepath] = 0
        file_debt[filepath] += 1

    if commit_hash:
        run_command("git checkout - > /dev/null 2>&1")

    return {
        "dead_code": len(dead_code_lines),
        "todos": len(orphaned_todos),
        "unwraps": len(unwrap_lines),
        "expects": len(expect_lines),
        "high_complexity": len(high_comp_funcs),
        "duplicated_types": len(duplicated_types),
        "magic_literals": len(all_magic),

        "dead_code_items": dead_code_lines,
        "todo_items": orphaned_todos,
        "unwrap_items": unwrap_lines,
        "expect_items": expect_lines,
        "high_complexity_items": high_comp_funcs,
        "duplicated_type_items": duplicated_types,
        "magic_items": all_magic,
        "file_debt": file_debt
    }

def get_history_metrics():
    run_command("git fetch --unshallow || git fetch --all")
    commits = run_command("git log -n 6 --format='%H'").splitlines()

    if len(commits) <= 1:
        return None

    oldest_commit = commits[-1]

    try:
        prev_metrics = calculate_debt_metrics(oldest_commit)
    except Exception as e:
        print(f"Error getting history metrics: {e}")
        return None

    return prev_metrics

def main():
    print("Running Chief Architect Sanitation Check...")
    current = calculate_debt_metrics()

    prev = get_history_metrics()
    if not prev:
        prev = current

    report = "## Sanitation Scorecard\n\n"
    report += "| Metric | Current | Previous | Trend |\n"
    report += "|---|---|---|---|\n"

    metrics_keys = [
        "dead_code", "todos", "unwraps", "expects",
        "high_complexity", "duplicated_types", "magic_literals"
    ]
    for key in metrics_keys:
        trend = "↑" if current[key] > prev[key] else "↓" if current[key] < prev[key] else "="
        report += f"| {key} | {current[key]} | {prev[key]} | {trend} |\n"

    report += "\n### Files with Increasing Debt\n"
    increasing_files = []
    if prev != current:
        for filepath, debt in current["file_debt"].items():
            prev_debt = prev["file_debt"].get(filepath, 0)
            if debt > prev_debt:
                increasing_files.append(f"- `{filepath}`: {prev_debt} -> {debt} (+{debt - prev_debt})")

    if not increasing_files:
        report += "No files show increasing debt density! 🎉\n"
    else:
        for f in increasing_files[:15]:
            report += f"{f}\n"
        if len(increasing_files) > 15:
            report += f"- ... and {len(increasing_files) - 15} more\n"

    report += "\n### High Complexity Functions (>15)\n"
    for item in current["high_complexity_items"][:10]:
        report += f"- {item}\n"
    if len(current["high_complexity_items"]) > 10:
        report += f"- ... and {len(current['high_complexity_items']) - 10} more\n"

    report += "\n### Orphaned TODOs\n"
    for item in current["todo_items"][:10]:
        report += f"- {item}\n"
    if len(current["todo_items"]) > 10:
        report += f"- ... and {len(current['todo_items']) - 10} more\n"

    report += "\n### Duplicated Types\n"
    for item in current["duplicated_type_items"][:10]:
        report += f"- {item}\n"
    if len(current["duplicated_type_items"]) > 10:
        report += f"- ... and {len(current['duplicated_type_items']) - 10} more\n"

    print("\n================ REPORT ================\n")
    print(report)
    print("\n========================================\n")

    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "push":
        repo = os.environ.get("GITHUB_REPOSITORY")
        token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
        commit_sha = os.environ.get("GITHUB_SHA")

        if repo and token and commit_sha:
            try:
                url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
                req = urllib.request.Request(url, headers={
                    "Authorization": f"Bearer {token}",
                    "Accept": "application/vnd.github.v3+json"
                })
                with urllib.request.urlopen(req) as response:
                    pr_data = json.loads(response.read().decode())
                    if pr_data:
                        pr_number = pr_data[0]["number"]
                        comment_url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
                        comment_req = urllib.request.Request(comment_url, data=json.dumps({"body": report}).encode(), headers={
                            "Authorization": f"Bearer {token}",
                            "Accept": "application/vnd.github.v3+json",
                            "Content-Type": "application/json"
                        }, method="POST")
                        urllib.request.urlopen(comment_req)
                        print(f"Posted scorecard to PR #{pr_number}")
            except Exception as e:
                print(f"Failed to post PR comment: {e}")

if __name__ == "__main__":
    main()
