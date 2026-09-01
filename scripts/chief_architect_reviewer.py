import os
import re
import subprocess
import json
import urllib.request
import urllib.error
import logging
from collections import defaultdict

logging.basicConfig(level=logging.INFO, format='%(levelname)s: %(message)s')

def get_git_history():
    try:
        subprocess.run(['git', 'fetch', '--unshallow'], capture_output=True)
    except Exception:
        subprocess.run(['git', 'fetch', '--all'], capture_output=True)

    res = subprocess.run(['git', 'log', '-n', '6', '--format=%H'], capture_output=True, text=True)
    commits = res.stdout.strip().split('\n')
    return commits

def parse_agent_system_prompt():
    prompt = ""
    try:
        with open('.agents/agents/chief_architect/agent.json', 'r') as f:
            config = json.load(f)
            sections = config.get('config', {}).get('customAgent', {}).get('systemPromptSections', [])
            for s in sections:
                if 'content' in s:
                    prompt += s['content'] + "\n"
    except Exception as e:
        logging.warning(f"Could not parse agent system prompt: {e}")
    return prompt

def scan_file_for_debt(filepath, todo_items):
    debt = {
        'dead_code': 0,
        'orphaned_todos': 0,
        'high_complexity': 0,
        'inconsistent_errors': 0,
        'magic_numbers': 0,
        'duplicated_structs': 0
    }

    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
        lines = content.split('\n')

    debt['dead_code'] = len(re.findall(r'#\[allow\(dead_code\)\]', content))

    matches = re.finditer(r'(?:#|//)\s*(TODO|FIXME|HACK|TEMP).*', content, re.IGNORECASE)
    for match in matches:
        comment_text = match.group(0).lower()

        is_tracked = False
        for todo in todo_items:
            if todo in comment_text or comment_text in todo:
                is_tracked = True
                break

        if not is_tracked:
            debt['orphaned_todos'] += 1

    current_fn = None
    fn_complexity = 0
    in_fn = False
    brace_depth = 0

    for line in lines:
        if not in_fn:
            if re.search(r'\b(?:pub\s+)?(?:async\s+)?fn\s+\w+', line):
                in_fn = True
                fn_complexity = 1
                brace_depth = line.count('{') - line.count('}')
        else:
            brace_depth += line.count('{') - line.count('}')

            if re.search(r'\b(if|match|for|while|loop)\b', line):
                fn_complexity += 1
            if re.search(r'=>', line):
                 fn_complexity += 1

            if brace_depth <= 0:
                in_fn = False
                if fn_complexity > 15:
                    debt['high_complexity'] += 1

    if in_fn and fn_complexity > 15:
        debt['high_complexity'] += 1

    has_unwrap = '.unwrap()' in content
    has_expect = '.expect(' in content
    has_question = '?' in content

    if (has_unwrap or has_expect) and has_question:
         debt['inconsistent_errors'] += 1

    magic_number_matches = re.findall(r'let\s+\w+\s*(?::\s*\w+)?\s*=\s*([2-9]|\d{2,})\s*;', content)
    magic_string_matches = re.findall(r'let\s+\w+\s*(?::\s*\w+)?\s*=\s*"(?:[^"]{3,})"\s*;', content)

    debt['magic_numbers'] = len(magic_number_matches) + len(magic_string_matches)

    return debt

def calculate_score(debt):
    return sum(debt.values())

def analyze_commit(commit_hash):
    subprocess.run(['git', 'checkout', commit_hash], capture_output=True)

    todo_items = []
    if os.path.exists('TODO.md'):
        with open('TODO.md', 'r', encoding='utf-8') as f:
            for line in f:
                if re.search(r'TODO|FIXME|HACK|TEMP', line, re.IGNORECASE):
                    todo_items.append(line.strip().lower())

    total_debt = {
        'dead_code': 0,
        'orphaned_todos': 0,
        'high_complexity': 0,
        'inconsistent_errors': 0,
        'magic_numbers': 0,
        'duplicated_structs': 0
    }

    file_debts = {}

    all_structs = defaultdict(list)

    dirs_to_scan = ['mythrax-core', 'scripts']
    for d in dirs_to_scan:
        if not os.path.exists(d): continue
        for root, _, files in os.walk(d):
            for file in files:
                if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                    filepath = os.path.join(root, file)
                    debt = scan_file_for_debt(filepath, todo_items)
                    file_debts[filepath] = debt

                    for k, v in debt.items():
                        total_debt[k] += v

                    if file.endswith('.rs'):
                        with open(filepath, 'r', encoding='utf-8') as f:
                            content = f.read()
                            structs = re.findall(r'struct\s+(\w+)\s*\{', content)
                            for s in structs:
                                all_structs[s].append(filepath)

    duplicated = sum(1 for s, paths in all_structs.items() if len(paths) > 1 and len(set(paths)) > 1)
    total_debt['duplicated_structs'] = duplicated

    return calculate_score(total_debt), file_debts, total_debt

def format_scorecard(results):
    report = "# Architecture Sanitation Scorecard\n\n"
    report += "## Trajectory (Debt Density)\n\n"

    report += "| Commit | Debt Score | Dead Code | Orphaned TODOs | High Complexity | Inconsistent Errors | Magic Numbers | Duplicated Structs |\n"
    report += "|--------|------------|-----------|----------------|-----------------|---------------------|---------------|--------------------|\n"

    for r in results:
        d = r['detailed_debt']
        report += f"| {r['commit'][:7]} | {r['score']} | {d['dead_code']} | {d['orphaned_todos']} | {d['high_complexity']} | {d['inconsistent_errors']} | {d['magic_numbers']} | {d['duplicated_structs']} |\n"

    report += "\n## Flagged Files (Increasing Debt or High Debt)\n\n"

    current = results[0]

    if len(results) >= 2:
        prev = results[1]

        for filepath, current_debt_dict in current['file_debts'].items():
            curr_score = calculate_score(current_debt_dict)
            prev_score = calculate_score(prev['file_debts'].get(filepath, {}))

            if curr_score > prev_score:
                report += f"- **{filepath}**: Debt increased from {prev_score} to {curr_score}\n"
            elif curr_score > 0 and curr_score >= prev_score:
                report += f"- **{filepath}**: High debt ({curr_score})\n"

    else:
        report += "Not enough history to compare trajectory.\n"

    return report

def main():
    commits = get_git_history()
    if not commits:
        logging.error("No commits found.")
        return

    print(f"Found commits: {commits}")

    prompt = parse_agent_system_prompt()
    if not prompt:
        logging.warning("No system prompt loaded.")
    else:
        logging.info("Agent configuration loaded successfully.")

    original_branch = subprocess.run(['git', 'branch', '--show-current'], capture_output=True, text=True).stdout.strip()
    if not original_branch:
        original_branch = subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True).stdout.strip()

    results = []
    for commit in commits:
        score, file_debts, detailed_debt = analyze_commit(commit)
        results.append({
            'commit': commit,
            'score': score,
            'file_debts': file_debts,
            'detailed_debt': detailed_debt
        })

    subprocess.run(['git', 'checkout', original_branch], capture_output=True)

    report = format_scorecard(results)

    print("\n--- SANITATION SCORECARD ---\n")
    print(report)
    print("\n--------------------------\n")

    event_path = os.environ.get('GITHUB_EVENT_PATH')
    event_name = os.environ.get('GITHUB_EVENT_NAME')
    if event_name == 'push' and event_path and os.path.exists(event_path):
        repo = os.environ.get('GITHUB_REPOSITORY')
        token = os.environ.get('GH_TOKEN') or os.environ.get('GITHUB_TOKEN')
        commit_sha = os.environ.get('GITHUB_SHA', commits[0])

        if repo and token:
            url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
            headers = {
                "Authorization": f"Bearer {token}",
                "Accept": "application/vnd.github.groot-preview+json"
            }
            req = urllib.request.Request(url, headers=headers)
            try:
                with urllib.request.urlopen(req) as response:
                    prs = json.loads(response.read().decode('utf-8'))
                    if prs:
                        pr_number = prs[0]['number']
                        comment_url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
                        data = {"body": report}
                        comment_req = urllib.request.Request(comment_url, headers=headers, data=json.dumps(data).encode('utf-8'))
                        with urllib.request.urlopen(comment_req) as comment_response:
                            logging.info(f"Successfully posted comment to PR #{pr_number}")
                    else:
                        logging.info("No PR found for this commit.")
            except urllib.error.HTTPError as e:
                logging.error(f"Failed to query/post PR: HTTP {e.code} - {e.reason}")
            except Exception as e:
                logging.error(f"Failed to query/post PR: {e}")
        else:
             logging.info("Missing credentials, skipping PR comment.")

if __name__ == "__main__":
    main()
