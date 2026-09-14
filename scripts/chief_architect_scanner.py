import os
import re
import subprocess
import json
import urllib.request

def get_tracked_todos(commit_hash=None):
    content = ""
    if commit_hash:
        try:
            content = subprocess.run(['git', 'show', f"{commit_hash}:TODO.md"], capture_output=True, text=True, check=True).stdout
        except:
            pass
    else:
        try:
            with open("TODO.md", "r") as f:
                content = f.read()
        except:
            pass
    return content.lower()

def scan_file_content(content, filepath, todo_content, is_rust=True):
    debt_items = []

    dead_code_count = len(re.findall(r'#\[allow\(.*(?:dead_code|unused).*?\)\]', content))
    if dead_code_count > 0:
        debt_items.append(f"{dead_code_count} dead_code/unused suppressions")

    todos = re.findall(r'(?i)\b(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', content)
    untracked_count = 0
    for t_type, t_text in todos:
        t_text_lower = t_text.strip().lower()
        if len(t_text_lower) > 5 and t_text_lower not in todo_content:
            untracked_count += 1
    if untracked_count > 0:
        debt_items.append(f"{untracked_count} untracked {t_type}")

    lines = content.splitlines()
    brace_depth = 0
    in_function = False
    func_start_depth = 0
    func_name = ""
    func_complexity = 1

    in_struct = False
    struct_start_depth = 0
    struct_name = ""
    struct_body = []
    structs = {}

    has_unwrap = False
    has_expect = False
    has_qmark = False

    for line in lines:
        open_braces = line.count('{') - line.count('"{') - line.count('\'{')
        close_braces = line.count('}') - line.count('"}') - line.count('\'}')

        if is_rust:
            if not in_function and not in_struct:
                m_func = re.search(r'fn\s+([a-zA-Z0-9_]+)', line)
                if m_func and '{' in line:
                    in_function = True
                    func_name = m_func.group(1)
                    func_start_depth = brace_depth
                    func_complexity = 1

                m_struct = re.search(r'(?:struct|enum)\s+([a-zA-Z0-9_]+)', line)
                if m_struct and '{' in line:
                    in_struct = True
                    struct_name = m_struct.group(1)
                    struct_start_depth = brace_depth
                    struct_body = []

            if in_function:
                func_complexity += len(re.findall(r'\b(if|while|for|match)\b', line))
                func_complexity += line.count('?') + line.count('&&') + line.count('||')

                if re.search(r'\b\d{3,}\b', line):
                    debt_items.append(f"Magic number in {func_name}")
                if re.search(r'"[^"]{5,}"', line) and not 'println!' in line and not 'format!' in line and not '#[doc' in line and not line.strip().startswith('//') and not 'panic!' in line and not 'assert' in line:
                    debt_items.append(f"String literal in {func_name} (possible magic string)")

            if in_struct:
                struct_body.append(line.strip())

            if 'unwrap(' in line: has_unwrap = True
            if 'expect(' in line: has_expect = True
            if '?' in line: has_qmark = True

            brace_depth += open_braces
            brace_depth -= close_braces

            if in_function and brace_depth == func_start_depth:
                in_function = False
                if func_complexity > 15:
                    debt_items.append(f"Function {func_name} has complexity {func_complexity} (>15)")

            if in_struct and brace_depth == struct_start_depth:
                in_struct = False
                if len(struct_body) > 2:
                    norm_body = "".join(struct_body[1:-1])
                    structs[struct_name] = norm_body

    if is_rust and (has_unwrap or has_expect) and has_qmark:
        debt_items.append("Inconsistent error handling (mixing unwrap/expect and ?)")

    return debt_items, structs

def get_commits():
    try:
        res = subprocess.run(['git', 'log', '-n', '6', '--format=%H'], capture_output=True, text=True, check=True)
        commits = res.stdout.strip().splitlines()
        return commits[1:6] if len(commits) > 1 else []
    except Exception as e:
        print(f"Error getting commits: {e}")
        return []

def scan_commit(commit_hash):
    todo_content = get_tracked_todos(commit_hash)
    total_debt = 0
    all_structs = {}

    try:
        res = subprocess.run(['git', 'ls-tree', '-r', '--name-only', commit_hash], capture_output=True, text=True, check=True)
        files = [f for f in res.stdout.splitlines() if (f.endswith('.rs') or f.endswith('.py') or f.endswith('.sh')) and ('mythrax-core' in f or 'scripts' in f)]
    except:
        return 0

    for f in files:
        try:
            content = subprocess.run(['git', 'show', f"{commit_hash}:{f}"], capture_output=True, text=True, check=True).stdout
            is_rust = f.endswith('.rs')
            debt, structs = scan_file_content(content, f, todo_content, is_rust)
            total_debt += len(debt)

            for s_name, s_body in structs.items():
                if len(s_body) > 10:
                    if s_body in all_structs:
                        total_debt += 1
                    else:
                        all_structs[s_body] = s_name
        except:
            pass

    return total_debt

def scan_current():
    todo_content = get_tracked_todos(None)

    files = []
    for root, _, filenames in os.walk('.'):
        if '.git' in root or 'target' in root: continue
        for filename in filenames:
            if filename.endswith(('.rs', '.py', '.sh')) and ('mythrax-core' in root or 'scripts' in root):
                files.append(os.path.join(root, filename))

    total_debt = 0
    all_structs = {}
    file_debts = {}

    for f in files:
        try:
            with open(f, 'r') as file:
                content = file.read()
            is_rust = f.endswith('.rs')
            debt, structs = scan_file_content(content, f, todo_content, is_rust)

            f_clean = f[2:] if f.startswith('./') else f

            for s_name, s_body in structs.items():
                if len(s_body) > 10:
                    if s_body in all_structs:
                        debt.append(f"Duplicate struct/enum: {s_name} is duplicate of {all_structs[s_body]}")
                    else:
                        all_structs[s_body] = s_name

            total_debt += len(debt)
            file_debts[f_clean] = len(debt)
        except Exception as e:
            pass

    return total_debt, file_debts

def compare_file_debts(current_debts, commit_hash):
    todo_content = get_tracked_todos(commit_hash)
    increasing_files = []

    try:
        res = subprocess.run(['git', 'ls-tree', '-r', '--name-only', commit_hash], capture_output=True, text=True, check=True)
        past_files = [f for f in res.stdout.splitlines() if (f.endswith('.rs') or f.endswith('.py') or f.endswith('.sh')) and ('mythrax-core' in f or 'scripts' in f)]
    except:
        return []

    for f in past_files:
        try:
            content = subprocess.run(['git', 'show', f"{commit_hash}:{f}"], capture_output=True, text=True, check=True).stdout
            is_rust = f.endswith('.rs')
            debt, _ = scan_file_content(content, f, todo_content, is_rust)
            past_debt_count = len(debt)
            curr_debt_count = current_debts.get(f, 0)

            if curr_debt_count > past_debt_count:
                increasing_files.append((f, past_debt_count, curr_debt_count))
        except:
            pass

    for f, curr_debt_count in current_debts.items():
        if f not in past_files and curr_debt_count > 0:
            increasing_files.append((f, 0, curr_debt_count))

    return increasing_files

def main():
    commits = get_commits()
    current_total, current_file_debts = scan_current()

    history_scores = []
    for c in commits:
        score = scan_commit(c)
        history_scores.append((c, score))

    increasing = []
    if len(commits) > 0:
        increasing = compare_file_debts(current_file_debts, commits[0])

    report = "### Chief Architect Sanitation Scorecard\n\n"
    report += "| State | Total Debt Items |\n"
    report += "|---|---|\n"
    report += f"| Current | {current_total} |\n"
    for c, score in history_scores:
        report += f"| {c[:7]} | {score} |\n"

    report += "\n#### Files with Increasing Debt Density\n"
    if increasing:
        for f, past, curr in increasing:
            report += f"- `{f}`: {past} -> {curr} items\n"
    else:
        report += "- None\n"

    print(report)
    with open('sanitation_scorecard.md', 'w') as f:
        f.write(report)

    if os.environ.get('GITHUB_EVENT_NAME') == 'push':
        token = os.environ.get('GITHUB_TOKEN')
        repo = os.environ.get('GITHUB_REPOSITORY')
        sha = os.environ.get('GITHUB_SHA')
        if token and repo and sha:
            req = urllib.request.Request(f"https://api.github.com/repos/{repo}/commits/{sha}/pulls")
            req.add_header("Authorization", f"Bearer {token}")
            req.add_header("Accept", "application/vnd.github+json")
            req.add_header("User-Agent", "Chief-Architect-Script")
            try:
                with urllib.request.urlopen(req) as response:
                    prs = json.loads(response.read().decode())
                    if prs:
                        pr_number = prs[0]['number']
                        req_comments = urllib.request.Request(f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments")
                        req_comments.add_header("Authorization", f"Bearer {token}")
                        req_comments.add_header("Accept", "application/vnd.github+json")
                        req_comments.add_header("User-Agent", "Chief-Architect-Script")
                        with urllib.request.urlopen(req_comments) as resp_comments:
                            comments = json.loads(resp_comments.read().decode())

                        # Delete old comments from this script
                        for c in comments:
                            if "Chief Architect Sanitation Scorecard" in c.get('body', ''):
                                c_id = c['id']
                                req_del = urllib.request.Request(f"https://api.github.com/repos/{repo}/issues/comments/{c_id}", method="DELETE")
                                req_del.add_header("Authorization", f"Bearer {token}")
                                req_del.add_header("Accept", "application/vnd.github+json")
                                req_del.add_header("User-Agent", "Chief-Architect-Script")
                                try:
                                    urllib.request.urlopen(req_del)
                                except:
                                    pass

                        # Post new comment
                        data = json.dumps({"body": report}).encode('utf-8')
                        req_post = urllib.request.Request(f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments", data=data, method="POST")
                        req_post.add_header("Authorization", f"Bearer {token}")
                        req_post.add_header("Accept", "application/vnd.github+json")
                        req_post.add_header("Content-Type", "application/json")
                        req_post.add_header("User-Agent", "Chief-Architect-Script")
                        urllib.request.urlopen(req_post)
            except Exception as e:
                print(f"Error posting PR comment: {e}")

if __name__ == "__main__":
    main()
