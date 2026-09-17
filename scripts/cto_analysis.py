import os
import json
import urllib.request
import subprocess

def get_modified_files():
    event_name = os.environ.get("EVENT_NAME", "")
    if event_name == "schedule":
        files = []
        for d in ["mythrax-core", "evals", ".agents"]:
            if os.path.isdir(d):
                for root, _, filenames in os.walk(d):
                    for f in filenames:
                        files.append(os.path.join(root, f))
        return files

    try:
        output = subprocess.check_output(["git", "diff", "--name-only", "HEAD^1", "HEAD"], text=True)
        files = [f for f in output.splitlines() if f]
        relevant = []
        for f in files:
            if f.startswith("mythrax-core/") or f.startswith("evals/") or f.startswith(".agents/"):
                relevant.append(f)
        return relevant
    except Exception as e:
        print(f"Error getting modified files: {e}")
        return []

def get_file_content(path):
    if not os.path.exists(path):
        return None
    with open(path, "r") as f:
        return f.read()

def chunk_files(files, max_chars=40000):
    chunks = []
    current_chunk = []
    current_size = 0
    for f in files:
        content = get_file_content(f)
        if not content:
            continue
        size = len(content)
        if current_size + size > max_chars and current_chunk:
            chunks.append(current_chunk)
            current_chunk = [(f, content)]
            current_size = size
        else:
            current_chunk.append((f, content))
            current_size += size
    if current_chunk:
        chunks.append(current_chunk)
    return chunks

def query_openai(prompt):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found.")
        return {"findings": []}

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    data = {
        "model": "gpt-4-turbo",
        "messages": [
            {
                "role": "system",
                "content": "You are a CTO who has been burned by production incidents traced to subtle bugs that code review missed. You are methodical and assume the code is wrong until proven otherwise. Analyze the code for logic bugs, edge case vulnerabilities, correctness issues, off-by-one errors, panic paths (.unwrap()/.expect()), integer overflow/underflow, race conditions, logic branches that silently succeed but are incorrect, and test coverage gaps."
            },
            {"role": "user", "content": prompt}
        ],
        "response_format": {"type": "json_object"},
        "temperature": 0.2
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode("utf-8"))
            return json.loads(result["choices"][0]["message"]["content"])
    except Exception as e:
        print(f"Error querying OpenAI: {e}")
        return {"findings": []}

def get_existing_issues(repo, token):
    url = f"https://api.github.com/repos/{repo}/issues?state=open&per_page=100"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json"
    }
    issues = []
    try:
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode("utf-8"))
            issues.extend(data)
    except Exception as e:
        print(f"Error getting issues: {e}")
    return issues

def create_issue(repo, token, title, body, labels):
    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }
    data = {
        "title": title,
        "body": body,
        "labels": labels
    }
    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {title}")
    except Exception as e:
        print(f"Error creating issue: {e}")

def main():
    files_to_analyze = get_modified_files()
    if not files_to_analyze:
        print("No relevant files modified.")
        return

    context_files = ["ARCHITECTURE.md", "mock_audit_report.md"]
    context_content = ""
    for ctx in context_files:
        content = get_file_content(ctx)
        if content:
            context_content += f"--- {ctx} ---\n{content}\n\n"

    chunks = chunk_files(files_to_analyze)
    all_findings = []

    for chunk in chunks:
        prompt = "Review the following files and identify logic bugs, panic paths, edge case vulnerabilities, and test coverage gaps.\n\n"
        prompt += context_content
        for f, content in chunk:
            prompt += f"--- {f} ---\n{content}\n\n"

        prompt += """
Respond in JSON format with a "findings" array. Each finding must have:
- title: A short title for the issue
- file: The file path
- line: The line number (approximate is fine)
- scenario: A minimal reproducible scenario
- severity: (e.g., High, Medium, Low)
- fix: Suggested fix
"""
        result = query_openai(prompt)
        all_findings.extend(result.get("findings", []))

    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print("GITHUB_TOKEN or GITHUB_REPOSITORY not set. Cannot create issues.")
        print(json.dumps(all_findings, indent=2))
        return

    existing_issues = get_existing_issues(repo, token)
    existing_titles = set(issue.get("title", "") for issue in existing_issues)

    for finding in all_findings:
        title = f"Adversarial CTO: {finding.get('title', 'Logic Bug')}"
        if title in existing_titles:
            print(f"Skipping duplicate issue: {title}")
            continue

        body = f"**File:** `{finding.get('file', 'Unknown')}`\n**Line:** {finding.get('line', 'Unknown')}\n**Severity:** {finding.get('severity', 'Unknown')}\n\n**Minimal Reproducible Scenario:**\n{finding.get('scenario', 'None provided')}\n\n**Suggested Fix:**\n{finding.get('fix', 'None provided')}\n\n> [!NOTE]\n> Reminder to developers: Never close this issue without a documented architectural decision record (ADR).\n"
        create_issue(repo, token, title, body, ["bug", "agent-found"])

if __name__ == "__main__":
    main()
