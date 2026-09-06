import os
import re
import subprocess
import json
import urllib.request
from collections import defaultdict

def find_rs_files(directory):
    rs_files = []
    for root, _, files in os.walk(directory):
        for file in files:
            if file.endswith((".rs", ".py", ".sh")):
                rs_files.append(os.path.join(root, file))
    return rs_files

def get_todo_md_items():
    if not os.path.exists("TODO.md"):
        return ""
    with open("TODO.md", 'r') as f:
        return f.read().lower()

def extract_structs_enums(content, filepath):
    definitions = []
    matches = re.finditer(r'(?:pub\s+)?(?:struct|enum)\s+([A-Z][a-zA-Z0-9_]*)', content)
    for match in matches:
        definitions.append((match.group(1), filepath))
    return definitions

def calculate_debt(commit_hash):
    subprocess.run(["git", "checkout", "-q", commit_hash])

    files = find_rs_files("mythrax-core")
    files.extend(find_rs_files("scripts"))

    todo_text = get_todo_md_items()

    total_debt = 0
    file_debt = {}

    all_structs = defaultdict(list)

    for filepath in files:
        if not os.path.exists(filepath): continue
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            content = f.read()

        debt_score = 0
        issues = []

        # Dead code
        dead_code_matches = len(re.findall(r'#\[allow\(dead_code\)\]', content))
        if dead_code_matches > 0:
            debt_score += dead_code_matches * 5
            issues.append(f"{dead_code_matches} dead_code suppressions")

        unused_imports = len(re.findall(r'#\[allow\(unused_imports\)\]', content))
        if unused_imports > 0:
            debt_score += unused_imports * 5
            issues.append(f"{unused_imports} unused_imports suppressions")

        # TODO/FIXME/HACK/TEMP
        for keyword in ["TODO", "FIXME", "HACK", "TEMP"]:
            matches = re.finditer(rf'(?://|#)\s*({keyword}.*)', content, re.IGNORECASE)
            for match in matches:
                comment_text = match.group(1)

                words = [w for w in re.split(r'\W+', comment_text.lower()) if len(w) > 3 and w not in ['todo', 'fixme', 'hack', 'temp']]
                orphaned = True
                if words:
                    if any(word in todo_text for word in words):
                        orphaned = False

                if orphaned:
                    debt_score += 10
                    issues.append(f"Orphaned {keyword}: {comment_text[:40]}")

        # Complexity
        if filepath.endswith(".rs"):
            functions = re.finditer(r'fn\s+\w+\s*(?:<[^>]+>)?\s*\([^)]*\)\s*(?:->\s*[^\{]+)?\s*\{', content, re.DOTALL)
            for func in functions:
                start_idx = func.end()
                brace_count = 1
                idx = start_idx
                while brace_count > 0 and idx < len(content):
                    if content[idx] == '{': brace_count += 1
                    elif content[idx] == '}': brace_count -= 1
                    idx += 1

                func_body = content[start_idx:idx]
                complexity = 1 + len(re.findall(r'\b(if|while|for|match)\b', func_body)) + len(re.findall(r'(\&\&|\|\||\?)', func_body))
                if complexity > 15:
                    debt_score += (complexity - 15) * 2
                    func_name = re.search(r'fn\s+(\w+)', func.group(0)).group(1)
                    issues.append(f"Complex function '{func_name}' (score {complexity})")

            # Error handling mixing
            error_counts = {
                'unwrap': len(re.findall(r'\.unwrap\(\)', content)),
                'expect': len(re.findall(r'\.expect\(', content)),
                'question': len(re.findall(r'\?', content)),
                'match': len(re.findall(r'match\s+[^\{]+\{', content))
            }
            active = sum(1 for k, v in error_counts.items() if v > 0)
            if active >= 2 and (error_counts['unwrap'] > 0 or error_counts['expect'] > 0):
                debt_score += active * 5
                issues.append(f"Mixed error handling: unwrap({error_counts['unwrap']}), expect({error_counts['expect']}), ?({error_counts['question']}), match({error_counts['match']})")

            # Magic numbers
            magic_numbers = len(re.findall(r'(?<![a-zA-Z0-9_])(?:[2-9]|\d{2,})(?![a-zA-Z0-9_])', content))
            if magic_numbers > 5:
                debt_score += (magic_numbers - 5)
                issues.append(f"{magic_numbers} potential magic numbers")

            # String literals (heuristics: >5 letters, not in a print/format, occurs multiple times)
            strings = re.findall(r'"([^"\\]{5,})"', content)
            string_counts = defaultdict(int)
            for s in strings:
                string_counts[s] += 1
            magic_strings = sum(1 for v in string_counts.values() if v > 2)
            if magic_strings > 0:
                debt_score += magic_strings * 2
                issues.append(f"{magic_strings} duplicated string literals (potential constants)")

            # Unreachable branches (simplistic heuristic: multiple loop/match breaks or nested unreachables)
            unreachable_count = len(re.findall(r'unreachable!\(', content))
            if unreachable_count > 0:
                debt_score += unreachable_count * 5
                issues.append(f"{unreachable_count} unreachable! macros")

            # Collect structs/enums
            for name, _ in extract_structs_enums(content, filepath):
                all_structs[name].append(filepath)

        if debt_score > 0:
            file_debt[filepath] = {
                "score": debt_score,
                "issues": issues
            }
            total_debt += debt_score

    # Process duplicated structs
    for name, paths in all_structs.items():
        if len(paths) > 1:
            for p in set(paths):
                if p in file_debt:
                    file_debt[p]['score'] += 10
                    file_debt[p]['issues'].append(f"Duplicated struct/enum: {name}")
                else:
                    file_debt[p] = {"score": 10, "issues": [f"Duplicated struct/enum: {name}"]}
                total_debt += 10

    return total_debt, file_debt

def get_pr_for_commit(repo, commit_sha, token):
    url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
    req = urllib.request.Request(url)
    req.add_header("Authorization", f"Bearer {token}")
    req.add_header("Accept", "application/vnd.github+json")
    req.add_header("X-GitHub-Api-Version", "2022-11-28")

    try:
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode('utf-8'))
            if data and len(data) > 0:
                # Get first open PR, or just first PR
                open_prs = [pr for pr in data if pr.get('state') == 'open']
                if open_prs:
                    return open_prs[0]['number']
                return data[0]['number']
    except Exception as e:
        print(f"Error fetching PR for commit: {e}")
    return None

def post_pr_comment(body):
    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")

    if not repo or not token:
        print("Missing GITHUB_REPOSITORY or GITHUB_TOKEN")
        return

    pr_number = None
    ref = os.environ.get("GITHUB_REF", "")

    # Check if pull_request event
    if "pull" in ref:
        try:
            pr_number = ref.split("/")[2]
        except:
            pass

    if not pr_number:
        # Check event payload
        event_path = os.environ.get("GITHUB_EVENT_PATH")
        if event_path and os.path.exists(event_path):
            try:
                with open(event_path, 'r') as f:
                    event_data = json.load(f)
                    if 'pull_request' in event_data:
                        pr_number = event_data['pull_request']['number']
            except:
                pass

    if not pr_number:
        # Check if push event, get PR for commit
        event_name = os.environ.get("GITHUB_EVENT_NAME", "")
        if event_name == "push":
            commit_sha = os.environ.get("GITHUB_SHA")
            if commit_sha:
                pr_number = get_pr_for_commit(repo, commit_sha, token)

    if not pr_number:
        print("Could not determine PR number. Skipping comment.")
        return

    url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
    req = urllib.request.Request(url, method="POST")
    req.add_header("Authorization", f"Bearer {token}")
    req.add_header("Accept", "application/vnd.github+json")
    req.add_header("Content-Type", "application/json")

    data = json.dumps({"body": body}).encode("utf-8")

    try:
        with urllib.request.urlopen(req, data=data) as f:
            print(f"Successfully posted PR comment to PR #{pr_number}")
    except Exception as e:
        print(f"Failed to post PR comment to PR #{pr_number}: {e}")

def main():
    try:
        subprocess.run(["git", "fetch", "--unshallow"], stderr=subprocess.DEVNULL)
    except:
        subprocess.run(["git", "fetch", "--all"], stderr=subprocess.DEVNULL)

    out = subprocess.check_output(["git", "log", "--oneline", "-n", "6", "--format=%H"], text=True)
    commits = out.strip().split('\n')

    if len(commits) == 0:
        return

    history = []
    original_commit = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()

    try:
        for commit in reversed(commits):
            total, files = calculate_debt(commit)
            history.append({
                "commit": commit,
                "short_commit": commit[:7],
                "total": total,
                "files": files
            })
    finally:
        subprocess.run(["git", "checkout", "-q", original_commit])

    # Generate Scorecard
    current = history[-1]

    trend = "FLAT"
    if len(history) > 1:
        if current['total'] > history[-2]['total']:
            trend = "DEGRADING 📉"
        elif current['total'] < history[-2]['total']:
            trend = "IMPROVING 📈"

    report = ["# 🏗️ Chief Architect Sanitation Scorecard"]
    report.append(f"**Trajectory:** {trend}")
    report.append(f"**Current Debt Score:** {current['total']}\n")

    report.append("### Last 6 Commits (Oldest to Newest)")
    report.append("| Commit | Debt Score |")
    report.append("| :--- | :--- |")
    for h in history:
        mark = " (Current)" if h == current else ""
        report.append(f"| {h['short_commit']} | {h['total']}{mark} |")

    report.append("\n### 🚨 Degrading Files (Score Increased)")
    if len(history) > 1:
        prev = history[-2]
        degraded = False
        for filepath, data in current['files'].items():
            prev_score = prev['files'].get(filepath, {}).get('score', 0)
            if data['score'] > prev_score:
                degraded = True
                report.append(f"\n**`{filepath}`** (Score: {prev_score} -> {data['score']})")
                for issue in data['issues']:
                    if issue not in prev['files'].get(filepath, {}).get('issues', []):
                        report.append(f"- {issue}")
        if not degraded:
            report.append("*No files degraded in this commit.*")
    else:
        report.append("*Insufficient history to determine degradation.*")

    report.append("\n### 🛑 Top 5 High-Debt Files")
    sorted_files = sorted(current['files'].items(), key=lambda x: x[1]['score'], reverse=True)
    for filepath, data in sorted_files[:5]:
        report.append(f"\n**`{filepath}`** (Score: {data['score']})")
        for issue in data['issues'][:3]:
            report.append(f"- {issue}")
        if len(data['issues']) > 3:
            report.append(f"- *...and {len(data['issues']) - 3} more issues*")

    report_text = "\n".join(report)
    print(report_text)

    event_name = os.environ.get("GITHUB_EVENT_NAME", "")
    if event_name == "push":
        post_pr_comment(report_text)

if __name__ == "__main__":
    main()
