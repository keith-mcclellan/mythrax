#!/usr/bin/env python3
import os
import re
import sys
import subprocess
import urllib.request
import urllib.error
import json

def get_files_at_commit(commit_hash):
    if commit_hash:
        try:
            res = subprocess.run(["git", "ls-tree", "-r", "--name-only", commit_hash], capture_output=True, text=True, check=True)
            return [f for f in res.stdout.split('\n') if f.strip()]
        except:
            return []
    else:
        files = []
        for root, _, fnames in os.walk('.'):
            if '.git' in root: continue
            for f in fnames:
                files.append(os.path.relpath(os.path.join(root, f)))
        return files

def get_file_content_at_commit(commit_hash, filepath):
    if commit_hash:
        try:
            res = subprocess.run(["git", "show", f"{commit_hash}:{filepath}"], capture_output=True, text=True)
            if res.returncode == 0:
                return res.stdout
        except:
            pass
        return ""
    else:
        try:
            with open(filepath, 'r') as f:
                return f.read()
        except:
            return ""

def read_todo_md(commit_hash):
    content = get_file_content_at_commit(commit_hash, "TODO.md")
    return content.lower() if content else ""

def iter_code_lines(content):
    in_multiline_comment = False
    for line_num, original_line in enumerate(content.split('\n'), 1):
        clean_line = ""
        i = 0
        while i < len(original_line):
            if in_multiline_comment:
                if i + 1 < len(original_line) and original_line[i] == '*' and original_line[i+1] == '/':
                    in_multiline_comment = False
                    i += 2
                else:
                    i += 1
            else:
                if i + 1 < len(original_line) and original_line[i] == '/' and original_line[i+1] == '*':
                    in_multiline_comment = True
                    i += 2
                elif i + 1 < len(original_line) and original_line[i] == '/' and original_line[i+1] == '/':
                    break
                elif original_line[i] == '"' or original_line[i] == "'":
                    quote = original_line[i]
                    i += 1
                    while i < len(original_line):
                        if original_line[i] == '\\':
                            i += 2
                        elif original_line[i] == quote:
                            i += 1
                            break
                        else:
                            i += 1
                    clean_line += '""'
                else:
                    clean_line += original_line[i]
                    i += 1
        yield line_num, original_line, clean_line

def analyze_file(filepath, content, todo_md_content, is_rust):
    findings = []
    debt_score = 0
    structs_enums = []

    dead_code_re = re.compile(r'#\[allow\((dead_code|unused_imports|unreachable_code)\)\]')
    todo_re = re.compile(r'\b(TODO|FIXME|HACK|TEMP)\b', re.IGNORECASE)

    in_function = False
    func_brace_depth_base = 0
    current_brace_depth = 0
    func_complexity = 0
    func_name = ""

    has_unwrap = False
    has_expect = False
    has_question = False
    has_match_err = False

    for line_num, orig_line, clean_line in iter_code_lines(content):
        for char in clean_line:
            if char == '{':
                current_brace_depth += 1
            elif char == '}':
                current_brace_depth -= 1

        if is_rust:
            if not in_function:
                m = re.search(r'\bfn\s+([a-zA-Z0-9_]+)\s*\(', clean_line)
                if m:
                    in_function = True
                    func_name = m.group(1)
                    func_brace_depth_base = current_brace_depth - clean_line.count('{')
                    func_complexity = 1
            else:
                func_complexity += len(re.findall(r'\b(if|while|for|match)\b', clean_line))
                func_complexity += clean_line.count('&&') + clean_line.count('||') + clean_line.count('?')
                if current_brace_depth <= func_brace_depth_base:
                    in_function = False
                    if func_complexity > 15:
                        findings.append(f"Line {line_num}: Function '{func_name}' has cyclomatic complexity {func_complexity} (>15)")
                        debt_score += (func_complexity - 15)

            if dead_code_re.search(clean_line):
                findings.append(f"Line {line_num}: Suppression of dead/unused/unreachable code found")
                debt_score += 5

            m_struct = re.search(r'\b(struct|enum)\s+([a-zA-Z0-9_]+)', clean_line)
            if m_struct:
                structs_enums.append(m_struct.group(2))

            if '.unwrap()' in clean_line: has_unwrap = True
            if '.expect(' in clean_line: has_expect = True
            if '?' in clean_line: has_question = True
            if 'Err(' in clean_line: has_match_err = True

            if '=' in clean_line or 'return' in clean_line or '(' in clean_line:
                if re.search(r'\b[2-9]\b|\b[1-9][0-9]+\b', clean_line) and not re.search(r'\b(const|static)\s+', clean_line):
                    debt_score += 1

        for m in todo_re.finditer(orig_line):
            todo_text = orig_line[m.start():].strip()
            words = [w.lower() for w in re.findall(r'[a-zA-Z]{4,}', todo_text)]
            tracked = False
            if words:
                for w in words:
                    if w in todo_md_content:
                        tracked = True
                        break
            if not tracked:
                findings.append(f"Line {line_num}: Untracked {m.group(1).upper()} found")
                debt_score += 3

    if is_rust and sum([has_unwrap, has_expect, has_question, has_match_err]) > 1:
        findings.append(f"Inconsistent error handling patterns mixed in file")
        debt_score += 10

    return findings, debt_score, structs_enums

def run_analysis(commit_hash):
    files = get_files_at_commit(commit_hash)
    todo_md = read_todo_md(commit_hash)

    total_debt = 0
    file_debts = {}
    all_findings = {}
    all_structs = []

    for f in files:
        if not (f.startswith('mythrax-core/') or f.startswith('scripts/')):
            continue

        is_rust = f.endswith('.rs')
        is_script = f.endswith('.py') or f.endswith('.sh')
        if not (is_rust or is_script):
            continue

        content = get_file_content_at_commit(commit_hash, f)
        if not content:
            continue

        findings, debt, structs = analyze_file(f, content, todo_md, is_rust)
        all_structs.extend(structs)

        if debt > 0:
            file_debts[f] = debt
            total_debt += debt
        if findings:
            all_findings[f] = findings

    from collections import Counter
    struct_counts = Counter(all_structs)
    duplicates = [k for k, v in struct_counts.items() if v > 1]

    return total_debt, file_debts, all_findings, duplicates

def get_recent_commits():
    try:
        res = subprocess.run(["git", "log", "-n", "6", "--format=%H"], capture_output=True, text=True, check=True)
        return [c for c in res.stdout.split('\n') if c.strip()]
    except:
        return []

def main():
    commits = get_recent_commits()

    status = subprocess.run(["git", "status", "--porcelain"], capture_output=True, text=True).stdout.strip()
    states = [None] + commits[:5] if status else commits[:6]

    results = []
    for state in states:
        total_debt, file_debts, all_findings, duplicates = run_analysis(state)
        results.append({
            "commit": state if state else "UNCOMMITTED",
            "total_debt": total_debt,
            "file_debts": file_debts,
            "findings": all_findings,
            "duplicates": duplicates
        })

    if not results:
        print("No results to report.")
        return

    current = results[0]

    scorecard = "## 🏗️ Chief Architect Sanitation Scorecard\n\n"
    scorecard += f"**Current Debt Score:** {current['total_debt']}\n\n"

    scorecard += "### 📉 Trajectory (Last 5 Commits)\n"
    for r in results:
        commit_short = r['commit'][:7] if r['commit'] != "UNCOMMITTED" else "Working Tree (or current commit)"
        scorecard += f"- {commit_short}: {r['total_debt']} debt points\n"

    if len(results) > 1:
        prev = results[1]
        increasing_files = []
        for f, debt in current['file_debts'].items():
            prev_debt = prev['file_debts'].get(f, 0)
            if debt > prev_debt:
                increasing_files.append((f, prev_debt, debt))

        if increasing_files:
            scorecard += "\n### ⚠️ Debt Increasing Files\n"
            for f, prev_d, curr_d in increasing_files:
                scorecard += f"- `{f}`: {prev_d} -> {curr_d} (+{curr_d - prev_d})\n"

    if current['duplicates']:
        scorecard += "\n### 👯 Duplicated Structures (Potential Unification)\n"
        scorecard += ", ".join(f"`{d}`" for d in current['duplicates'][:10])
        if len(current['duplicates']) > 10:
            scorecard += "..."
        scorecard += "\n"

    scorecard += "\n### 🔍 Key Findings (Current State)\n"
    count = 0
    for f, findings in current['findings'].items():
        if not findings: continue
        scorecard += f"**{f}**\n"
        for finding in findings[:3]:
            scorecard += f"- {finding}\n"
        count += 1
        if count >= 5:
            break

    print(scorecard)

    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "push":
        token = os.environ.get("GITHUB_TOKEN")
        repo = os.environ.get("GITHUB_REPOSITORY")
        sha = os.environ.get("GITHUB_SHA")
        if token and repo and sha:
            url = f"https://api.github.com/repos/{repo}/commits/{sha}/pulls"
            req = urllib.request.Request(url, headers={
                "Authorization": f"token {token}",
                "Accept": "application/vnd.github.v3+json",
                "User-Agent": "ChiefArchitectScanner"
            })
            try:
                with urllib.request.urlopen(req) as resp:
                    pr_data = json.loads(resp.read().decode())
                    if pr_data and isinstance(pr_data, list) and len(pr_data) > 0:
                        pr_number = pr_data[0]["number"]
                        comments_url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments?per_page=100"

                        existing_comment_id = None
                        creq = urllib.request.Request(comments_url, headers={
                            "Authorization": f"token {token}",
                            "Accept": "application/vnd.github.v3+json",
                            "User-Agent": "ChiefArchitectScanner"
                        })
                        with urllib.request.urlopen(creq) as cresp:
                            comments = json.loads(cresp.read().decode())
                            for comment in comments:
                                if "## 🏗️ Chief Architect Sanitation Scorecard" in comment.get("body", ""):
                                    existing_comment_id = comment["id"]
                                    break

                        if existing_comment_id:
                            update_url = f"https://api.github.com/repos/{repo}/issues/comments/{existing_comment_id}"
                            patch_req = urllib.request.Request(update_url, headers={
                                "Authorization": f"token {token}",
                                "Accept": "application/vnd.github.v3+json",
                                "User-Agent": "ChiefArchitectScanner",
                                "Content-Type": "application/json"
                            }, method="PATCH", data=json.dumps({"body": scorecard}).encode())
                            urllib.request.urlopen(patch_req)
                            print("Updated existing PR comment.")
                        else:
                            post_url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
                            post_req = urllib.request.Request(post_url, headers={
                                "Authorization": f"token {token}",
                                "Accept": "application/vnd.github.v3+json",
                                "User-Agent": "ChiefArchitectScanner",
                                "Content-Type": "application/json"
                            }, method="POST", data=json.dumps({"body": scorecard}).encode())
                            urllib.request.urlopen(post_req)
                            print("Posted new PR comment.")
            except Exception as e:
                print(f"Error posting PR comment: {e}")

if __name__ == "__main__":
    main()
