import os
import re
import subprocess
import json

def run_cmd(cmd):
    result = subprocess.run(cmd, capture_output=True, text=True)
    return result.stdout.strip()

def get_commits(n=6):
    stdout = run_cmd(["git", "log", "-n", str(n), "--format=%H"])
    return stdout.split()

def get_tracked_files(commit, paths):
    files = []
    for path in paths:
        stdout = run_cmd(["git", "ls-tree", "-r", "--name-only", commit, path])
        if stdout:
            files.extend(stdout.split('\n'))
    return [f for f in files if f.endswith('.rs') or f.endswith('.py') or f.endswith('.sh')]

def get_file_content(commit, filepath):
    return run_cmd(["git", "show", f"{commit}:{filepath}"])

def parse_todo_md():
    content = ""
    if os.path.exists("TODO.md"):
        with open("TODO.md", "r") as f:
            content = f.read().lower()
    return content

def analyze_file(content, filename, todo_md_content):
    score = 0
    issues = []

    # 1. Dead code suppression
    suppressions = re.findall(r'#\[allow\(([^)]+)\)\]', content)
    for supp in suppressions:
        if 'dead_code' in supp or 'unused_imports' in supp or 'unreachable_code' in supp:
            score += 5
            issues.append(f"Found debt suppression annotation: #[allow({supp})]")

    # 2. TODO, FIXME, HACK, TEMP
    todos = re.findall(r'(?i)\b(?:todo|fixme|hack|temp)\b\s*[:\-]*\s*(.*)', content)
    orphaned_todos = 0
    for todo_text in todos:
        todo_text_clean = todo_text.strip().lower()
        if not todo_text_clean:
            continue
        if todo_text_clean not in todo_md_content:
            orphaned_todos += 1

    if orphaned_todos > 0:
        score += orphaned_todos * 2
        issues.append(f"Found {orphaned_todos} orphaned TODO/FIXME/HACK/TEMP comments not in TODO.md")

    # 3. Cyclomatic complexity
    def get_complexity(body):
        return len(re.findall(r'\b(if|else|while|for|match|loop)\b', body)) + len(re.findall(r'(&&|\|\||\?)', body))

    idx = 0
    while True:
        idx = content.find("fn ", idx)
        if idx == -1:
            break
        start_brace = content.find("{", idx)
        if start_brace == -1:
            idx += 3
            continue

        brace_count = 1
        body_start = start_brace + 1
        curr = body_start
        while curr < len(content) and brace_count > 0:
            if content[curr] == '{':
                brace_count += 1
            elif content[curr] == '}':
                brace_count -= 1
            curr += 1

        body = content[body_start:curr]
        comp = get_complexity(body)
        if comp > 15:
            score += (comp - 15)
            issues.append(f"High cyclomatic complexity ({comp}) in a function")
        idx = curr

    # 4. Inconsistent error handling
    if filename.endswith('.rs'):
        unwraps = len(re.findall(r'\.unwrap\(\)', content))
        expects = len(re.findall(r'\.expect\(', content))
        qmarks = len(re.findall(r'\?', content))
        types_used = 0
        if unwraps > 0: types_used += 1
        if expects > 0: types_used += 1
        if qmarks > 0: types_used += 1

        if types_used > 1:
            score += 10
            issues.append(f"Inconsistent error handling: mixed unwrap({unwraps}), expect({expects}), ?({qmarks})")

    # 5. Duplicated struct/enum
    structs = re.findall(r'(struct|enum)\s+([A-zA-Z0-9_]+)\s*\{([^}]+)\}', content)
    seen = {}
    for stype, sname, sbody in structs:
        clean_body = re.sub(r'\s+', '', sbody)
        if clean_body in seen:
            score += 20
            issues.append(f"Duplicated {stype} definition: {sname} matches {seen[clean_body]}")
        else:
            seen[clean_body] = sname

    # 6. Magic numbers and string literals
    magic_nums = len(re.findall(r'(?<=\=)\s*[2-9][0-9]*\b', content))
    magic_strs = len(re.findall(r'(?<=\=)\s*"[^"]+"', content))
    if magic_nums > 5 or magic_strs > 5:
        score += (magic_nums + magic_strs)
        issues.append(f"Possible magic numbers ({magic_nums}) or strings ({magic_strs})")

    lines = max(1, len(content.splitlines()))
    density = score / lines * 100

    return score, density, issues

def main():
    commits = get_commits(6)
    if not commits:
        return

    todo_md_content = parse_todo_md()
    paths_to_scan = ["mythrax-core", "scripts"]

    history = []

    for commit in commits:
        files = get_tracked_files(commit, paths_to_scan)
        commit_score = 0
        file_metrics = {}
        for f in files:
            content = get_file_content(commit, f)
            score, density, issues = analyze_file(content, f, todo_md_content)
            commit_score += score
            file_metrics[f] = {'score': score, 'density': density, 'issues': issues}

        history.append({
            'commit': commit,
            'total_score': commit_score,
            'files': file_metrics
        })

    current = history[0]
    older = history[1:]

    report = ["# Sanitation Scorecard", ""]
    report.append(f"Current Commit: {current['commit']}")
    report.append(f"Total Debt Score: {current['total_score']}")
    report.append("")
    report.append("## Trajectory (Last 6 commits)")
    for i, h in enumerate(reversed(history)):
        report.append(f"- {h['commit'][:7]}: {h['total_score']} debt score")
    report.append("")

    report.append("## Files with Increasing Debt Density")
    flagged_files = set()
    for f, metrics in current['files'].items():
        curr_density = metrics['density']
        for h in older:
            if f in h['files']:
                prev_density = h['files'][f]['density']
                if curr_density > prev_density:
                    flagged_files.add(f)
                    report.append(f"- **{f}**: Density increased from {prev_density:.2f} to {curr_density:.2f}")
                    for issue in set(metrics['issues']):
                        report.append(f"  - {issue}")
                break

    if not flagged_files:
        report.append("No files with increasing debt density found. Good job!")

    report.append("")
    report_text = "\n".join(report)
    print(report_text)

    if os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_EVENT_NAME") == "push":
        try:
            subprocess.run(["gh", "pr", "comment", "-b", report_text], check=False)
        except Exception as e:
            pass

if __name__ == "__main__":
    main()
