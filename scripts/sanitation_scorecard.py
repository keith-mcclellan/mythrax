#!/usr/bin/env python3
import os
import subprocess
import re
import urllib.request
import json

def run_cmd(cmd_list):
    try:
        return subprocess.check_output(cmd_list, text=True, stderr=subprocess.DEVNULL)
    except subprocess.CalledProcessError:
        return ""

def get_commits():
    os.system("git fetch --unshallow || git fetch --all")
    out = run_cmd(["git", "rev-list", "HEAD", "-n", "6"])
    return [c.strip() for c in out.splitlines() if c.strip()]

def get_file_content(commit, path):
    return run_cmd(["git", "show", f"{commit}:{path}"])

def analyze_commit(commit):
    out = run_cmd(["git", "ls-tree", "-r", commit, "--name-only"])
    files = out.splitlines()
    target_files = [f for f in files if (f.startswith("mythrax-core/") and f.endswith(".rs")) or f.startswith("scripts/")]
    todo_content = get_file_content(commit, "TODO.md").lower()

    metrics = {"dead_code": 0, "orphaned_todos": 0, "complex_functions": 0, "inconsistent_err": 0, "duplicate_structs": 0, "magic_literals": 0}
    file_debt = {}
    struct_bodies = {}

    for f in target_files:
        content = get_file_content(commit, f)
        if not content: continue
        debt = 0

        dead_count = content.count("#[allow(dead_code)]")
        metrics["dead_code"] += dead_count
        debt += dead_count

        todos = re.findall(r'(?:#|//).*?(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE)
        for t, rest in todos:
            rest_clean = rest.strip().lower()
            if len(rest_clean) > 5 and rest_clean not in todo_content:
                metrics["orphaned_todos"] += 1
                debt += 1

        if f.endswith(".rs"):
            lines = content.splitlines()
            in_func = False
            brace_count = 0
            func_complexity = 0
            for line in lines:
                if re.search(r'\bfn\s+\w+', line) and '{' in line:
                    in_func = True
                    brace_count = line.count('{') - line.count('}')
                    func_complexity = 1
                elif in_func:
                    brace_count += line.count('{') - line.count('}')
                    func_complexity += len(re.findall(r'\b(if|while|for|match)\b', line))
                    func_complexity += line.count('&&') + line.count('||') + line.count('?')
                    if brace_count <= 0:
                        in_func = False
                        if func_complexity > 15:
                            metrics["complex_functions"] += 1
                            debt += 1

            has_unwrap = 'unwrap()' in content or 'expect(' in content
            has_question = '?' in content
            if has_unwrap and has_question:
                metrics["inconsistent_err"] += 1
                debt += 1

            structs = []
            current_struct = ""
            in_struct = False
            struct_braces = 0
            for line in lines:
                if re.search(r'(?:struct|enum)\s+\w+', line) and not in_struct:
                    in_struct = True
                    current_struct = line
                    struct_braces = line.count('{') - line.count('}')
                elif in_struct:
                    current_struct += "\n" + line
                    struct_braces += line.count('{') - line.count('}')
                    if struct_braces <= 0:
                        in_struct = False
                        structs.append(current_struct)
                        current_struct = ""

            for s in structs:
                s_clean = re.sub(r'\s+', '', s)
                if len(s_clean) > 5:
                    if s_clean in struct_bodies:
                        metrics["duplicate_structs"] += 1
                        debt += 1
                    else:
                        struct_bodies[s_clean] = f

            magics = re.findall(r'\b(?:[2-9]|\d{2,})\b', content)
            metrics["magic_literals"] += len(magics)
            debt += len(magics) * 0.1

            magic_strs = re.findall(r'"[^"]{4,}"', content)
            metrics["magic_literals"] += len(magic_strs)
            debt += len(magic_strs) * 0.1

        file_debt[f] = debt

    return metrics, file_debt

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        return

    results = []
    for c in commits:
        m, f_debt = analyze_commit(c)
        results.append({"commit": c, "metrics": m, "file_debt": f_debt})

    scorecard = "## Chief Architect Sanitation Scorecard\n\n"
    scorecard += "| Commit | Dead Code | Orphaned TODOs | Complex Fns | Inconsistent Err | Dup Structs | Magic Lits |\n"
    scorecard += "|---|---|---|---|---|---|---|\n"
    for r in results:
        m = r["metrics"]
        scorecard += f"| {r['commit'][:7]} | {m['dead_code']} | {m['orphaned_todos']} | {m['complex_functions']} | {m['inconsistent_err']} | {m['duplicate_structs']} | {int(m['magic_literals'])} |\n"

    scorecard += "\n### File Debt Trajectory\n"
    if len(results) > 1:
        curr_debt = results[0]["file_debt"]
        prev_debt = results[1]["file_debt"]
        increasing_files = []
        for f, val in curr_debt.items():
            if val > prev_debt.get(f, 0):
                increasing_files.append(f)
        if increasing_files:
            scorecard += "⚠️ **Warning: Debt density is increasing in the following files:**\n"
            for f in increasing_files:
                scorecard += f"- `{f}`\n"
        else:
            scorecard += "✅ Debt density is stable or improving across all files.\n"

    print(scorecard)

    event_name = os.environ.get("GITHUB_EVENT_NAME")
    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GH_TOKEN")
    commit_sha = os.environ.get("GITHUB_SHA") or commits[0]

    if event_name == "push" and repo and token:
        url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
        req = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}", "Accept": "application/vnd.github.v3+json"})
        try:
            with urllib.request.urlopen(req) as response:
                prs = json.loads(response.read().decode())
                if prs:
                    pr_num = prs[0]["number"]
                    comment_url = f"https://api.github.com/repos/{repo}/issues/{pr_num}/comments"
                    data = json.dumps({"body": scorecard}).encode("utf-8")
                    req_post = urllib.request.Request(comment_url, data=data, headers={"Authorization": f"Bearer {token}", "Accept": "application/vnd.github.v3+json", "Content-Type": "application/json"}, method="POST")
                    urllib.request.urlopen(req_post)
                    print(f"Commented on PR #{pr_num}")
        except Exception as e:
            print(f"Failed to post PR comment: {e}")

if __name__ == "__main__":
    main()
