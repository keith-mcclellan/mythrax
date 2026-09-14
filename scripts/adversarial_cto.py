import os
import json
import urllib.request
import urllib.error
import subprocess

def get_modified_files():
    try:
        result = subprocess.run(['git', 'diff', '--name-only', 'HEAD^1', 'HEAD'], capture_output=True, text=True, check=True)
        return [f.strip() for f in result.stdout.split('\n') if f.strip()]
    except Exception as e:
        print(f"Error getting modified files: {e}")
        return []

def get_all_files():
    files = []
    for d in ['mythrax-core', '.agents', 'evals']:
        if os.path.exists(d):
            for root, _, filenames in os.walk(d):
                for f in filenames:
                    if f.endswith(('.rs', '.py', '.md', '.json', '.toml', '.sh')):
                        files.append(os.path.join(root, f))
    return files

def get_files_to_analyze(event_name):
    if event_name == 'pull_request':
        files = get_modified_files()
        valid_prefixes = ('mythrax-core/', '.agents/', 'evals/')
        files = [f for f in files if f.startswith(valid_prefixes) and os.path.isfile(f)]
    else:
        files = get_all_files()

    for core_file in ['ARCHITECTURE.md', 'mock_audit_report.md']:
        if os.path.isfile(core_file) and core_file not in files:
            files.append(core_file)

    return files

def analyze_code_with_llm(files_content):
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        print("OPENAI_API_KEY missing")
        return []

    system_prompt = """You are a CTO who has been burned by production incidents traced to subtle bugs that code review missed. You are methodical and assume the code is wrong until proven otherwise.
Analyze the provided code for logic bugs, edge case vulnerabilities, and correctness issues:
• Off-by-one errors in any loop, index, or range operation — pay special attention to the AI agent scaffolding and context window management in .agents/
• Panic paths: any .unwrap() or .expect() that could be triggered by realistic runtime inputs, especially those processing external data or LLM responses
• Integer overflow or underflow risks in any arithmetic involving agent scoring, token counting, or eval metrics
• Race conditions or incorrect assumptions about execution order in any async or concurrent code paths
• Logic branches where the code silently succeeds but produces an incorrect result (false positives worse than panics)
• Test coverage gaps — identify any public functions in mythrax-core with no corresponding test in the eval or test suite

For each bug found with a realistic trigger path, provide:
- title: A short title for the issue
- file: The file path
- line_number: The line number(s)
- minimal_reproducible_scenario: How to reproduce
- severity: The severity (e.g. Critical, High, Medium, Low)
- suggested_fix: How to fix the bug
- has_realistic_trigger_path: true if the finding has a realistic trigger path, false otherwise. Only true ones will be filed.

Output the result as a JSON object containing a 'findings' list of objects with these exact keys."""

    prompt = "Here are the files to analyze:\n\n"
    for f, content in files_content.items():
        prompt += f"=== File: {f} ===\n{content}\n\n"

    req_data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": prompt}
        ],
        "response_format": {"type": "json_object"}
    }

    data = json.dumps(req_data).encode('utf-8')
    req = urllib.request.Request("https://api.openai.com/v1/chat/completions", data=data)
    req.add_header("Content-Type", "application/json")
    req.add_header("Authorization", f"Bearer {api_key}")

    try:
        with urllib.request.urlopen(req) as response:
            res_data = json.loads(response.read().decode('utf-8'))
            content = res_data['choices'][0]['message']['content']
            parsed = json.loads(content)
            return parsed.get('findings', [])
    except Exception as e:
        print(f"Error calling LLM: {e}")
        return []

def get_existing_issues(repo, token):
    url = f"https://api.github.com/repos/{repo}/issues?state=open"
    req = urllib.request.Request(url)
    req.add_header("Authorization", f"Bearer {token}")
    req.add_header("Accept", "application/vnd.github.v3+json")
    req.add_header("User-Agent", "Adversarial-CTO-Script")

    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode('utf-8'))
            return [issue['title'] for issue in issues if 'pull_request' not in issue]
    except Exception as e:
        print(f"Error fetching issues: {e}")
        return []

def create_issue(repo, token, finding):
    url = f"https://api.github.com/repos/{repo}/issues"

    body = f"""**File:** `{finding.get('file', 'N/A')}:{finding.get('line_number', 'N/A')}`
**Severity:** {finding.get('severity', 'N/A')}

**Minimal Reproducible Scenario:**
{finding.get('minimal_reproducible_scenario', 'N/A')}

**Suggested Fix:**
{finding.get('suggested_fix', 'N/A')}

> **Note:** Do not close this issue without a documented architectural decision record (ADR)."""

    data = {
        "title": finding.get('title', 'Bug found by Adversarial CTO'),
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'))
    req.add_header("Authorization", f"Bearer {token}")
    req.add_header("Accept", "application/vnd.github.v3+json")
    req.add_header("Content-Type", "application/json")
    req.add_header("User-Agent", "Adversarial-CTO-Script")

    try:
        with urllib.request.urlopen(req) as response:
            print(f"Successfully created issue: {data['title']}")
    except urllib.error.HTTPError as e:
        print(f"Failed to create issue: {e.read().decode('utf-8')}")
    except Exception as e:
        print(f"Error creating issue: {e}")

def main():
    event_name = os.environ.get('GITHUB_EVENT_NAME', 'schedule')
    repo = os.environ.get('GITHUB_REPOSITORY')
    token = os.environ.get('GITHUB_TOKEN')

    if not repo or not token:
        print("Missing GITHUB_REPOSITORY or GITHUB_TOKEN")
        return

    files_to_analyze = get_files_to_analyze(event_name)
    if not files_to_analyze:
        print("No files to analyze.")
        return

    print(f"Analyzing {len(files_to_analyze)} files...")

    files_content = {}
    for f in files_to_analyze:
        try:
            with open(f, 'r', encoding='utf-8') as file:
                files_content[f] = file.read()
        except Exception as e:
            print(f"Could not read {f}: {e}")

    findings = analyze_code_with_llm(files_content)

    if not findings:
        print("No findings returned by LLM.")
        return

    existing_issue_titles = get_existing_issues(repo, token)

    for finding in findings:
        if not finding.get('has_realistic_trigger_path', False):
            continue

        title = finding.get('title')
        if title in existing_issue_titles:
            print(f"Issue already exists: {title}")
            continue

        create_issue(repo, token, finding)

if __name__ == "__main__":
    main()
