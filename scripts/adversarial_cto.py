import os
import sys
import json
import subprocess
import urllib.request
import urllib.error

PROMPT = """You are an Adversarial CTO. Your job is to challenge the architecture and find vulnerabilities.
Review the following code files.

Tasks:
1. Challenge every documented architectural decision in ARCHITECTURE.md - ask "what assumption does this break if it's wrong?"
2. Identify single points of failure: any module, interface, or data path where failure has no graceful degradation path.
3. Evaluate the agent orchestration design for prompt injection vulnerabilities, unbounded recursion risk, and failure to enforce agent scope boundaries.
4. Assess whether the eval framework in evals/ actually tests adversarial inputs.
5. Identify coupling: any two modules that cannot be independently deployed, tested, or replaced without modifying both.
6. Project the architecture 18 months forward: name the top 3 decisions made today that will become re-architecture projects if the system scales 10x.
7. Code-level Bugs: Off-by-one errors (especially in .agents/), panic paths (.unwrap()/.expect()), integer overflow/underflow, race conditions, silent logic failures, and test coverage gaps.

Return a JSON object with two lists:
"architecture_findings": [
  {
    "finding": "Short title of finding",
    "current_assumption": "What assumption is broken",
    "attack_scenario": "How it can be exploited",
    "blast_radius": "Impact",
    "recommended_structural_change": "Fix"
  }
],
"bug_findings": [
  {
    "finding": "Short title",
    "file_line": "File and line number",
    "scenario": "Minimal reproducible scenario",
    "severity": "CRITICAL, HIGH, MEDIUM, LOW",
    "suggested_fix": "How to fix"
  }
]
"""

def get_modified_files():
    try:
        result = subprocess.run(["git", "diff", "--name-only", "HEAD^1", "HEAD"], capture_output=True, text=True, check=True)
        return [f for f in result.stdout.split('\n') if f.strip() and os.path.exists(f.strip())]
    except subprocess.CalledProcessError:
        return []

def get_all_files():
    files = []
    for root, _, filenames in os.walk('.'):
        if '.git' in root or 'target' in root:
            continue
        for filename in filenames:
            if filename.endswith(('.rs', '.md', '.py', '.toml', '.json')):
                files.append(os.path.join(root, filename))
    return files

def get_existing_issues(repo, token):
    if not repo or not token:
        return []
    issues = []
    page = 1
    while True:
        url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100&page={page}"
        req = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}", "Accept": "application/vnd.github.v3+json"})
        try:
            with urllib.request.urlopen(req) as response:
                page_issues = json.loads(response.read().decode("utf-8"))
                if not page_issues:
                    break
                issues.extend([issue["title"] for issue in page_issues])
                page += 1
        except Exception as e:
            print(f"Error fetching issues: {e}")
            break
    return issues

def create_issue(repo, token, title, body, labels):
    if not repo or not token:
        print(f"Mock creating issue: {title}")
        return
    url = f"https://api.github.com/repos/{repo}/issues"
    data = json.dumps({"title": title, "body": body, "labels": labels}).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers={"Authorization": f"Bearer {token}", "Accept": "application/vnd.github.v3+json"})
    try:
        urllib.request.urlopen(req)
        print(f"Created issue: {title}")
    except Exception as e:
        print(f"Error creating issue {title}: {e}")

def call_openai(prompt, content):
    api_key = os.environ.get("OPENAI_API_KEY", "dummy")
    if api_key == "dummy":
        return {
            "architecture_findings": [{"finding": "Mock finding", "current_assumption": "Mock", "attack_scenario": "Mock", "blast_radius": "Mock", "recommended_structural_change": "Mock"}],
            "bug_findings": [{"finding": "Mock bug", "file_line": "mock.rs:10", "scenario": "Mock", "severity": "HIGH", "suggested_fix": "Mock"}]
        }

    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=json.dumps({
            "model": "gpt-4-turbo",
            "response_format": {"type": "json_object"},
            "messages": [
                {"role": "system", "content": prompt},
                {"role": "user", "content": content}
            ]
        }).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        }
    )
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode("utf-8"))
            return json.loads(result["choices"][0]["message"]["content"])
    except Exception as e:
        print(f"Error calling OpenAI: {e}")
        return {"architecture_findings": [], "bug_findings": []}

def main():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "push")
    if event_name == "schedule":
        files = get_all_files()
    else:
        files = get_modified_files()
        for extra in ["ARCHITECTURE.md", "mock_audit_report.md"]:
            if os.path.exists(extra) and extra not in files:
                files.append(extra)

    chunks = []
    current_chunk = ""
    for f in files:
        try:
            with open(f, 'r', encoding='utf-8') as file:
                content = file.read()
                if len(current_chunk) + len(content) > 100000:
                    chunks.append(current_chunk)
                    current_chunk = ""
                current_chunk += f"\n--- {f} ---\n" + content
        except Exception:
            pass
    if current_chunk:
        chunks.append(current_chunk)

    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")
    existing_issues = get_existing_issues(repo, token)

    for chunk in chunks:
        res = call_openai(PROMPT, chunk)
        for arch in res.get("architecture_findings", []):
            title = f"[Adversarial CTO] {arch.get('finding', 'Unknown Architecture Finding')}"
            if title in existing_issues:
                continue
            body = f"**Current Assumption:** {arch.get('current_assumption', 'N/A')}\n\n"
            body += f"**Attack Scenario:** {arch.get('attack_scenario', 'N/A')}\n\n"
            body += f"**Blast Radius:** {arch.get('blast_radius', 'N/A')}\n\n"
            body += f"**Recommended Structural Change:** {arch.get('recommended_structural_change', 'N/A')}\n\n"
            body += "Note: Never close this issue without a documented architectural decision record (ADR) response."
            create_issue(repo, token, title, body, ["architecture-review", "adversarial"])
            existing_issues.append(title)

        for bug in res.get("bug_findings", []):
            title = f"[Bug] {bug.get('finding', 'Unknown Bug')}"
            if title in existing_issues:
                continue
            body = f"**File/Line:** {bug.get('file_line', 'N/A')}\n\n"
            body += f"**Scenario:** {bug.get('scenario', 'N/A')}\n\n"
            body += f"**Severity:** {bug.get('severity', 'N/A')}\n\n"
            body += f"**Suggested Fix:** {bug.get('suggested_fix', 'N/A')}\n\n"
            body += "Note: Never close this issue without a documented architectural decision record (ADR) response."
            create_issue(repo, token, title, body, ["bug", "agent-found"])
            existing_issues.append(title)

if __name__ == "__main__":
    main()
