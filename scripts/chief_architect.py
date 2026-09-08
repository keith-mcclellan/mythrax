import os
import re
import subprocess
import json
import urllib.request
import urllib.error
import sys

def get_git_commits():
    result = subprocess.run(["git", "log", "--format=%H", "-n", "6"], capture_output=True, text=True)
    if result.returncode != 0:
        return []
    return result.stdout.strip().split("\n")

def get_file_content(commit_hash, file_path):
    result = subprocess.run(["git", "show", f"{commit_hash}:{file_path}"], capture_output=True, text=True)
    if result.returncode != 0:
        return ""
    return result.stdout

def get_files_in_commit(commit_hash):
    result = subprocess.run(["git", "ls-tree", "-r", "--name-only", commit_hash], capture_output=True, text=True)
    if result.returncode != 0:
        return []
    files = result.stdout.strip().split("\n")
    return [f for f in files if (f.startswith("mythrax-core/") and f.endswith(".rs")) or (f.startswith("scripts/") and (f.endswith(".py") or f.endswith(".sh")))]

def check_complexity(code):
    high_cc = 0
    in_function = False
    brace_depth = 0
    cc = 1
    started = False

    for line in code.split('\n'):
        if '"' in line:
            line_clean = re.sub(r'".*?"', '""', line)
        else:
            line_clean = line
        line_clean = line_clean.split('//')[0]

        if not in_function:
            if 'fn ' in line_clean and re.search(r'\bfn\s+\w+', line_clean):
                in_function = True
                brace_depth = 0
                cc = 1
                started = False

        if in_function:
            brace_depth += line_clean.count('{')
            brace_depth -= line_clean.count('}')
            if '{' in line_clean:
                started = True

            words = re.findall(r'\b(if|while|for|match|loop)\b', line_clean)
            cc += len(words)
            cc += line_clean.count('&&')
            cc += line_clean.count('||')
            cc += line_clean.count('?')

            if ';' in line_clean and not started:
                in_function = False
            elif started and brace_depth <= 0:
                in_function = False
                if cc > 15:
                    high_cc += 1
    return high_cc

def check_error_handling(code):
    has_unwrap = '.unwrap()' in code or '.expect(' in code
    has_question = '?' in code
    has_match = 'match ' in code
    has_convention = 'error handling convention' in code.lower()
    if has_unwrap and (has_question or has_match) and not has_convention:
        return 1
    return 0

def check_magic_numbers(code):
    magic_count = 0
    for line in code.split('\n'):
        if line.strip().startswith('const ') or line.strip().startswith('static '):
            continue

        line_clean = line.split('//')[0]

        # Check string literals
        strings = re.findall(r'"([^"]*)"', line_clean)
        for s in strings:
            if len(s) > 3 and not s.startswith('{') and not s.endswith('}'):
                magic_count += 1

        if '"' in line:
            line_clean = re.sub(r'".*?"', '""', line)

        nums = re.findall(r'\b\d+\b', line_clean)
        for n in nums:
            if int(n) > 9:
                magic_count += 1
    return magic_count

def check_struct_dups(code):
    bodies = []
    in_struct = False
    brace_depth = 0
    started = False
    current_body = []

    for line in code.split('\n'):
        if '"' in line:
            line_clean = re.sub(r'".*?"', '""', line)
        else:
            line_clean = line
        line_clean = line_clean.split('//')[0]

        if not in_struct:
            if re.search(r'\b(struct|enum)\s+\w+', line_clean):
                in_struct = True
                brace_depth = 0
                started = False
                current_body = []

        if in_struct:
            brace_depth += line_clean.count('{')
            brace_depth -= line_clean.count('}')
            if '{' in line_clean:
                started = True

            current_body.append(line_clean)

            if ';' in line_clean and not started:
                in_struct = False
            elif started and brace_depth <= 0:
                in_struct = False
                body_str = '\n'.join(current_body)
                start_idx = body_str.find('{')
                end_idx = body_str.rfind('}')
                if start_idx != -1 and end_idx != -1 and end_idx > start_idx:
                    body = body_str[start_idx+1:end_idx]
                    body = re.sub(r'\s+', '', body)
                    if body:
                        bodies.append(body)
    return bodies

def analyze_commit(commit_hash):
    files = get_files_in_commit(commit_hash)
    todo_content = get_file_content(commit_hash, "TODO.md").lower()

    total_dead_code = 0
    total_orphaned_todos = 0
    total_high_cc = 0
    total_inconsistent_err = 0
    total_magic_numbers = 0

    struct_bodies = {}
    total_dup_structs = 0

    file_debt = {}

    for f in files:
        content = get_file_content(commit_hash, f)
        if not content:
            continue

        dead_code = len(re.findall(r'#\[allow\([^)]*(dead_code|unused_imports|unreachable_code)[^)]*\)\]', content))
        orphaned_todos = 0
        todos = re.findall(r'(?:TODO|FIXME|HACK|TEMP)([^\n]*)', content, re.IGNORECASE)
        for t in todos:
            words = [w for w in re.split(r'\W+', t.lower()) if len(w) > 4]
            if words and not any(w in todo_content for w in words):
                orphaned_todos += 1

        high_cc = 0
        inconsistent_err = 0
        magic_numbers = 0

        if f.endswith('.rs'):
            high_cc = check_complexity(content)
            inconsistent_err = check_error_handling(content)
            magic_numbers = check_magic_numbers(content)

            for body in check_struct_dups(content):
                if body in struct_bodies:
                    struct_bodies[body].append(f)
                else:
                    struct_bodies[body] = [f]

        elif f.endswith('.py') or f.endswith('.sh'):
            magic_numbers = check_magic_numbers(content)

        file_total = dead_code + orphaned_todos + high_cc + inconsistent_err + magic_numbers
        if file_total > 0:
            file_debt[f] = file_total

        total_dead_code += dead_code
        total_orphaned_todos += orphaned_todos
        total_high_cc += high_cc
        total_inconsistent_err += inconsistent_err
        total_magic_numbers += magic_numbers

    for body, flist in struct_bodies.items():
        if len(flist) > 1:
            dups = len(flist) - 1
            total_dup_structs += dups
            for f in flist:
                file_debt[f] = file_debt.get(f, 0) + 1

    total_debt = (total_dead_code + total_orphaned_todos + total_high_cc +
                  total_inconsistent_err + total_magic_numbers + total_dup_structs)

    return {
        'hash': commit_hash,
        'dead_code': total_dead_code,
        'orphaned_todos': total_orphaned_todos,
        'high_cc': total_high_cc,
        'inconsistent_err': total_inconsistent_err,
        'magic_numbers': total_magic_numbers,
        'dup_structs': total_dup_structs,
        'total': total_debt,
        'file_debt': file_debt
    }

def generate_scorecard(results):
    if not results:
        return "No commits found."

    current = results[0]

    md = "## Chief Architect Sanitation Scorecard\n\n"
    md += "| Metric | Current Commit | Last 5 Commits Trend |\n"
    md += "|--------|----------------|----------------------|\n"

    def trend_str(key):
        vals = [r[key] for r in results[::-1]]
        return " -> ".join(map(str, vals))

    md += f"| Dead Code/Unused | {current['dead_code']} | {trend_str('dead_code')} |\n"
    md += f"| Orphaned TODOs | {current['orphaned_todos']} | {trend_str('orphaned_todos')} |\n"
    md += f"| High Complexity (>15) | {current['high_cc']} | {trend_str('high_cc')} |\n"
    md += f"| Inconsistent Error Handling | {current['inconsistent_err']} | {trend_str('inconsistent_err')} |\n"
    md += f"| Duplicated Structs | {current['dup_structs']} | {trend_str('dup_structs')} |\n"
    md += f"| Magic Numbers | {current['magic_numbers']} | {trend_str('magic_numbers')} |\n"
    md += f"| **Total Debt** | **{current['total']}** | **{trend_str('total')}** |\n\n"

    if len(results) > 1:
        prev = results[1]
        if current['total'] > prev['total']:
            md += "📉 **Trajectory: Degrading** (Debt increased compared to last commit)\n\n"
        elif current['total'] < prev['total']:
            md += "📈 **Trajectory: Improving** (Debt decreased compared to last commit)\n\n"
        else:
            md += "⚖️ **Trajectory: Stagnant** (No change in debt)\n\n"

    md += "### Files with Increasing Debt\n"
    increasing_files = []
    if len(results) > 1:
        prev_file_debt = results[1]['file_debt']
        curr_file_debt = current['file_debt']
        for f, debt in curr_file_debt.items():
            prev_debt = prev_file_debt.get(f, 0)
            if debt > prev_debt:
                increasing_files.append(f"- `{f}`: {prev_debt} -> {debt} debt items")

    if increasing_files:
        md += "\n".join(increasing_files) + "\n"
    else:
        md += "No files with increasing debt.\n"

    return md

def post_pr_comment(markdown_body):
    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")
    sha = os.environ.get("GITHUB_SHA")
    event_name = os.environ.get("GITHUB_EVENT_NAME")

    if not repo or not token:
        print("GitHub context missing. Saving locally.")
        os.makedirs("issues", exist_ok=True)
        with open("issues/chief_architect_report.md", "w") as f:
            f.write(markdown_body)
        return

    pr_number = None

    if event_name == "pull_request":
        event_path = os.environ.get("GITHUB_EVENT_PATH")
        if event_path and os.path.exists(event_path):
            with open(event_path, "r") as f:
                event_data = json.load(f)
                pr_number = event_data.get("pull_request", {}).get("number")
    elif event_name == "push" and sha:
        url = f"https://api.github.com/repos/{repo}/commits/{sha}/pulls"
        req = urllib.request.Request(url, headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github.v3+json",
            "User-Agent": "python-urllib"
        })
        try:
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read().decode())
                if data and len(data) > 0:
                    pr_number = data[0].get("number")
        except Exception as e:
            print(f"Failed to fetch PRs for commit {sha}: {e}")

    if pr_number:
        url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
        req = urllib.request.Request(url, method="POST", headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github.v3+json",
            "Content-Type": "application/json",
            "User-Agent": "python-urllib"
        }, data=json.dumps({"body": markdown_body}).encode("utf-8"))
        try:
            with urllib.request.urlopen(req) as response:
                print(f"Successfully posted comment to PR #{pr_number}")
        except Exception as e:
            print(f"Failed to post comment to PR #{pr_number}: {e}")
    else:
        print("No associated PR found. Saving locally.")
        os.makedirs("issues", exist_ok=True)
        with open("issues/chief_architect_report.md", "w") as f:
            f.write(markdown_body)

if __name__ == "__main__":
    print("Fetching recent commits...")
    commits = get_git_commits()
    if not commits:
        print("No commits found.")
        sys.exit(0)

    print(f"Found {len(commits)} commits. Analyzing...")
    results = []
    for c in commits:
        print(f"Analyzing commit {c}...")
        results.append(analyze_commit(c))

    scorecard = generate_scorecard(results)
    print("\nScorecard generated:\n")
    print(scorecard)
    post_pr_comment(scorecard)
