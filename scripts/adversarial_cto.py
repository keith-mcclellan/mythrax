#!/usr/bin/env python3
import os
import sys
import json
import urllib.request
import urllib.error
import subprocess

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout.strip(), result.stderr.strip(), result.returncode

def get_target_files():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "")
    files = set()
    if event_name == "pull_request":
        # Need to get modified files
        # In actions, GITHUB_SHA might be a merge commit, so diff against base
        base_sha = os.environ.get("GITHUB_BASE_REF", "main")
        stdout, _, _ = run_cmd(f"git diff --name-only origin/{base_sha}...HEAD")
        for line in stdout.split('\n'):
            if line and (line.endswith('.rs') or line.endswith('.py') or line.endswith('.md')):
                files.add(line)
    else:
        stdout, _, _ = run_cmd("git ls-tree -r --name-only HEAD")
        for line in stdout.split('\n'):
            if line and (line.endswith('.rs') or line.endswith('.py') or line.endswith('.md')):
                files.add(line)

    # Always include core context
    for core in ["ARCHITECTURE.md", "mock_audit_report.md"]:
        if os.path.exists(core):
            files.add(core)

    return list(files)

def call_llm(prompt, model="gpt-4o"):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("Error: OPENAI_API_KEY is not set.")
        return None

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    data = {
        "model": model,
        "messages": [
            {"role": "system", "content": "You are a CTO who has been burned by production incidents traced to subtle bugs that code review missed. You are methodical and assume the code is wrong until proven otherwise. Output valid JSON only, wrapped in ```json ... ``` blocks, with a list of findings."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.2
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            resp_data = json.loads(response.read().decode('utf-8'))
            return resp_data['choices'][0]['message']['content']
    except urllib.error.URLError as e:
        print(f"LLM API Call failed: {e}")
        return None

def parse_llm_response(response_text):
    if not response_text: return []
    try:
        # Extract json
        if "```json" in response_text:
            content = response_text.split("```json")[1].split("```")[0]
        elif "```" in response_text:
            content = response_text.split("```")[1].split("```")[0]
        else:
            content = response_text
        return json.loads(content)
    except Exception as e:
        print(f"Failed to parse JSON: {e}")
        return []

def get_existing_issues():
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        return []

    url = f"https://api.github.com/repos/{repo}/issues?state=all&labels=agent-found"
    headers = {
        "Accept": "application/vnd.github.v3+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode('utf-8'))
            return [issue['title'] for issue in issues]
    except urllib.error.URLError as e:
        print(f"Failed to get existing issues: {e}")
        return []

def create_issue(finding):
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print("Missing GITHUB_TOKEN or GITHUB_REPOSITORY. Cannot create issue.")
        return

    title = f"Adversarial CTO Finding: {finding.get('title', 'Subtle Bug Detected')}"

    existing = get_existing_issues()
    if title in existing:
        print(f"Issue already exists: {title}")
        return

    body = f"**File:** `{finding.get('file', 'Unknown')}:{finding.get('line', 'Unknown')}`\n"
    body += f"**Severity:** {finding.get('severity', 'Unknown')}\n\n"
    body += f"### Description\n{finding.get('description', '')}\n\n"
    body += f"### Minimal Reproducible Scenario\n{finding.get('scenario', '')}\n\n"
    body += f"### Suggested Fix\n```\n{finding.get('fix', '')}\n```\n\n"
    body += f"---\n\n"
    body += "⚠️ **CTO Mandate:** Never close this issue without a documented architectural decision record (ADR) response."

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Accept": "application/vnd.github.v3+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28",
        "Content-Type": "application/json"
    }
    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {title}")
    except urllib.error.URLError as e:
        print(f"Failed to create issue: {e}")
        if hasattr(e, 'read'):
            print(e.read().decode())

def main():
    files = get_target_files()
    if not files:
        print("No files to analyze.")
        return

    print(f"Analyzing {len(files)} files...")

    # Read file contents (limit sizes to avoid token limits)
    file_contents = []
    for f in files:
        if os.path.exists(f):
            try:
                with open(f, 'r') as file:
                    content = file.read()
                    # Truncate if too long, simple approach for now
                    if len(content) > 10000:
                        content = content[:10000] + "... [TRUNCATED]"
                    file_contents.append(f"File: {f}\n```\n{content}\n```\n")
            except Exception as e:
                pass

    prompt = "Analyze the following files for logic bugs, edge case vulnerabilities, and correctness issues:\n"
    prompt += "- Off-by-one errors in any loop, index, or range operation (pay special attention to .agents/)\n"
    prompt += "- Panic paths: .unwrap() or .expect() that could be triggered by external data\n"
    prompt += "- Integer overflow or underflow risks in arithmetic\n"
    prompt += "- Race conditions or incorrect assumptions about execution order\n"
    prompt += "- Logic branches where code silently succeeds but produces incorrect result\n"
    prompt += "- Test coverage gaps (public functions in mythrax-core with no corresponding test)\n\n"
    prompt += "Return a JSON list of objects. Each object MUST have:\n"
    prompt += "  'title': short descriptive title\n"
    prompt += "  'file': file path\n"
    prompt += "  'line': line number or range\n"
    prompt += "  'severity': (Critical/High/Medium/Low)\n"
    prompt += "  'description': what is the bug, current assumption vs attack scenario, blast radius, recommended structural change\n"
    prompt += "  'scenario': minimal reproducible scenario\n"
    prompt += "  'fix': suggested fix code\n\n"
    prompt += "Files:\n" + "\n".join(file_contents)

    response = call_llm(prompt)
    findings = parse_llm_response(response)

    if not findings:
        print("No findings or failed to parse.")
        return

    print("\n--- Red Team Architecture Brief ---")
    for idx, f in enumerate(findings):
        print(f"\nFinding {idx+1}: {f.get('title')}")
        print(f"File: {f.get('file')}:{f.get('line')}")
        print(f"Severity: {f.get('severity')}")
        print(f"Description:\n{f.get('description')}")
        print(f"Scenario:\n{f.get('scenario')}")

        create_issue(f)

if __name__ == "__main__":
    main()
