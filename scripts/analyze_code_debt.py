import os
import re
import subprocess
from collections import defaultdict
import json
import urllib.request
from urllib.error import URLError

def get_commits():
    try:
        res = subprocess.run(['git', 'rev-parse', '--is-shallow-repository'], capture_output=True, text=True)
        if res.stdout.strip() == 'true':
            # Use shell=True for this specific fallback command
            subprocess.run('git fetch --unshallow || git fetch --all', shell=True, capture_output=True)
        # current commit + 5 previous commits
        res = subprocess.run(['git', 'log', '-n', '6', '--format=%H'], capture_output=True, text=True)
        return res.stdout.strip().split('\n')
    except Exception as e:
        print(f"Error getting commits: {e}")
        return []

def get_tracked_todos():
    try:
        with open('TODO.md', 'r') as f:
            content = f.read()
            words = set(re.findall(r'[a-zA-Z]{4,}', content.lower()))
            return words
    except FileNotFoundError:
        return set()

def count_cyclomatic_complexity(content):
    complexity = 1
    complexity += len(re.findall(r'\bif\b', content))
    complexity += len(re.findall(r'\bwhile\b', content))
    complexity += len(re.findall(r'\bfor\b', content))
    complexity += len(re.findall(r'\bmatch\b', content))
    complexity += len(re.findall(r'\?', content))
    complexity += len(re.findall(r'&&', content))
    complexity += len(re.findall(r'\|\|', content))
    return complexity

def analyze_file(content, file_path, tracked_words):
    dead_code = []
    orphaned_comments = []
    complex_functions = []
    error_handling = []
    magic_numbers = []

    # Rust files
    if file_path.endswith('.rs'):
        DEAD_CODE_PATTERN = re.compile(r'#\[allow\((dead_code|unused_imports|unreachable_code)\)\]')
        COMMENT_PATTERN = re.compile(r'//.*?(TODO|FIXME|HACK|TEMP)(.*)', re.IGNORECASE)
        FN_PATTERN = re.compile(r'\s*fn\s+(\w+)\s*\(')
        UNWRAP_EXPECT_PATTERN = re.compile(r'\.unwrap\(\)|\.expect\(')
        MAGIC_NUMBER_PATTERN = re.compile(r'\b(?!0\b|1\b)\d+\b')
        # Check for string literals not inside print/log macros or asserts.
        # This is a basic check.
        MAGIC_STRING_PATTERN = re.compile(r'(?<!print!)(?<!println!)(?<!eprint!)(?<!eprintln!)(?<!format!)(?<!log!)(?<!info!)(?<!warn!)(?<!error!)(?<!debug!)(?<!assert!)(?<!assert_eq!)\(\s*"[^"]{5,}"\s*\)')

    elif file_path.endswith('.py') or file_path.endswith('.sh'):
        DEAD_CODE_PATTERN = re.compile(r'^$')
        COMMENT_PATTERN = re.compile(r'#.*?(TODO|FIXME|HACK|TEMP)(.*)', re.IGNORECASE)
        FN_PATTERN = re.compile(r'\s*def\s+(\w+)\s*\(')
        UNWRAP_EXPECT_PATTERN = re.compile(r'^$')
        MAGIC_NUMBER_PATTERN = re.compile(r'^$')
        MAGIC_STRING_PATTERN = re.compile(r'^$')

    lines = content.split('\n')

    in_function = False
    current_fn_name = ""
    current_fn_content = []
    brace_count = 0
    fn_start_line = 0

    # Also updated pattern to allow pub(crate) etc.
    structs_enums = []
    STRUCT_ENUM_PATTERN = re.compile(r'\s*(pub(\([^)]+\))?\s+)?(struct|enum)\s+(\w+)')

    for i, line in enumerate(lines):
        line_num = i + 1

        if file_path.endswith('.rs'):
            struct_match = STRUCT_ENUM_PATTERN.search(line)
            if struct_match:
                structs_enums.append(struct_match.group(4)) # Group 4 is the name now

        if DEAD_CODE_PATTERN.search(line):
            dead_code.append(line_num)

        comment_match = COMMENT_PATTERN.search(line)
        if comment_match:
            comment_text = comment_match.group(2).strip().lower()
            comment_words = set(re.findall(r'[a-zA-Z]{4,}', comment_text))
            if not comment_words.intersection(tracked_words):
                orphaned_comments.append(line_num)

        if UNWRAP_EXPECT_PATTERN.search(line):
            error_handling.append(line_num)

        if file_path.endswith('.rs'):
            fn_match = FN_PATTERN.search(line)
            if fn_match and not in_function:
                in_function = True
                current_fn_name = fn_match.group(1)
                fn_start_line = line_num
                brace_count = 0
                current_fn_content = []

            if in_function:
                # Strip comments for brace counting to avoid false positives
                clean_line = re.sub(r'//.*', '', line)
                clean_line = re.sub(r'".*?"', '""', clean_line) # roughly strip strings

                current_fn_content.append(line)
                brace_count += clean_line.count('{')
                brace_count -= clean_line.count('}')

                if not re.search(r'\bconst\s+[A-Z0-9_]+\s*:', line) and not re.search(r'let\s+[A-Z0-9_]+\s*:', line):
                     if MAGIC_NUMBER_PATTERN.search(clean_line) or MAGIC_STRING_PATTERN.search(line):
                         magic_numbers.append(line_num)

                if brace_count <= 0 and '{' in ''.join(current_fn_content):
                    in_function = False
                    complexity = count_cyclomatic_complexity('\n'.join(current_fn_content))
                    if complexity > 15:
                        complex_functions.append((current_fn_name, complexity, fn_start_line))

    return {
        'dead_code': dead_code,
        'orphaned_comments': orphaned_comments,
        'complex_functions': complex_functions,
        'error_handling': error_handling,
        'magic_numbers': magic_numbers,
        'structs_enums': structs_enums
    }

def run_analysis_for_commit(commit_hash):
    subprocess.run(['git', 'checkout', '-q', commit_hash])

    tracked_words = get_tracked_todos()

    files_to_check = []
    for root in ['mythrax-core', 'scripts']:
        if not os.path.exists(root): continue
        for dirpath, _, files in os.walk(root):
            for f in files:
                if f.endswith('.rs') or f.endswith('.py') or f.endswith('.sh'):
                    files_to_check.append(os.path.join(dirpath, f))

    total_metrics = {
        'dead_code': 0,
        'orphaned_comments': 0,
        'complex_functions': 0,
        'error_handling': 0,
        'magic_numbers': 0,
        'total_issues': 0
    }

    file_findings = {}
    all_structs = []

    for f in files_to_check:
        try:
            with open(f, 'r') as file:
                content = file.read()
                metrics = analyze_file(content, f, tracked_words)

                all_structs.extend(metrics['structs_enums'])

                issues = len(metrics['dead_code']) + len(metrics['orphaned_comments']) + len(metrics['complex_functions']) + len(metrics['error_handling']) + len(metrics['magic_numbers'])

                file_findings[f] = {
                    'issues': issues,
                    'details': metrics
                }

                total_metrics['dead_code'] += len(metrics['dead_code'])
                total_metrics['orphaned_comments'] += len(metrics['orphaned_comments'])
                total_metrics['complex_functions'] += len(metrics['complex_functions'])
                total_metrics['error_handling'] += len(metrics['error_handling'])
                total_metrics['magic_numbers'] += len(metrics['magic_numbers'])
                total_metrics['total_issues'] += issues

        except Exception:
            file_findings[f] = {'issues': 0}

    struct_counts = {}
    for s in all_structs:
        struct_counts[s] = struct_counts.get(s, 0) + 1
    duplicated_structs = sum(1 for c in struct_counts.values() if c > 1)
    total_metrics['total_issues'] += duplicated_structs
    total_metrics['duplicated_structs'] = duplicated_structs

    return total_metrics, file_findings

def post_comment(report_body):
    repo = os.environ.get('GITHUB_REPOSITORY')
    token = os.environ.get('GITHUB_TOKEN')
    event_path = os.environ.get('GITHUB_EVENT_PATH')

    if not repo or not token or not event_path:
        print("Missing required environment variables for posting PR comment.")
        return

    try:
        with open(event_path, 'r') as f:
            event = json.load(f)

        pr_number = None

        if 'pull_request' in event:
            pr_number = event['pull_request']['number']
        elif 'commits' in event:
            commit_sha = event['after']
            url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
            req = urllib.request.Request(url, headers={
                'Authorization': f'token {token}',
                'Accept': 'application/vnd.github.groot-preview+json'
            })
            response = urllib.request.urlopen(req)
            prs = json.loads(response.read())
            if prs:
                pr_number = prs[0]['number']

        if not pr_number:
            print("No associated PR found for this push event.")
            return

        url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
        req = urllib.request.Request(url, method='POST', headers={
            'Authorization': f'token {token}',
            'Accept': 'application/vnd.github.v3+json',
            'Content-Type': 'application/json'
        }, data=json.dumps({'body': report_body}).encode('utf-8'))

        urllib.request.urlopen(req)
        print("Successfully posted comment to PR.")
    except Exception as e:
        print(f"Error posting comment: {e}")

def main():
    commits = get_commits()
    if not commits:
        return

    original_branch = subprocess.run(['git', 'rev-parse', '--abbrev-ref', 'HEAD'], capture_output=True, text=True).stdout.strip()
    if original_branch == 'HEAD':
        original_branch = subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True).stdout.strip()

    results = []
    all_seen_files = set()
    commit_file_findings = []

    for commit in commits[:6]:
        metrics, file_findings = run_analysis_for_commit(commit)
        results.append((commit, metrics))
        commit_file_findings.append(file_findings)
        for f in file_findings:
            all_seen_files.add(f)

    subprocess.run(['git', 'checkout', '-q', original_branch])

    file_histories = defaultdict(list)
    for file_findings in commit_file_findings:
        for f in all_seen_files:
            file_histories[f].append(file_findings.get(f, {}).get('issues', 0))

    current_metrics = results[0][1]
    oldest_metrics = results[-1][1]

    trend = "DEGRADING 📉" if current_metrics['total_issues'] > oldest_metrics['total_issues'] else ("IMPROVING 📈" if current_metrics['total_issues'] < oldest_metrics['total_issues'] else "STABLE ➖")

    report = f"## 🏗️ Chief Architect Sanitation Scorecard\n\n"
    report += f"**Overall Trajectory:** {trend}\n\n"
    report += "| Metric | Current Commit | 5 Commits Ago | Trend |\n"
    report += "|--------|----------------|---------------|-------|\n"
    report += f"| **Total Debt** | {current_metrics['total_issues']} | {oldest_metrics['total_issues']} | {current_metrics['total_issues'] - oldest_metrics['total_issues']:+} |\n"
    report += f"| **Dead Code** | {current_metrics['dead_code']} | {oldest_metrics['dead_code']} | {current_metrics['dead_code'] - oldest_metrics['dead_code']:+} |\n"
    report += f"| **Orphaned TODOs** | {current_metrics['orphaned_comments']} | {oldest_metrics['orphaned_comments']} | {current_metrics['orphaned_comments'] - oldest_metrics['orphaned_comments']:+} |\n"
    report += f"| **Complex Functions** (>15) | {current_metrics['complex_functions']} | {oldest_metrics['complex_functions']} | {current_metrics['complex_functions'] - oldest_metrics['complex_functions']:+} |\n"
    report += f"| **Inconsistent Errors** (`unwrap/expect`) | {current_metrics['error_handling']} | {oldest_metrics['error_handling']} | {current_metrics['error_handling'] - oldest_metrics['error_handling']:+} |\n"
    report += f"| **Duplicated Structs** | {current_metrics['duplicated_structs']} | {oldest_metrics['duplicated_structs']} | {current_metrics['duplicated_structs'] - oldest_metrics['duplicated_structs']:+} |\n"
    report += f"| **Magic Numbers/Strings** | {current_metrics['magic_numbers']} | {oldest_metrics['magic_numbers']} | {current_metrics['magic_numbers'] - oldest_metrics['magic_numbers']:+} |\n\n"

    increasing_files = []
    for f, history in file_histories.items():
        if len(history) >= 2 and history[0] > history[-1]:
            increasing_files.append((f, history[0] - history[-1]))

    if increasing_files:
        report += "### ⚠️ Files with Increasing Debt\n"
        for f, diff in sorted(increasing_files, key=lambda x: x[1], reverse=True):
            report += f"- `{f}` (+{diff} issues)\n"

    print(report)

    if os.environ.get('GITHUB_ACTIONS') == 'true':
        post_comment(report)

if __name__ == '__main__':
    main()
