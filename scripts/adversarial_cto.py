import os
import json
import urllib.request
import subprocess

def get_files_to_analyze(event_name):
    files = set(['ARCHITECTURE.md', 'mock_audit_report.md'])
    if event_name == 'schedule':
        for root, dirs, filenames in os.walk('mythrax-core/src'):
            for f in filenames:
                if f.endswith('.rs'):
                    files.add(os.path.join(root, f))
        for d in ['.agents', 'evals']:
            if os.path.exists(d):
                for root, dirs, filenames in os.walk(d):
                    for f in filenames:
                        files.add(os.path.join(root, f))
    else:
        try:
            diff_output = subprocess.check_output(['git', 'diff', '--name-only', 'HEAD^1', 'HEAD']).decode('utf-8')
            for f in diff_output.strip().split('\n'):
                if f:
                    files.add(f)
        except Exception as e:
            print(f"Error getting diff: {e}")

    existing_files = [f for f in files if os.path.isfile(f)]
    return existing_files

def get_file_contents(filepaths):
    contents = ""
    for fp in filepaths:
        try:
            with open(fp, 'r') as f:
                content = f.read()
                contents += f"\n--- {fp} ---\n{content}\n"
        except Exception as e:
            print(f"Could not read {fp}: {e}")
    return contents

def query_llm(file_contents):
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        print("No OPENAI_API_KEY found")
        return []

    system_prompt = (
        "You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax. "
        "You have no loyalty to the current design. You have read ARCHITECTURE.md and mock_audit_report.md in full, "
        "and your job is to find where the architecture will fail under load, under adversarial inputs, or under changing requirements. "
        "You are not here to validate — you are here to stress-fracture."
    )

    user_prompt = f"""Files to analyze:
{file_contents}

Task:
- Challenge every documented architectural decision in ARCHITECTURE.md — ask 'what assumption does this break if it's wrong?' for each one
- Identify single points of failure: any module, interface, or data path where failure has no graceful degradation path
- Evaluate the agent orchestration design for prompt injection vulnerabilities, unbounded recursion risk, and failure to enforce agent scope boundaries
- Assess whether the eval framework in evals/ actually tests adversarial inputs
- Identify coupling: any two modules that cannot be independently deployed, tested, or replaced without modifying both — flag as an architectural liability
- Project the architecture 18 months forward: name the top 3 decisions made today that will become re-architecture projects if the system scales 10x
- Analyze for off-by-one errors (especially in .agents/), panic paths (.unwrap()/.expect()), integer overflow/underflow, race conditions, silent logic failures, and test coverage gaps.

Output your findings strictly in valid JSON format as a list of objects. Each object must have:
- 'title': A short, descriptive title for the issue.
- 'body': The issue body formatted as a Red Team Architecture Brief containing:
  - Finding: ...
  - Current Assumption: ...
  - Attack Scenario: ...
  - Blast Radius: ...
  - Recommended Structural Change: ...
  - File/Line Number (if applicable): ...
  - Minimal Reproducible Scenario (if applicable): ...
  - Severity: ...
  - Suggested Fix (if applicable): ...
  - An explicit note reminding developers to never close the issue without a documented architectural decision record (ADR)."""

    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "response_format": {"type": "json_object"}
    }

    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=json.dumps(data).encode('utf-8'),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        }
    )

    try:
        response = urllib.request.urlopen(req)
        res_data = json.loads(response.read().decode('utf-8'))
        content = res_data['choices'][0]['message']['content']
        parsed = json.loads(content)
        for key in parsed:
            if isinstance(parsed[key], list):
                return parsed[key]
        if isinstance(parsed, list):
            return parsed
        return []
    except Exception as e:
        print(f"LLM query failed: {e}")
        return []

def get_existing_issues():
    token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    if not token or not repo:
        return []

    url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100"
    req = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github.v3+json"
        }
    )
    try:
        response = urllib.request.urlopen(req)
        issues = json.loads(response.read().decode('utf-8'))
        return [issue['title'] for issue in issues if 'pull_request' not in issue]
    except Exception as e:
        print(f"Failed to fetch issues: {e}")
        return []

def create_issue(issue):
    token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    if not token or not repo:
        print("Missing GITHUB_TOKEN or GITHUB_REPOSITORY, cannot create issue.")
        return

    url = f"https://api.github.com/repos/{repo}/issues"

    data = {
        "title": issue.get('title'),
        "body": issue.get('body'),
        "labels": ["architecture-review", "adversarial", "bug", "agent-found"]
    }

    req = urllib.request.Request(
        url,
        data=json.dumps(data).encode('utf-8'),
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github.v3+json",
            "Content-Type": "application/json"
        }
    )
    try:
        response = urllib.request.urlopen(req)
        print(f"Created issue: {issue.get('title')}")
    except Exception as e:
        print(f"Failed to create issue: {e}")

def main():
    event_name = os.environ.get('GITHUB_EVENT_NAME', 'schedule')
    files_to_analyze = get_files_to_analyze(event_name)
    print(f"Analyzing {len(files_to_analyze)} files.")

    if not files_to_analyze:
        return

    file_contents = get_file_contents(files_to_analyze)
    findings = query_llm(file_contents)

    if not findings:
        print("No findings returned.")
        return

    existing_issues = get_existing_issues()

    for finding in findings:
        title = finding.get('title')
        if title in existing_issues:
            print(f"Skipping duplicate issue: {title}")
            continue
        create_issue(finding)

if __name__ == "__main__":
    main()
