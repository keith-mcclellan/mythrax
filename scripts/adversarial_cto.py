import os
import sys
import json
import urllib.request
import urllib.error
import subprocess

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    if result.returncode != 0:
        pass
    return result.stdout.strip()

def get_modified_files():
    stdout = run_cmd("git diff --name-only HEAD^1 HEAD")
    if not stdout:
        return []
    return [f for f in stdout.split('\n') if f]

def get_all_files():
    stdout = run_cmd("find mythrax-core evals .agents -type f")
    if not stdout:
        return []
    return [f for f in stdout.split('\n') if f and f.endswith(('.rs', '.py', '.md', '.json', '.yaml', '.toml'))]

def get_file_content(filepath):
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            return f.read()
    except Exception:
        return ""

def call_llm(prompt, model="gpt-4o"):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("No OPENAI_API_KEY provided, skipping LLM call.")
        return "{}"

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    data = {
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "You are a CTO who has been burned by production incidents traced to subtle bugs. You analyze code for logic bugs, edge case vulnerabilities, and correctness issues."
            },
            {
                "role": "user",
                "content": prompt
            }
        ],
        "response_format": {"type": "json_object"}
    }
    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req) as response:
            res_body = response.read().decode("utf-8")
            res_json = json.loads(res_body)
            return res_json["choices"][0]["message"]["content"]
    except Exception as e:
        print(f"LLM API Error: {e}")
        return "{}"

def chunk_list(lst, chunk_size):
    for i in range(0, len(lst), chunk_size):
        yield lst[i:i + chunk_size]

def analyze_files(files_to_analyze):
    context_files = ["ARCHITECTURE.md", "mock_audit_report.md"]
    context_content = ""
    for cf in set(context_files):
        if os.path.exists(cf):
            content = get_file_content(cf)
            if content:
                context_content += f"\n--- {cf} ---\n{content}\n"

    findings = []
    target_dirs = ("mythrax-core/", "evals/", ".agents/")
    valid_files = [f for f in files_to_analyze if any(f.startswith(d) for d in target_dirs) and os.path.exists(f)]

    for chunk in chunk_list(valid_files, 5):
        chunk_content = ""
        for f in chunk:
            content = get_file_content(f)
            if content:
                chunk_content += f"\n--- {f} ---\n{content}\n"

        if not chunk_content:
            continue

        prompt = f"""You are a CTO analyzing code for bugs.
Analyze the following codebase files for:
1. Off-by-one errors (loops, indexes, AI agent context window management in .agents/).
2. Panic paths (.unwrap() / .expect()) with realistic triggers.
3. Integer overflow/underflow (agent scoring, tokens).
4. Race conditions in async/concurrent code.
5. Silently failing logic branches.
6. Test coverage gaps (public functions in mythrax-core with no test).

Context:
{context_content}

Files to analyze:
{chunk_content}

Return JSON with a 'findings' list. Each finding must have:
- title: string
- file: string
- line: string or number
- scenario: minimal reproducible scenario
- severity: string (Critical, High, Medium, Low)
- fix: suggested fix
- is_bug: boolean (true if realistic trigger path)
"""
        response = call_llm(prompt)
        try:
            res_data = json.loads(response)
            file_findings = res_data.get("findings", [])
            findings.extend(file_findings)
        except Exception as e:
            print(f"Failed to parse LLM response: {e}")

    return findings

def get_existing_issues(repo, token):
    url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "User-Agent": "Adversarial-CTO-Script"
    }
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode("utf-8"))
            return [issue["title"] for issue in issues if "title" in issue]
    except Exception as e:
        print(f"Error fetching issues: {e}")
        return []

def create_issue(repo, token, finding):
    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json",
        "User-Agent": "Adversarial-CTO-Script"
    }

    body = f"**File:** {finding.get('file')}\n**Line:** {finding.get('line')}\n**Severity:** {finding.get('severity')}\n\n**Minimal Reproducible Scenario:**\n{finding.get('scenario')}\n\n**Suggested Fix:**\n{finding.get('fix')}\n\n**Note:** Never close this issue without a documented architectural decision record (ADR)."

    data = {
        "title": finding.get("title"),
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Successfully created issue: {finding.get('title')}")
    except Exception as e:
        print(f"Error creating issue '{finding.get('title')}': {e}")

def main():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "schedule")
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")

    if not token or not repo:
        print("Missing GITHUB_TOKEN or GITHUB_REPOSITORY. Running dry-run mode for testing.")

    if event_name == "pull_request":
        print("Running on pull request. Analyzing modified files...")
        files_to_analyze = get_modified_files()
    else:
        print("Running on schedule (or push). Analyzing full codebase...")
        files_to_analyze = get_all_files()

    print(f"Found {len(files_to_analyze)} valid files to analyze.")
    if not files_to_analyze:
        return

    findings = analyze_files(files_to_analyze)

    existing_issues = []
    if token and repo:
        existing_issues = get_existing_issues(repo, token)

    for finding in findings:
        if not finding.get("is_bug"):
            continue

        title = finding.get("title")
        if title in existing_issues:
            print(f"Issue already exists for: {title}")
            continue

        if token and repo:
            create_issue(repo, token, finding)
        else:
            print(f"[DRY RUN] Would create issue: {title}")

if __name__ == "__main__":
    main()
