import os
import sys
import subprocess
import json
import urllib.request
import urllib.error

# CTO Agent: Analyzes PRs and main for bugs and correctness issues.

def chunk_file(content, max_chars=120000):
    """Yield chunks of a string, approximately bounded by characters to avoid API limits."""
    for i in range(0, len(content), max_chars):
        yield content[i:i + max_chars]

def get_modified_files():
    """Returns a list of files modified in the PR."""
    try:
        # Assuming GitHub Actions environment
        base_ref = os.environ.get('GITHUB_BASE_REF')
        if base_ref:
            subprocess.run(['git', 'fetch', 'origin', base_ref], check=True)
            result = subprocess.run(
                ['git', 'diff', f'origin/{base_ref}...HEAD', '--name-only'],
                capture_output=True, text=True, check=True
            )
            return [f for f in result.stdout.strip().split('\n') if f]
        return []
    except subprocess.CalledProcessError as e:
        print(f"Error getting modified files: {e}")
        return []

def get_all_rust_files(directory):
    """Returns a list of all Rust files in the directory."""
    files = []
    for root, _, filenames in os.walk(directory):
        for filename in filenames:
            if filename.endswith('.rs'):
                files.append(os.path.join(root, filename))
    return files

def query_llm(prompt):
    """Queries the OpenAI API."""
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        print("OPENAI_API_KEY not found. Bypassing LLM logic gracefully.")
        return '{"findings": []}'

    url = 'https://api.openai.com/v1/chat/completions'
    headers = {
        'Content-Type': 'application/json',
        'Authorization': f'Bearer {api_key}'
    }
    data = {
        "model": "gpt-4o-2024-08-06",
        "response_format": {"type": "json_object"},
        "messages": [
            {
                "role": "system",
                "content": "You are a CTO who has been burned by production incidents traced to subtle bugs that code review missed. You are methodical and assume the code is wrong until proven otherwise. Analyze code for logic bugs, edge case vulnerabilities, and correctness issues: Off-by-one errors in any loop, index, or range operation (pay special attention to the AI agent scaffolding and context window management in .agents/), Panic paths (.unwrap()/.expect()) triggered by realistic inputs (especially processing external data or LLM responses), Integer overflow/underflow risks in any arithmetic involving agent scoring, token counting, or eval metrics, Race conditions or incorrect assumptions about execution order in any async or concurrent code paths, Logic branches where the code silently succeeds but produces an incorrect result (false positives worse than panics), Test coverage gaps (identify any public functions in mythrax-core with no corresponding test in the eval or test suite). For each bug found, provide: the file and line number, a minimal reproducible scenario, the severity, and a suggested fix. Format output as a 'Red Team Architecture Brief' (Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change). Return a valid JSON object containing a single key 'findings' which holds a JSON array of these results. Each finding should have keys: 'title', 'file', 'line', 'scenario', 'severity', 'fix', 'brief'."
            },
            {
                "role": "user",
                "content": prompt
            }
        ]
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            return result['choices'][0]['message']['content']
    except urllib.error.URLError as e:
        print(f"LLM API Error: {e}")
        return '{"findings": []}'
    except json.JSONDecodeError as e:
         print(f"JSON Parse Error from LLM: {e}")
         return '{"findings": []}'

def file_github_issue(finding):
    """Files a GitHub issue for a finding."""
    token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')

    if not token or not repo:
        print("GITHUB_TOKEN or GITHUB_REPOSITORY not set. Skipping issue creation.")
        return

    # Check for existing issues to prevent duplicates with pagination
    page = 1
    issue_exists = False
    while True:
        url = f'https://api.github.com/repos/{repo}/issues?state=all&per_page=100&page={page}'
        headers = {
            'Authorization': f'token {token}',
            'Accept': 'application/vnd.github.v3+json'
        }
        req = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(req) as response:
                issues = json.loads(response.read().decode('utf-8'))
                if not issues:
                    break
                for issue in issues:
                    if issue.get('title') == finding['title']:
                        print(f"Issue '{finding['title']}' already exists. Skipping.")
                        issue_exists = True
                        break
                if issue_exists:
                    break
                page += 1
        except urllib.error.URLError as e:
            print(f"Error checking existing issues: {e}")
            break

    if issue_exists:
        return

    # Create new issue
    url = f'https://api.github.com/repos/{repo}/issues'

    body = f"""**Red Team Architecture Brief**

**File:** {finding.get('file', 'Unknown')}
**Line:** {finding.get('line', 'Unknown')}
**Severity:** {finding.get('severity', 'Unknown')}

**Current Assumption / Minimal Reproducible Scenario:**
{finding.get('scenario', 'Unknown')}

**Attack Scenario / Bug Context:**
{finding.get('brief', 'Unknown')}

**Blast Radius:**
{finding.get('severity', 'Unknown')} - Potentially systemic.

**Recommended Structural Change / Fix:**
{finding.get('fix', 'Unknown')}

*Note to Developers: Do not close this issue without a documented architectural decision record (ADR) response.*
"""

    data = {
        'title': finding['title'],
        'body': body,
        'labels': ['bug', 'agent-found', 'architecture-review', 'adversarial']
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Successfully filed issue: {finding['title']}")
    except urllib.error.URLError as e:
        print(f"Error creating issue: {e}")

def main():
    event_name = os.environ.get('GITHUB_EVENT_NAME', 'schedule')

    if event_name == 'pull_request':
        files_to_analyze = get_modified_files()
        # Always include core context files
        for core_file in ['ARCHITECTURE.md', 'mock_audit_report.md']:
            if os.path.exists(core_file) and core_file not in files_to_analyze:
                 files_to_analyze.append(core_file)
    else:
        files_to_analyze = get_all_rust_files('mythrax-core')

        # also include python scripts and agents in .agents/
        for root, _, filenames in os.walk('.agents'):
            for filename in filenames:
                if filename.endswith('.py') or filename.endswith('.json') or filename.endswith('.md') or filename.endswith('.rs'):
                     files_to_analyze.append(os.path.join(root, filename))

        # and eval scripts
        for root, _, filenames in os.walk('evals'):
             for filename in filenames:
                  if filename.endswith('.py') or filename.endswith('.rs'):
                       files_to_analyze.append(os.path.join(root, filename))

        files_to_analyze.extend(['ARCHITECTURE.md', 'mock_audit_report.md'])

    all_findings = []

    for file_path in files_to_analyze:
        if not os.path.exists(file_path):
             continue

        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()

            for chunk in chunk_file(content):
                prompt = f"Analyze the following code from {file_path}:\n\n```\n{chunk}\n```"
                response_str = query_llm(prompt)

                try:
                    response_json = json.loads(response_str)
                    findings = response_json.get('findings', [])
                    all_findings.extend(findings)
                except json.JSONDecodeError:
                    print(f"Failed to parse LLM response for {file_path}")
        except Exception as e:
            print(f"Error processing file {file_path}: {e}")

    for finding in all_findings:
        file_github_issue(finding)

if __name__ == "__main__":
    main()
