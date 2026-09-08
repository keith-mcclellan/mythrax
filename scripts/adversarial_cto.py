import os
import sys
import json
import urllib.request
import subprocess

def get_modified_files():
    base_ref = os.environ.get("GITHUB_BASE_REF")
    if not base_ref:
        # Fallback if not available
        return []

    cmd = ["git", "fetch", "origin", base_ref]
    subprocess.run(cmd, check=False)

    cmd = ["git", "diff", "--name-only", f"origin/{base_ref}...HEAD"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode == 0:
        files = result.stdout.strip().split('\n')
        return [f for f in files if f and os.path.exists(f)]
    return []

def get_all_files():
    target_dirs = ["mythrax-core", "evals", ".agents"]
    files_to_check = []
    for d in target_dirs:
        for root, _, files in os.walk(d):
            for file in files:
                if file.endswith((".rs", ".md", ".py")):
                    files_to_check.append(os.path.join(root, file))
    return files_to_check

def analyze_file_with_llm(filepath, content, api_key):
    prompt = f"""You are a CTO who has been burned by production incidents traced to subtle bugs that code review missed. You are methodical and assume the code is wrong until proven otherwise.
Analyze the following code for logic bugs, edge case vulnerabilities, and correctness issues:
• Off-by-one errors in any loop, index, or range operation
• Panic paths: any .unwrap() or .expect() that could be triggered by realistic runtime inputs
• Integer overflow or underflow risks in arithmetic
• Race conditions or incorrect assumptions about execution order
• Logic branches where the code silently succeeds but produces an incorrect result
• Test coverage gaps

For each bug found that has a realistic trigger path, output a JSON object in a list. If no bugs, return an empty list [].
Do not wrap the JSON output in markdown code blocks. Output ONLY valid JSON.
Each object must have:
"title": "A short, descriptive title",
"finding": "Description of the finding",
"current_assumption": "Current Assumption",
"attack_scenario": "Attack Scenario (minimal reproducible scenario)",
"blast_radius": "Blast Radius",
"recommended_structural_change": "Recommended Structural Change (suggested fix)",
"file": "{filepath}",
"line": "line number",
"severity": "Severity"
"""

    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": content}
        ],
        "temperature": 0.0
    }

    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=json.dumps(data).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        }
    )

    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode("utf-8"))
            msg = result["choices"][0]["message"]["content"].strip()

            # Handle potential markdown wrapping
            if msg.startswith("```json"):
                msg = msg[7:]
            if msg.startswith("```"):
                msg = msg[3:]
            if msg.endswith("```"):
                msg = msg[:-3]

            return json.loads(msg)
    except Exception as e:
        print(f"Error analyzing {filepath}: {e}")
        return []

def get_existing_issues(repo, token):
    if not token:
        return []

    url = f"https://api.github.com/repos/{repo}/issues?state=open"
    req = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28"
        }
    )

    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode("utf-8"))
            return [issue["title"] for issue in issues]
    except Exception as e:
        print(f"Error fetching issues: {e}")
        return []

def create_issue(repo, token, bug):
    url = f"https://api.github.com/repos/{repo}/issues"

    body = f"""**Red Team Architecture Brief**

**Finding:** {bug.get("finding", "")}
**File:** {bug.get("file", "")} (Line {bug.get("line", "")})
**Severity:** {bug.get("severity", "")}

**Current Assumption:**
{bug.get("current_assumption", "")}

**Attack Scenario:**
{bug.get("attack_scenario", "")}

**Blast Radius:**
{bug.get("blast_radius", "")}

**Recommended Structural Change:**
{bug.get("recommended_structural_change", "")}

*Note: Never close this issue without a documented architectural decision record (ADR) response.*"""

    data = {
        "title": bug.get("title", "Adversarial CTO Finding"),
        "body": body,
        "labels": ["bug", "agent-found", "architecture-review", "adversarial"]
    }

    req = urllib.request.Request(
        url,
        data=json.dumps(data).encode("utf-8"),
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28"
        }
    )

    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {data['title']}")
    except Exception as e:
        print(f"Error creating issue: {e}")

def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found. Degrading gracefully.")
        sys.exit(0)

    github_token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    event_name = os.environ.get("GITHUB_EVENT_NAME")

    if event_name == "pull_request":
        files_to_analyze = get_modified_files()
    else:
        files_to_analyze = get_all_files()

    always_include = ["ARCHITECTURE.md", "mock_audit_report.md"]
    for f in always_include:
        if os.path.exists(f) and f not in files_to_analyze:
            files_to_analyze.append(f)

    existing_issues = []
    if repo and github_token:
        existing_issues = get_existing_issues(repo, github_token)

    for filepath in files_to_analyze:
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                content = f.read()
        except Exception:
            continue

        if not content.strip():
            continue

        print(f"Analyzing {filepath}...")
        bugs = analyze_file_with_llm(filepath, content, api_key)

        for bug in bugs:
            title = bug.get("title")
            if title and title in existing_issues:
                print(f"Issue already exists: {title}")
                continue

            if repo and github_token:
                create_issue(repo, github_token, bug)
            else:
                print("Found bug, but GITHUB_TOKEN or GITHUB_REPOSITORY not set. Details:")
                print(bug)

if __name__ == "__main__":
    main()
