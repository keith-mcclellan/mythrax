import os
import json
import urllib.request
import subprocess

def run_command(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout.strip()

def get_modified_files():
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name in ("pull_request", "push"):
        output = run_command("git diff --name-only HEAD^1 HEAD")
        if output:
            return [f for f in output.split('\n') if f.strip() and os.path.exists(f.strip())]
    elif event_name == "schedule" or not event_name:
        output = run_command("find mythrax-core evals .agents -type f -name '*.rs' -o -name '*.py' -o -name '*.md'")
        if output:
            return [f for f in output.split('\n') if f.strip() and os.path.exists(f.strip())]
    return []

def get_files_to_analyze():
    files = get_modified_files()
    core_files = ["ARCHITECTURE.md", "mock_audit_report.md"]
    for f in core_files:
        if os.path.exists(f) and f not in files:
            files.append(f)
    return [f for f in files if os.path.exists(f)]

def call_openai_api(files_content):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not set")
        return []

    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    system_prompt = """You are an Adversarial CTO. You analyze code for logic bugs, edge case vulnerabilities, and correctness issues:
1. Off-by-one errors in any loop, index, or range operation — pay special attention to the AI agent scaffolding and context window management in .agents/
2. Panic paths: any .unwrap() or .expect() that could be triggered by realistic runtime inputs, especially those processing external data or LLM responses
3. Integer overflow or underflow risks in any arithmetic involving agent scoring, token counting, or eval metrics
4. Race conditions or incorrect assumptions about execution order in any async or concurrent code paths
5. Logic branches where the code silently succeeds but produces an incorrect result (false positives worse than panics)
6. Test coverage gaps — identify any public functions in mythrax-core with no corresponding test in the eval or test suite

Return a JSON object with a 'findings' array. Each finding must have:
- 'title': Short descriptive title
- 'file_line': The file and line number
- 'reproducible_scenario': A minimal reproducible scenario
- 'severity': Severity level
- 'suggested_fix': Suggested fix code or description
"""

    user_prompt = "Analyze the following files:\n" + files_content

    data = {
        "model": "gpt-4-turbo",
        "response_format": {"type": "json_object"},
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]
    }

    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=json.dumps(data).encode("utf-8"),
        headers=headers,
        method="POST"
    )

    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode("utf-8"))
            content = result["choices"][0]["message"]["content"]
            return json.loads(content).get("findings", [])
    except Exception as e:
        print(f"Error calling OpenAI API: {e}")
        return []

def get_existing_issues(repo, token):
    issues = []
    page = 1
    while True:
        url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100&page={page}"
        req = urllib.request.Request(url, headers={
            "Authorization": f"token {token}",
            "Accept": "application/vnd.github.v3+json"
        })
        try:
            with urllib.request.urlopen(req) as response:
                page_issues = json.loads(response.read().decode("utf-8"))
                if not page_issues:
                    break
                issues.extend(page_issues)
                page += 1
        except Exception as e:
            print(f"Error fetching existing issues: {e}")
            break
    return issues

def create_github_issue(repo, token, finding, existing_issues):
    title = f"Adversarial CTO Finding: {finding['title']}"
    for issue in existing_issues:
        if issue["title"] == title:
            print(f"Issue '{title}' already exists. Skipping.")
            return

    body = f"""### File & Line Number
{finding['file_line']}

### Minimal Reproducible Scenario
{finding['reproducible_scenario']}

### Severity
{finding['severity']}

### Suggested Fix
{finding['suggested_fix']}

> **Note:** Do not close this issue without a documented architectural decision record (ADR).
"""

    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    url = f"https://api.github.com/repos/{repo}/issues"
    req = urllib.request.Request(url, headers={
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }, data=json.dumps(data).encode("utf-8"), method="POST")

    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {title}")
    except Exception as e:
        print(f"Error creating issue '{title}': {e}")

def main():
    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")

    if not repo or not token:
        print("GITHUB_REPOSITORY or GITHUB_TOKEN not set. Running locally without issue creation.")

    files_to_analyze = get_files_to_analyze()
    if not files_to_analyze:
        print("No files to analyze.")
        return

    MAX_CHARS = 100000
    file_chunks = []
    current_chunk = ""
    for f in files_to_analyze:
        try:
            with open(f, "r") as file:
                content = f"\n--- {f} ---\n{file.read()}\n"
                if len(current_chunk) + len(content) > MAX_CHARS and current_chunk:
                    file_chunks.append(current_chunk)
                    current_chunk = content
                else:
                    current_chunk += content
        except Exception as e:
            print(f"Could not read {f}: {e}")
    if current_chunk:
        file_chunks.append(current_chunk)

    all_findings = []
    for chunk in file_chunks:
        findings = call_openai_api(chunk)
        if findings:
            all_findings.extend(findings)

    existing_issues = []
    if repo and token:
        existing_issues = get_existing_issues(repo, token)

    for finding in all_findings:
        if repo and token:
            create_github_issue(repo, token, finding, existing_issues)
        else:
            print(f"Found issue: {finding['title']}\n{finding}\n")

if __name__ == "__main__":
    main()
