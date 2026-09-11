import os
import json
import subprocess
import urllib.request
import urllib.error

GITHUB_TOKEN = os.environ.get("GITHUB_TOKEN")
OPENAI_API_KEY = os.environ.get("OPENAI_API_KEY")
GITHUB_REPOSITORY = os.environ.get("GITHUB_REPOSITORY")
EVENT_NAME = os.environ.get("GITHUB_EVENT_NAME")
BASE_REF = os.environ.get("GITHUB_BASE_REF")

def get_modified_files():
    if EVENT_NAME == "pull_request":
        try:
            subprocess.run(["git", "fetch", "origin", BASE_REF], check=True)
            result = subprocess.run(["git", "diff", "--name-only", f"origin/{BASE_REF}"], capture_output=True, text=True, check=True)
            files = result.stdout.strip().split("\n")
        except Exception as e:
            print(f"Error fetching modified files for PR: {e}")
            files = []
    elif EVENT_NAME == "push":
        try:
            result = subprocess.run(["git", "diff", "--name-only", "HEAD~1"], capture_output=True, text=True, check=True)
            files = result.stdout.strip().split("\n")
        except subprocess.CalledProcessError:
            files = []
    else:
        files = []
        for d in ["mythrax-core/src", ".agents", "evals"]:
            if not os.path.exists(d):
                continue
            for root, _, filenames in os.walk(d):
                for f in filenames:
                    if f.endswith(".rs") or f.endswith(".md") or f.endswith(".py"):
                        files.append(os.path.join(root, f))
        return [f for f in files if f]

    valid_files = []
    for f in files:
        if not f: continue
        if os.path.exists(f) and (f.startswith("mythrax-core/src") or f.startswith(".agents") or f.startswith("evals")):
            valid_files.append(f)

    for f in ["ARCHITECTURE.md", "mock_audit_report.md"]:
        if f not in valid_files and os.path.exists(f):
            valid_files.append(f)

    return valid_files

def query_openai(prompt, system_prompt):
    if not OPENAI_API_KEY:
        print("No OPENAI_API_KEY provided.")
        return '{"findings": []}'

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {OPENAI_API_KEY}"
    }

    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.2,
        "response_format": {"type": "json_object"}
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            res_body = response.read().decode("utf-8")
            res_json = json.loads(res_body)
            return res_json["choices"][0]["message"]["content"]
    except Exception as e:
        print(f"OpenAI API Error: {e}")
        return '{"findings": []}'

def get_existing_issues():
    if not GITHUB_TOKEN or not GITHUB_REPOSITORY:
        return []

    url = f"https://api.github.com/repos/{GITHUB_REPOSITORY}/issues?state=all&per_page=100"
    headers = {
        "Authorization": f"Bearer {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json"
    }
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode("utf-8"))
            return [issue["title"] for issue in issues]
    except Exception as e:
        print(f"GitHub API Error when fetching issues: {e}")
        return []

def create_issue(title, body, labels):
    if not GITHUB_TOKEN or not GITHUB_REPOSITORY:
        print("Mock issue creation (no GITHUB_TOKEN):", title)
        print(body)
        return

    url = f"https://api.github.com/repos/{GITHUB_REPOSITORY}/issues"
    headers = {
        "Authorization": f"Bearer {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }
    data = {
        "title": title,
        "body": body,
        "labels": labels
    }
    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            res = json.loads(response.read().decode("utf-8"))
            print(f"Created issue: {res['html_url']}")
    except Exception as e:
        print(f"GitHub API Error when creating issue: {e}")

def analyze_file(file_path):
    print(f"Analyzing {file_path}...")
    try:
        with open(file_path, "r", encoding="utf-8") as f:
            content = f.read()
    except Exception as e:
        print(f"Error reading {file_path}: {e}")
        return []

    system_prompt = """You are an Adversarial CTO. You must analyze the provided code for logic bugs, edge case vulnerabilities, and correctness issues.
Focus on:
1. Off-by-one errors in loops, indices, or ranges (especially agent scaffolding/context in .agents).
2. Panic paths: .unwrap() or .expect() that could be triggered by realistic runtime inputs (external data, LLMs).
3. Integer overflow/underflow risks in agent scoring, token counting, or evals.
4. Race conditions or execution order assumptions in async/concurrent code.
5. Logic branches where code silently succeeds but produces incorrect results (false positives).
6. Test coverage gaps (public functions with no tests).

Output JSON with a "findings" key containing a list of objects. Each object must have:
- title: Short descriptive title
- file: The filename
- line: Line number (or a range)
- severity: Critical, High, Medium, or Low
- red_team_brief: Object with keys: "finding", "current_assumption", "attack_scenario", "blast_radius", "recommended_structural_change"
- minimal_reproducible_scenario: String describing how to trigger it
"""

    prompt = f"File: {file_path}\n\nCode:\n{content}"
    response = query_openai(prompt, system_prompt)
    try:
        parsed = json.loads(response)
        return parsed.get("findings", [])
    except Exception as e:
        print(f"Error parsing LLM response for {file_path}: {e}")
        return []

def main():
    files_to_analyze = get_modified_files()
    print(f"Files to analyze: {files_to_analyze}")

    existing_issues = get_existing_issues()

    all_findings = []
    for file_path in files_to_analyze:
        findings = analyze_file(file_path)
        all_findings.extend(findings)

    for finding in all_findings:
        title = f"[Adversarial CTO] {finding['title']}"

        if title in existing_issues:
            print(f"Skipping duplicate issue: {title}")
            continue

        brief = finding.get("red_team_brief", {})

        body = f"""## Red Team Architecture Brief

**Finding:** {brief.get('finding', 'N/A')}
**Current Assumption:** {brief.get('current_assumption', 'N/A')}
**Attack Scenario:** {brief.get('attack_scenario', 'N/A')}
**Blast Radius:** {brief.get('blast_radius', 'N/A')}
**Recommended Structural Change:** {brief.get('recommended_structural_change', 'N/A')}

**File:** `{finding.get('file')}` (Line: {finding.get('line', 'N/A')})
**Severity:** {finding.get('severity', 'N/A')}

### Minimal Reproducible Scenario
{finding.get('minimal_reproducible_scenario', 'N/A')}

> **Note:** Developers, never close this issue without a documented architectural decision record (ADR) response.
"""
        create_issue(title, body, ["bug", "agent-found"])
        existing_issues.append(title)

if __name__ == "__main__":
    main()
