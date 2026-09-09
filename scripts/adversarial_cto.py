#!/usr/bin/env python3
import os
import sys
import json
import urllib.request
import urllib.error
import subprocess

def get_modified_files():
    base_ref = os.environ.get('GITHUB_BASE_REF')
    if not base_ref:
        base_ref = 'main'
    try:
        subprocess.run(['git', 'fetch', 'origin', base_ref], check=True)
        result = subprocess.run(['git', 'diff', '--name-only', f'origin/{base_ref}'], capture_output=True, text=True, check=True)
        files = result.stdout.strip().split('\n')
        return [f for f in files if f and os.path.exists(f)]
    except Exception as e:
        print(f"Error getting modified files: {e}")
        return []

def get_all_files():
    try:
        result = subprocess.run(['git', 'ls-tree', '-r', '--name-only', 'HEAD'], capture_output=True, text=True, check=True)
        files = result.stdout.strip().split('\n')
        relevant_dirs = ['mythrax-core/', '.agents/']
        return [f for f in files if any(f.startswith(d) for d in relevant_dirs) and os.path.exists(f)]
    except Exception as e:
        print(f"Error getting all files: {e}")
        return []

def analyze_chunk(files_chunk, core_contents, api_key):
    code_content = core_contents
    for f in files_chunk:
        if not os.path.isfile(f): continue
        with open(f, 'r', encoding='utf-8', errors='ignore') as file:
            code_content += f"\n--- {f} ---\n"
            code_content += file.read()[:20000]

    prompt = """
You are a CTO who has been burned by production incidents traced to subtle bugs that code review missed.
Analyze the provided code for logic bugs, edge case vulnerabilities, and correctness issues:
- Off-by-one errors (esp. in agent context .agents/)
- Panic paths (.unwrap() or .expect() triggered by runtime inputs/external data)
- Integer overflow or underflow risks in agent scoring, token counting, or eval metrics
- Race conditions or incorrect assumptions about execution order
- Logic branches where code silently succeeds but produces an incorrect result
- Test coverage gaps (public functions in mythrax-core with no corresponding test in the eval or test suite)

For each finding, output a JSON object in a list with these exact keys:
- title
- file
- line (integer or string)
- finding
- current_assumption
- attack_scenario
- blast_radius
- recommended_structural_change
- minimal_reproducible_scenario
- severity
- suggested_fix

Output a JSON object with a single key 'findings' containing the list of issues. Ensure the output is strictly valid JSON.
"""

    req_data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": code_content}
        ],
        "response_format": { "type": "json_object" }
    }

    req_bytes = json.dumps(req_data).encode('utf-8')
    req = urllib.request.Request("https://api.openai.com/v1/chat/completions", data=req_bytes, headers={
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    })

    try:
        with urllib.request.urlopen(req) as response:
            res_body = response.read().decode('utf-8')
            res_json = json.loads(res_body)
            content = res_json['choices'][0]['message']['content']
            parsed = json.loads(content)
            return parsed.get('findings', [])
    except Exception as e:
        print(f"Error calling LLM: {e}")
        return []

def analyze_with_llm(files, api_key):
    core_contents = ""
    for core_file in ['ARCHITECTURE.md', 'mock_audit_report.md']:
        if os.path.exists(core_file):
            with open(core_file, 'r', encoding='utf-8', errors='ignore') as file:
                core_contents += f"\n--- {core_file} ---\n"
                core_contents += file.read()

    files = [f for f in files if f not in ['ARCHITECTURE.md', 'mock_audit_report.md']]

    all_findings = []
    chunk_size = 10
    for i in range(0, len(files), chunk_size):
        chunk = files[i:i+chunk_size]
        print(f"Analyzing chunk {i//chunk_size + 1} with {len(chunk)} files...")
        findings = analyze_chunk(chunk, core_contents, api_key)
        all_findings.extend(findings)

    return all_findings

def get_existing_issues(repo, token):
    url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100"
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json"
    })
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode('utf-8'))
            return [issue['title'] for issue in issues]
    except Exception as e:
        print(f"Error fetching existing issues: {e}")
        return []

def create_issue(repo, token, issue_data):
    url = f"https://api.github.com/repos/{repo}/issues"

    body = f"""**Red Team Architecture Brief**

**Finding:** {issue_data.get('finding')}
**File:** {issue_data.get('file')}:{issue_data.get('line')}
**Severity:** {issue_data.get('severity')}

**Current Assumption:** {issue_data.get('current_assumption')}
**Attack Scenario:** {issue_data.get('attack_scenario')}
**Blast Radius:** {issue_data.get('blast_radius')}

**Minimal Reproducible Scenario:**
```
{issue_data.get('minimal_reproducible_scenario')}
```

**Recommended Structural Change:** {issue_data.get('recommended_structural_change')}
**Suggested Fix:** {issue_data.get('suggested_fix')}

---
*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
"""

    payload = {
        "title": f"Adversarial Audit: {issue_data.get('title')}",
        "body": body,
        "labels": ["bug", "agent-found", "architecture-review", "adversarial"]
    }

    req = urllib.request.Request(url, data=json.dumps(payload).encode('utf-8'), headers={
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    })

    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {payload['title']}")
    except Exception as e:
        print(f"Error creating issue: {e}")

def main():
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        print("OPENAI_API_KEY not found. Bypassing LLM logic to degrade gracefully.")
        sys.exit(0)

    github_token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')

    if not github_token or not repo:
        print("GITHUB_TOKEN or GITHUB_REPOSITORY missing. Cannot file issues.")
        sys.exit(0)

    event_name = os.environ.get('GITHUB_EVENT_NAME')

    if event_name == 'pull_request':
        print("PR event detected, scoping to modified files.")
        files_to_analyze = get_modified_files()
    else:
        print("Scheduled/Main run detected, analyzing full codebase.")
        files_to_analyze = get_all_files()

    for core_file in ['ARCHITECTURE.md', 'mock_audit_report.md']:
        if os.path.exists(core_file) and core_file not in files_to_analyze:
            files_to_analyze.append(core_file)

    if not files_to_analyze:
        print("No files to analyze.")
        sys.exit(0)

    print(f"Analyzing {len(files_to_analyze)} files...")
    findings = analyze_with_llm(files_to_analyze, api_key)

    if not findings:
        print("No findings from LLM.")
        sys.exit(0)

    existing_titles = get_existing_issues(repo, github_token)

    for finding in findings:
        title = f"Adversarial Audit: {finding.get('title')}"
        if title in existing_titles:
            print(f"Issue already exists: {title}")
            continue
        create_issue(repo, github_token, finding)

if __name__ == "__main__":
    main()
