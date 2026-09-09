import os
import sys
import json
import urllib.request
import urllib.error
import subprocess

OPENAI_API_KEY = os.environ.get("OPENAI_API_KEY")
GITHUB_TOKEN = os.environ.get("GITHUB_TOKEN")
GITHUB_REPOSITORY = os.environ.get("GITHUB_REPOSITORY")
EVENT_NAME = os.environ.get("GITHUB_EVENT_NAME", "schedule")

if not OPENAI_API_KEY:
    print("OPENAI_API_KEY not found. Bypassing LLM logic gracefully.")
    sys.exit(0)

def get_existing_issues():
    if not GITHUB_TOKEN or not GITHUB_REPOSITORY:
        return []
    url = f"https://api.github.com/repos/{GITHUB_REPOSITORY}/issues?state=all"
    try:
        req = urllib.request.Request(url, headers={
            "Authorization": f"Bearer {GITHUB_TOKEN}",
            "Accept": "application/vnd.github.v3+json"
        })
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode())
            return [issue["title"] for issue in issues]
    except Exception as e:
        print(f"Failed to fetch issues: {e}")
        return []

def create_issue(title, body, labels):
    if not GITHUB_TOKEN or not GITHUB_REPOSITORY:
        print(f"Mock Issue Creation:\nTitle: {title}\nLabels: {labels}\nBody:\n{body}")
        return
    url = f"https://api.github.com/repos/{GITHUB_REPOSITORY}/issues"
    data = json.dumps({
        "title": title,
        "body": body,
        "labels": labels
    }).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers={
        "Authorization": f"Bearer {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    })
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Successfully created issue: {title}")
    except Exception as e:
        print(f"Failed to create issue {title}: {e}")

def get_modified_files():
    try:
        # Assuming fetch-depth: 0 is used in checkout
        if EVENT_NAME == "pull_request":
            # For PR, diff against the base branch, but HEAD~1 is often a reasonable proxy in GH Actions if properly configured
            # GH Actions sets GITHUB_BASE_REF
            base_ref = os.environ.get("GITHUB_BASE_REF")
            if base_ref:
                subprocess.run(["git", "fetch", "origin", base_ref], capture_output=True)
                cmd = ["git", "diff", "--name-only", f"origin/{base_ref}"]
            else:
                cmd = ["git", "diff", "--name-only", "HEAD~1", "HEAD"]
        else:
            cmd = ["git", "diff", "--name-only", "HEAD~1", "HEAD"]

        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode == 0:
            return [f for f in result.stdout.strip().split("\n") if f]
    except Exception as e:
        print(f"Failed to get modified files: {e}")
    return []

def read_file(filepath):
    if os.path.exists(filepath):
        with open(filepath, "r") as f:
            return f.read()
    return ""

def gather_context():
    context = {}
    context["ARCHITECTURE.md"] = read_file("ARCHITECTURE.md")
    context["mock_audit_report.md"] = read_file("mock_audit_report.md")

    if EVENT_NAME == "schedule":
        cmd = ["git", "ls-files", "evals/", ".agents/", "mythrax-core/src/"]
        result = subprocess.run(cmd, capture_output=True, text=True)
        files_to_read = result.stdout.strip().split("\n") if result.returncode == 0 else []
        for f in files_to_read:
            if f and os.path.exists(f) and f not in context:
                context[f] = read_file(f)
    else:
        mod_files = get_modified_files()
        for f in mod_files:
            if os.path.exists(f) and f not in context:
                context[f] = read_file(f)

    context_str = ""
    for k, v in context.items():
        if len(v) > 10000:
            v = v[:10000] + "\n...[truncated]..."
        context_str += f"--- {k} ---\n{v}\n\n"

    return context_str

def call_openai(context_str):
    prompt = f"""
You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax. You have no loyalty to the current design.

Your job is to find where the architecture will fail under load, under adversarial inputs, or under changing requirements. You are not here to validate — you are here to stress-fracture.

Tasks:
1. Challenge every documented architectural decision in ARCHITECTURE.md — ask "what assumption does this break if it's wrong?" for each one.
2. Identify single points of failure: any module, interface, or data path where failure has no graceful degradation path.
3. Evaluate the agent orchestration design for prompt injection vulnerabilities, unbounded recursion risk, and failure to enforce agent scope boundaries.
4. Assess whether the eval framework in evals/ actually tests adversarial inputs.
5. Identify coupling: any two modules that cannot be independently deployed, tested, or replaced without modifying both.
6. Project the architecture 18 months forward: name the top 3 decisions made today that will become re-architecture projects if the system scales 10x.
7. Subtle bugs: analyze code for off-by-one errors (esp. in agent context), panic paths (unwrap/expect with external data), integer overflow/underflow in scoring, race conditions, silent logic failures, and test coverage gaps.

Output MUST be a JSON object with an "issues" array. Each issue must have:
- "title": A short, descriptive title.
- "body": The markdown body.
- "type": Either "architecture" or "bug".

For "architecture" issues, structure the body exactly as a Red Team Architecture Brief:
- **Finding**: ...
- **Current Assumption**: ...
- **Attack Scenario**: ...
- **Blast Radius**: ...
- **Recommended Structural Change**: ...

For "bug" issues, include in the body:
- **File/Line**: ...
- **Minimal Reproducible Scenario**: ...
- **Severity**: ...
- **Suggested Fix**: ...

Here is the codebase context:
{context_str}
"""
    url = "https://api.openai.com/v1/chat/completions"
    data = json.dumps({
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": "You are a JSON-producing assistant. Always return valid JSON."},
            {"role": "user", "content": prompt}
        ],
        "response_format": {"type": "json_object"}
    }).encode("utf-8")

    req = urllib.request.Request(url, data=data, headers={
        "Authorization": f"Bearer {OPENAI_API_KEY}",
        "Content-Type": "application/json"
    })

    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode())
            content = result["choices"][0]["message"]["content"]
            parsed = json.loads(content)
            if "issues" in parsed:
                return parsed["issues"]
            return []
    except Exception as e:
        print(f"OpenAI API call failed: {e}")
        return []

def main():
    existing_titles = get_existing_issues()
    context_str = gather_context()

    issues = call_openai(context_str)

    for issue in issues:
        title = issue.get("title", "Adversarial Finding")
        body = issue.get("body", "")
        issue_type = issue.get("type", "architecture")

        if any(title.lower() == et.lower() for et in existing_titles):
            print(f"Skipping duplicate issue: {title}")
            continue

        if issue_type == "bug":
            labels = ["bug", "agent-found"]
        else:
            labels = ["architecture-review", "adversarial"]

        body += "\n\n**Note:** Never close this issue without a documented architectural decision record (ADR) response."

        create_issue(title, body, labels)

if __name__ == "__main__":
    main()
