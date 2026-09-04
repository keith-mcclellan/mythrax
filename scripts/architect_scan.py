import os
import re
import sys
import subprocess
import urllib.request
import json

def run_cmd(cmd):
    try:
        return subprocess.check_output(cmd, shell=True, stderr=subprocess.DEVNULL).decode('utf-8').strip()
    except subprocess.CalledProcessError:
        return ""

def get_commits():
    # Ensure we have history
    subprocess.run("git fetch --unshallow || git fetch --all", shell=True, stderr=subprocess.DEVNULL)
    commits_str = run_cmd("git log -6 --format='%H'")
    return [c for c in commits_str.split('\n') if c]

def parse_todo():
    todo_text = ""
    if os.path.exists("TODO.md"):
        with open("TODO.md", "r", encoding="utf-8") as f:
            todo_text = f.read().lower()
    return todo_text

def analyze_codebase():
    todo_text = parse_todo()
    findings = []
    total_debt = 0
    file_debt = {}
    structs = {}

    # Gather files
    files_str = run_cmd("find mythrax-core scripts -type f -name '*.rs' -o -name '*.py' -o -name '*.sh'")
    files = [f for f in files_str.split('\n') if f]

    for filepath in files:
        if not os.path.exists(filepath):
            continue
        try:
            with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                content = f.read()
                lines = content.split('\n')
        except Exception:
            continue

        debt = 0
        file_findings = []

        # 1. Dead code suppressions
        allow_matches = re.finditer(r'#\[allow\((dead_code|unused_imports|unreachable_code)\)\]', content)
        for match in allow_matches:
            debt += 5
            file_findings.append(f"Suppressed dead/unused code: {match.group(0)}")

        # 2. Orphaned TODOs
        for i, line in enumerate(lines):
            match = re.search(r'//\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', line, re.IGNORECASE)
            if not match:
                match = re.search(r'#\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', line, re.IGNORECASE)
            if match:
                tag = match.group(1).upper()
                desc = match.group(2).strip().lower()

                words = [w for w in re.split(r'\W+', desc) if len(w) > 3]
                if words:
                    found_in_todo = any(w in todo_text for w in words)
                else:
                    found_in_todo = desc in todo_text if desc else False

                if not found_in_todo and desc:
                    debt += 2
                    file_findings.append(f"Orphaned {tag} on line {i+1}: {match.group(2)}")

        # 3. Cyclomatic complexity
        if filepath.endswith('.rs'):
            in_function = False
            brace_depth = 0
            complexity = 1
            func_name = ""
            for i, line in enumerate(lines):
                func_match = re.match(r'^\s*(?:pub\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)', line)
                if func_match:
                    in_function = True
                    brace_depth = 0
                    complexity = 1
                    func_name = func_match.group(1)

                if in_function:
                    brace_depth += line.count('{') - line.count('}')
                    complexity += len(re.findall(r'\b(if|match|for|while|loop)\b', line))
                    complexity += line.count('?')
                    complexity += line.count('&&')
                    complexity += line.count('||')

                    if brace_depth <= 0 and '{' in content[:content.find(line)+len(line)]:
                        if complexity > 15:
                            debt += (complexity - 15)
                            file_findings.append(f"High cyclomatic complexity ({complexity}) in function '{func_name}'")
                        in_function = False

        # 4. Inconsistent error handling
        if filepath.endswith('.rs'):
            unwraps = content.count('.unwrap()')
            expects = content.count('.expect(')
            questions = content.count('?')
            matches = content.count('match ')

            if (unwraps > 0 or expects > 0) and (questions > 0 or matches > 0):
                debt += 3
                file_findings.append(f"Inconsistent error handling: {unwraps} unwrap, {expects} expect, {questions} ?, {matches} match")

        # 5. Duplicated structs and enums
        if filepath.endswith('.rs'):
            struct_matches = re.finditer(r'(struct|enum)\s+([A-Za-z0-9_]+)\s*\{', content)
            for sm in struct_matches:
                s_type = sm.group(1)
                s_name = sm.group(2)
                start_idx = sm.end()
                depth = 1
                body_end = -1
                for j in range(start_idx, len(content)):
                    if content[j] == '{': depth += 1
                    elif content[j] == '}': depth -= 1
                    if depth == 0:
                        body_end = j
                        break
                if body_end != -1:
                    body = content[start_idx:body_end]
                    clean_body = re.sub(r'//.*', '', body)
                    clean_body = re.sub(r'\s+', '', clean_body)
                    if len(clean_body) > 10:
                        if clean_body in structs and structs[clean_body] != s_name:
                            debt += 5
                            file_findings.append(f"Duplicated {s_type} body found in '{s_name}' (matches '{structs[clean_body]}')")
                        else:
                            structs[clean_body] = s_name

        # 6. Magic numbers/strings
        if filepath.endswith('.rs'):
            for i, line in enumerate(lines):
                if re.match(r'^\s*(pub\s+)?const\s+', line):
                    continue
                if re.search(r'let\s+[A-Za-z_]+\s*=\s*\d{3,};', line):
                    debt += 1
                    file_findings.append(f"Potential magic number on line {i+1}: {line.strip()}")
                if re.search(r'let\s+[A-Za-z_]+\s*=\s*"[^"]{10,}";', line) and not 'error' in line.lower() and not 'msg' in line.lower():
                    debt += 1
                    file_findings.append(f"Potential magic string on line {i+1}: {line.strip()}")

        if file_findings:
            file_debt[filepath] = debt
            total_debt += debt
            findings.append((filepath, debt, file_findings))

    return total_debt, file_debt, findings

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        sys.exit(1)

    print(f"Found {len(commits)} commits to analyze.")

    current_branch = run_cmd("git rev-parse --abbrev-ref HEAD")
    if current_branch == "HEAD":
        current_branch = commits[0]

    history = []

    for commit in reversed(commits):
        print(f"Checking out {commit}...")
        subprocess.run(f"git checkout {commit}", shell=True, stderr=subprocess.DEVNULL)
        total_debt, file_debt, findings = analyze_codebase()
        history.append({
            "commit": commit,
            "total_debt": total_debt,
            "file_debt": file_debt,
            "findings": findings
        })

    print(f"Restoring {current_branch}...")
    subprocess.run(f"git checkout {current_branch}", shell=True, stderr=subprocess.DEVNULL)

    current = history[-1]

    scorecard = "## 🏗️ Chief Architect Sanitation Scorecard\n\n"
    scorecard += f"**Current Commit:** `{current['commit'][:7]}`\n"
    scorecard += f"**Total Tech Debt Score:** {current['total_debt']}\n\n"

    scorecard += "### 📈 Trajectory (Last 6 Commits)\n"
    for h in history:
        scorecard += f"- `{h['commit'][:7]}`: {h['total_debt']} points\n"

    if len(history) > 1:
        prev_debt = history[-2]['total_debt']
        if current['total_debt'] < prev_debt:
            scorecard += "\n✅ **Status: Improving** (Debt decreased)\n"
        elif current['total_debt'] > prev_debt:
            scorecard += "\n❌ **Status: Degrading** (Debt increased)\n"
        else:
            scorecard += "\n➖ **Status: Stagnant** (Debt unchanged)\n"

    scorecard += "\n### 🚨 Increasing Debt Density\n"
    increasing_files = []
    if len(history) > 1:
        prev_file_debt = history[-2]['file_debt']
        curr_file_debt = current['file_debt']
        for f, d in curr_file_debt.items():
            if f in prev_file_debt and d > prev_file_debt[f]:
                increasing_files.append((f, prev_file_debt[f], d))
            elif f not in prev_file_debt and d > 0:
                increasing_files.append((f, 0, d))

    if increasing_files:
        for f, old, new in increasing_files:
            scorecard += f"- `{f}`: {old} ➡️ {new} points\n"
    else:
        scorecard += "_No files showed increasing debt density._\n"

    scorecard += "\n### 🔍 Key Findings (Current)\n"
    for filepath, debt, file_findings in sorted(current['findings'], key=lambda x: x[1], reverse=True)[:10]:
        scorecard += f"**{filepath}** (Debt: {debt})\n"
        for finding in file_findings[:5]:
            scorecard += f"- {finding}\n"
        if len(file_findings) > 5:
            scorecard += f"- ... and {len(file_findings) - 5} more issues.\n"
        scorecard += "\n"

    print(scorecard)

    print("--- FULL DETAILED FINDINGS ---")
    for filepath, debt, file_findings in current['findings']:
        print(f"\n{filepath} (Debt: {debt})")
        for finding in file_findings:
            print(f"  - {finding}")

    token = os.environ.get("GITHUB_TOKEN")
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    repo = os.environ.get("GITHUB_REPOSITORY")
    pr_number = os.environ.get("PR_NUMBER")

    if token and repo:
        if event_name == "push":
            commit_sha = current['commit']
            try:
                req = urllib.request.Request(
                    f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls",
                    headers={"Authorization": f"Bearer {token}", "Accept": "application/vnd.github.v3+json"}
                )
                with urllib.request.urlopen(req) as response:
                    pr_data = json.loads(response.read().decode())
                    if pr_data:
                        pr_number = pr_data[0]['number']
            except Exception as e:
                print(f"Failed to find PR for commit: {e}")

        if pr_number:
            print(f"Posting comment to PR #{pr_number}")
            try:
                data = json.dumps({"body": scorecard}).encode('utf-8')
                req = urllib.request.Request(
                    f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments",
                    data=data,
                    headers={"Authorization": f"Bearer {token}", "Accept": "application/vnd.github.v3+json", "Content-Type": "application/json"},
                    method="POST"
                )
                with urllib.request.urlopen(req) as response:
                    print("Comment posted successfully:", response.status)
            except Exception as e:
                print(f"Failed to post comment: {e}")
        else:
            print("No PR found to post comment to.")

if __name__ == "__main__":
    main()
