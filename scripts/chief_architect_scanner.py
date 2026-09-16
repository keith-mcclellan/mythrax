import os, sys, re, subprocess, urllib.request, urllib.error, json

def get_file_content(commit, filepath):
    if commit == 'working_tree':
        if os.path.exists(filepath):
            with open(filepath, 'r', encoding='utf-8') as f:
                return f.read()
        return None
    else:
        try:
            return subprocess.check_output(['git', 'show', f'{commit}:{filepath}'], stderr=subprocess.DEVNULL).decode('utf-8')
        except subprocess.CalledProcessError:
            return None

def get_files_in_commit(commit):
    if commit == 'working_tree':
        files = []
        for root, _, filenames in os.walk('.'):
            if '.git' in root: continue
            for fn in filenames:
                path = os.path.join(root, fn).replace('./', '', 1)
                if path.startswith('mythrax-core/') and path.endswith('.rs'):
                    files.append(path)
                elif path.startswith('scripts/') and (path.endswith('.py') or path.endswith('.sh')):
                    files.append(path)
        return files
    else:
        try:
            out = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', commit]).decode('utf-8')
            files = []
            for line in out.strip().split('\n'):
                if line.startswith('mythrax-core/') and line.endswith('.rs'):
                    files.append(line)
                elif line.startswith('scripts/') and (line.endswith('.py') or line.endswith('.sh')):
                    files.append(line)
            return files
        except subprocess.CalledProcessError:
            return []

def analyze_file_content(content, filepath, todo_md_content, global_structs):
    debts = []

    # 1. Dead code, unused imports, unreachable branches
    for m in re.finditer(r'#\[allow\(\s*(dead_code|unused_imports|unreachable_code)\s*\)\]', content):
        debts.append("Dead code suppression")

    # 2. TODO, FIXME, HACK, TEMP
    for m in re.finditer(r'(?://|#)\s*(TODO|FIXME|HACK|TEMP)(.*)', content, re.IGNORECASE):
        ctype = m.group(1).upper()
        ctext = m.group(2).strip()
        if todo_md_content:
            words = [w for w in re.split(r'\W+', ctext.lower()) if len(w) > 3]
            found = False
            for w in words:
                if w in todo_md_content.lower():
                    found = True
                    break
            if not found:
                debts.append(f"Orphaned {ctype} comment")
            else:
                debts.append(f"{ctype} comment")
        else:
            debts.append(f"{ctype} comment")

    # 3. Inconsistent error handling
    if filepath.endswith('.rs'):
        has_unwrap = 'unwrap()' in content
        has_expect = 'expect(' in content
        has_question = '?' in content
        has_match = 'match ' in content and ('Err(' in content or 'Ok(' in content)
        if sum([has_unwrap, has_expect, has_question, has_match]) > 1:
            debts.append("Inconsistent error handling")

    # 4. Cyclomatic complexity & Duplicates & Magic Numbers
    lines = content.split('\n')
    brace_depth = 0
    current_func = None
    base_depth = 0
    complexity = 1

    struct_body = []
    current_struct = None
    struct_base_depth = 0

    for line in lines:
        stripped = ""
        in_string = False
        i = 0
        while i < len(line):
            c = line[i]
            if in_string:
                if c == '\\': i += 1
                elif c == '"': in_string = False
            else:
                if c == '"': in_string = True
                elif (c == '/' or c == '#') and not filepath.endswith('.rs'):
                    break
                elif c == '/' and filepath.endswith('.rs') and i + 1 < len(line) and line[i+1] == '/':
                    break
                else:
                    stripped += c
            i += 1

        if filepath.endswith('.rs'):
            m = re.search(r'\bfn\s+([a-zA-Z0-9_]+)\s*\(', stripped)
            if m and current_func is None:
                current_func = m.group(1)
                base_depth = brace_depth
                complexity = 1

            m_struct = re.search(r'\b(?:struct|enum)\s+([a-zA-Z0-9_]+)\s*\{', stripped)
            if m_struct and current_struct is None:
                current_struct = m_struct.group(1)
                struct_base_depth = brace_depth
                struct_body = []

            if current_struct is not None:
                struct_body.append(stripped)

            if current_func is not None:
                complexity += len(re.findall(r'\b(if|while|for|match)\b', stripped))
                complexity += stripped.count('&&') + stripped.count('||') + stripped.count('?')

                nums = re.findall(r'\b\d+\b', stripped)
                for n in nums:
                    if n not in ('0', '1', '2'):
                        debts.append("Magic number")
                        break

            for c in stripped:
                if c == '{':
                    brace_depth += 1
                elif c == '}':
                    brace_depth -= 1
                    if current_func is not None and brace_depth == base_depth:
                        if complexity > 15:
                            debts.append("Excessive cyclomatic complexity (>15)")
                        current_func = None
                    if current_struct is not None and brace_depth == struct_base_depth:
                        body_str = "".join(struct_body)
                        body_hash = hash(re.sub(r'\s+', '', body_str))
                        if body_hash in global_structs:
                            debts.append(f"Duplicated struct/enum definition ({current_struct})")
                        else:
                            global_structs.add(body_hash)
                        current_struct = None

    return debts

def analyze_commit(commit):
    files = get_files_in_commit(commit)
    todo_md_content = get_file_content(commit, 'TODO.md')

    total_debts = 0
    file_debts = {}
    global_structs = set()

    for f in files:
        content = get_file_content(commit, f)
        if not content: continue
        debts = analyze_file_content(content, f, todo_md_content, global_structs)
        total_debts += len(debts)
        file_debts[f] = len(debts)

    return total_debts, file_debts

def post_pr_comment(comment):
    token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    sha = os.environ.get('GITHUB_SHA')

    if not token or not repo or not sha:
        print("Missing GITHUB_TOKEN, GITHUB_REPOSITORY, or GITHUB_SHA")
        return

    req = urllib.request.Request(f"https://api.github.com/repos/{repo}/commits/{sha}/pulls")
    req.add_header('Authorization', f'token {token}')
    req.add_header('Accept', 'application/vnd.github.v3+json')

    try:
        with urllib.request.urlopen(req) as response:
            prs = json.loads(response.read().decode())
            if not prs:
                print("No PR found for this commit.")
                return
            pr_number = prs[0]['number']
    except Exception as e:
        print(f"Error fetching PR: {e}")
        return

    req = urllib.request.Request(f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments", method='POST')
    req.add_header('Authorization', f'token {token}')
    req.add_header('Accept', 'application/vnd.github.v3+json')
    data = json.dumps({'body': comment}).encode('utf-8')

    try:
        urllib.request.urlopen(req, data=data)
        print("Successfully posted PR comment.")
    except Exception as e:
        print(f"Error posting comment: {e}")

def main():
    try:
        status = subprocess.check_output(['git', 'status', '--porcelain']).decode('utf-8').strip()
        has_uncommitted = len(status) > 0
    except Exception:
        has_uncommitted = False

    try:
        commits = subprocess.check_output(['git', 'log', '-n', '6', '--format=%H']).decode('utf-8').strip().split('\n')
    except Exception:
        commits = []

    states = []
    if has_uncommitted:
        states.append('working_tree')
        states.extend(commits[:5])
    else:
        states.extend(commits[:6])

    if not states:
        print("No Git history found.")
        return

    results = []
    for state in states:
        total, file_debts = analyze_commit(state)
        results.append((state, total, file_debts))

    scorecard = "## Chief Architect Sanitation Scorecard\n\n"
    scorecard += "| State | Total Debt |\n|---|---|\n"

    for state, total, _ in results:
        short_state = state[:7] if state != 'working_tree' else 'Working Tree'
        scorecard += f"| {short_state} | {total} |\n"

    current_total = results[0][1]
    prev_total = results[-1][1]

    if current_total < prev_total:
        scorecard += "\n**Trajectory:** 🟢 Improving (Debt decreased)\n"
    elif current_total > prev_total:
        scorecard += "\n**Trajectory:** 🔴 Degrading (Debt increased)\n"
    else:
        scorecard += "\n**Trajectory:** 🟡 Stable (Debt unchanged)\n"

    if len(results) > 1:
        current_file_debts = results[0][2]
        prev_file_debts = results[1][2]

        increased_files = []
        for f, d in current_file_debts.items():
            prev_d = prev_file_debts.get(f, 0)
            if d > prev_d:
                increased_files.append((f, d - prev_d))

        if increased_files:
            scorecard += "\n### 🚩 Files with Increasing Debt Density\n"
            for f, inc in increased_files:
                scorecard += f"- `{f}` (+{inc} debt items)\n"

    print(scorecard)

    if os.environ.get('GITHUB_EVENT_NAME') == 'push':
        post_pr_comment(scorecard)

if __name__ == '__main__':
    main()
