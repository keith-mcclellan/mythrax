#!/usr/bin/env python3

import os
import re
import sys
import subprocess
import json
import urllib.request
import urllib.error

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout.strip()

def get_commits():
    subprocess.run("git fetch --unshallow || git fetch --all", shell=True, stderr=subprocess.DEVNULL)
    out = run_cmd("git rev-list -n 6 HEAD")
    commits = out.split('\n') if out else []
    return commits

def parse_todo_md():
    if not os.path.exists("TODO.md"):
        return ""
    with open("TODO.md", "r", encoding="utf-8") as f:
        content = f.read().lower()
    return content

def scan_file(filepath, todo_md_content):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    score = 0
    issues = []

    allows = re.findall(r'#\[allow\((dead_code|unused_imports|unreachable_code)\)\]', content)
    score += len(allows)
    if allows:
        issues.append(f"Found {len(allows)} debt suppression annotations (dead_code/unused_imports)")

    comments = re.findall(r'(?:#|//)\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', content, flags=re.IGNORECASE)
    orphaned_count = 0
    for tag, text in comments:
        if text.strip() and text.strip().lower() not in todo_md_content:
            orphaned_count += 1
            issues.append(f"Orphaned {tag}: {text.strip()}")
    score += orphaned_count

    fn_count = len(re.findall(r'(?:pub\s+)?fn\s+\w+', content))
    if fn_count > 0:
        complexity_keywords = len(re.findall(r'\b(if|match|for|while|loop)\b|\?|&&|\|\|', content))
        avg_complexity = complexity_keywords / fn_count
        if avg_complexity > 15:
            issues.append(f"High average cyclomatic complexity (~{avg_complexity:.1f})")
            score += int(avg_complexity - 15)

    unwraps = len(re.findall(r'\.unwrap\(\)', content))
    expects = len(re.findall(r'\.expect\(', content))
    questions = len(re.findall(r'\?', content))
    matches = len(re.findall(r'\bmatch\b', content))
    if (unwraps > 0 or expects > 0) and questions > 0:
        issues.append(f"Inconsistent error handling: mixing unwrap/expect with ? operator")
        score += unwraps + expects

    structs = re.findall(r'(?:pub\s+)?(?:struct|enum)\s+(\w+)', content)

    magic_strings = len(re.findall(r'\(\s*"[^"]+"\s*\)', content))
    if magic_strings > 0:
        issues.append(f"Found {magic_strings} potential magic strings in function calls")
        score += magic_strings // 5

    return score, issues, structs

def scan_all():
    todo_md_content = parse_todo_md()
    results = {}
    total_score = 0
    all_structs = {}

    for root, dirs, files in os.walk('.'):
        if '.git' in root or 'target' in root:
            continue
        for file in files:
            if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                if not ('mythrax-core' in root or 'scripts' in root):
                    continue
                filepath = os.path.join(root, file)
                try:
                    score, issues, structs = scan_file(filepath, todo_md_content)
                    if score > 0 or issues:
                        results[filepath] = {
                            'score': score,
                            'issues': issues
                        }
                        total_score += score
                    for s in structs:
                        if s not in all_structs:
                            all_structs[s] = []
                        all_structs[s].append(filepath)
                except Exception:
                    pass

    dups = {s: paths for s, paths in all_structs.items() if len(paths) > 1}
    if dups:
        dup_issues = []
        for s, paths in dups.items():
            dup_issues.append(f"Duplicated struct/enum `{s}` found in {len(paths)} locations")
        results['Global'] = {
            'score': len(dups),
            'issues': dup_issues
        }
        total_score += len(dups)

    return results, total_score

def analyze_history():
    commits = get_commits()
    if not commits:
        return {}, [], {}

    history_scores = []
    file_histories = {}

    original_branch = run_cmd("git rev-parse --abbrev-ref HEAD")
    if original_branch == "HEAD":
        original_branch = commits[0]

    for commit in reversed(commits):
        subprocess.run(f"git checkout {commit} --force", shell=True, stderr=subprocess.DEVNULL, stdout=subprocess.DEVNULL)
        results, score = scan_all()
        history_scores.append((commit, score))
        for filepath, data in results.items():
            if filepath not in file_histories:
                file_histories[filepath] = []
            file_histories[filepath].append(data['score'])

    subprocess.run(f"git checkout {original_branch} --force", shell=True, stderr=subprocess.DEVNULL, stdout=subprocess.DEVNULL)

    current_results, current_score = scan_all()
    return current_results, history_scores, file_histories

def generate_scorecard(current_results, history_scores, file_histories):
    md = "# Sanitation Scorecard\n\n"

    if history_scores:
        md += "## Trajectory (Last 5 commits)\n"
        for commit, score in history_scores:
            md += f"- Commit `{commit[:7]}`: {score} debt points\n"

        if len(history_scores) > 1:
            if history_scores[-1][1] > history_scores[-2][1]:
                md += "\n**⚠️ WARNING: Overall debt density is increasing!**\n"
            else:
                md += "\n**✅ Overall debt density is stable or improving.**\n"

    md += "\n## Files with Increasing Debt\n"
    increasing_files = []
    for filepath, scores in file_histories.items():
        if len(scores) > 1 and scores[-1] > scores[-2]:
            increasing_files.append(filepath)

    if increasing_files:
        for f in increasing_files:
            md += f"- `{f}`\n"
    else:
        md += "- None\n"

    md += "\n## Current Findings\n"
    for filepath, data in current_results.items():
        md += f"\n### `{filepath}` (Debt Score: {data['score']})\n"
        for issue in data['issues']:
            md += f"- {issue}\n"

    return md

def get_pr_number(repo, commit_sha, gh_token):
    url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
    headers = {
        "Authorization": f"token {gh_token}",
        "Accept": "application/vnd.github.groot-preview+json"
    }
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            pulls = json.loads(response.read().decode())
            if pulls:
                return pulls[0]["number"]
    except Exception as e:
        print(f"Error fetching PRs for commit: {e}")
    return None

def post_pr_comment(md):
    gh_token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    event_name = os.environ.get("GITHUB_EVENT_NAME")

    if not (gh_token and repo):
        print("Missing GH_TOKEN or GITHUB_REPOSITORY.")
        return

    pr_number = None
    if event_name == "pull_request":
        gh_event_path = os.environ.get("GITHUB_EVENT_PATH")
        if gh_event_path and os.path.exists(gh_event_path):
            with open(gh_event_path, "r") as f:
                event_data = json.load(f)
                pr_number = event_data.get("pull_request", {}).get("number")
    elif event_name == "push":
        sha = os.environ.get("GITHUB_SHA")
        if sha:
            pr_number = get_pr_number(repo, sha, gh_token)

    if not pr_number:
        print("No PR found to comment on.")
        return

    url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
    headers = {
        "Authorization": f"token {gh_token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }
    data = json.dumps({"body": md}).encode("utf-8")

    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        urllib.request.urlopen(req)
        print("Successfully posted PR comment.")
    except urllib.error.URLError as e:
        print(f"Failed to post PR comment: {e}")

if __name__ == "__main__":
    current_results, history_scores, file_histories = analyze_history()

    scorecard = generate_scorecard(current_results, history_scores, file_histories)
    print(scorecard)

    if os.environ.get("GITHUB_ACTIONS"):
        post_pr_comment(scorecard)
