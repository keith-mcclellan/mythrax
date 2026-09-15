import os
import json
import urllib.request
import urllib.error
import subprocess

def get_modified_files():
    try:
        output = subprocess.check_output(['git', 'diff', '--name-only', 'HEAD^1', 'HEAD']).decode('utf-8')
        return [line.strip() for line in output.split('\n') if line.strip()]
    except Exception as e:
        print(f"Error getting modified files: {e}")
        return []

def get_all_files():
    try:
        output = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', 'HEAD']).decode('utf-8')
        return [line.strip() for line in output.split('\n') if line.strip()]
    except Exception as e:
        print(f"Error getting all files: {e}")
        return []

def get_existing_issues():
    github_repository = os.environ.get('GITHUB_REPOSITORY')
    github_token = os.environ.get('GITHUB_TOKEN')

    if not github_repository or not github_token:
        print("Missing GITHUB_REPOSITORY or GITHUB_TOKEN environment variables. Cannot fetch issues.")
        return []

    url = f"https://api.github.com/repos/{github_repository}/issues?state=all&per_page=100&creator=app%2Fgithub-actions"
    req = urllib.request.Request(url)
    req.add_header('Authorization', f'token {github_token}')
    req.add_header('Accept', 'application/vnd.github.v3+json')
    req.add_header('User-Agent', 'GitHubActions')

    existing_issues = []
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode())
            existing_issues = [issue.get('title') for issue in issues]
    except Exception as e:
        print(f"Error fetching existing issues: {e}")

    return existing_issues

def create_github_issue(finding):
    github_repository = os.environ.get('GITHUB_REPOSITORY')
    github_token = os.environ.get('GITHUB_TOKEN')

    if not github_repository or not github_token:
        print("Missing GITHUB_REPOSITORY or GITHUB_TOKEN environment variables. Cannot create issue.")
        return

    title = f"Bug: {finding.get('title')}"
    body = f"""**File:** `{finding.get('file')}` (Line {finding.get('line')})
**Severity:** {finding.get('severity')}

**Scenario / Explanation:**
{finding.get('scenario')}

**Suggested Fix:**
{finding.get('suggested_fix')}

> ⚠️ **Adversarial CTO Note:** Do not close this issue without a documented architectural decision record (ADR).
"""
    labels = ["bug", "agent-found"]

    url = f"https://api.github.com/repos/{github_repository}/issues"
    data = json.dumps({"title": title, "body": body, "labels": labels}).encode('utf-8')
    req = urllib.request.Request(url, data=data)
    req.add_header('Authorization', f'token {github_token}')
    req.add_header('Accept', 'application/vnd.github.v3+json')
    req.add_header('Content-Type', 'application/json')
    req.add_header('User-Agent', 'GitHubActions')

    try:
        with urllib.request.urlopen(req) as response:
            res_data = json.loads(response.read().decode())
            print(f"Created issue: {res_data.get('html_url')}")
    except urllib.error.HTTPError as e:
        print(f"Failed to create issue. HTTP Error: {e.code} - {e.read().decode()}")
    except Exception as e:
        print(f"Failed to create issue: {e}")

def main():
    github_event_name = os.environ.get('GITHUB_EVENT_NAME', 'push')
    is_pr = github_event_name in ['pull_request', 'push']
    is_schedule = github_event_name == 'schedule'

    if is_schedule:
        files = get_all_files()
    else:
        files = get_modified_files()

    target_dirs = ('mythrax-core/', 'evals/', '.agents/')
    files_to_analyze = [f for f in files if f.startswith(target_dirs)]

    # Always include core context files
    for core_file in ['ARCHITECTURE.md', 'mock_audit_report.md']:
        if core_file not in files_to_analyze and os.path.exists(core_file):
            files_to_analyze.append(core_file)

    if not files_to_analyze:
        print("No relevant files to analyze.")
        return

    print(f"Analyzing {len(files_to_analyze)} files...")

    file_contents = {}
    for f in files_to_analyze:
        if os.path.exists(f):
            with open(f, 'r', encoding='utf-8') as file:
                file_contents[f] = file.read()

    if not file_contents:
        print("No contents to analyze.")
        return

    prompt = """You are an Adversarial CTO who has been burned by production incidents traced to subtle bugs that code review missed. You are methodical and assume the code is wrong until proven otherwise.

Analyze the provided codebase files for logic bugs, edge case vulnerabilities, and correctness issues:
1. Off-by-one errors in any loop, index, or range operation (pay special attention to AI agent scaffolding and context window management in .agents/).
2. Panic paths: any .unwrap() or .expect() that could be triggered by realistic runtime inputs.
3. Integer overflow or underflow risks in arithmetic involving agent scoring, token counting, or eval metrics.
4. Race conditions or incorrect assumptions about execution order in any async or concurrent paths.
5. Logic branches where code silently succeeds but produces an incorrect result (false positives worse than panics).
6. Test coverage gaps: identify any public functions in mythrax-core with no corresponding test in the eval or test suite.
7. Architectural decisions: challenge assumptions, identify single points of failure without graceful degradation, assess agent orchestration for vulnerabilities (prompt injection, unbounded recursion, failure to enforce boundaries), verify adversarial inputs in evals, flag module coupling, and project 10x scaling limitations 18 months forward.

Output your findings as a JSON object containing a "findings" array. Each object in the array MUST have the following keys:
- "title": A short, descriptive title for the issue.
- "file": The file path where the issue was found.
- "line": The approximate line number (or null if general).
- "scenario": A minimal reproducible scenario or explanation of how this is triggered.
- "severity": "High", "Medium", or "Low".
- "suggested_fix": A description of how to fix the issue.

Return ONLY valid JSON in the format: {"findings": [...] }
"""

    code_context = ""
    for f, content in file_contents.items():
        code_context += f"\n\n--- File: {f} ---\n\n{content}"

    messages = [
        {"role": "system", "content": prompt},
        {"role": "user", "content": f"Here is the code to analyze:\n{code_context[:80000]}"}
    ]

    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        print("No OPENAI_API_KEY found, skipping analysis.")
        return

    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=json.dumps({
            "model": "gpt-4o",
            "messages": messages,
            "response_format": {"type": "json_object"}
        }).encode('utf-8'),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        }
    )

    try:
        response = urllib.request.urlopen(req)
        result = json.loads(response.read().decode('utf-8'))
        content = result['choices'][0]['message']['content']
        parsed_result = json.loads(content)
        findings = parsed_result.get('findings', [])

        print(f"Found {len(findings)} issues.")

        existing_issues = get_existing_issues()

        for finding in findings:
            title = f"Bug: {finding.get('title')}"
            if title in existing_issues:
                print(f"Skipping duplicate issue: {title}")
                continue

            create_github_issue(finding)

    except Exception as e:
        print(f"Error calling OpenAI API or parsing result: {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    main()
