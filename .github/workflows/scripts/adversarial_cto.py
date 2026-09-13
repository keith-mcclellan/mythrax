import os
import sys
import json
import urllib.request
import subprocess

def get_modified_files():
    event_name = os.environ.get("EVENT_NAME", "push")
    if event_name == "schedule":
        return None

    try:
        output = subprocess.check_output(["git", "diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"], text=True)
        return [line.strip() for line in output.splitlines() if line.strip()]
    except Exception as e:
        print(f"Warning: Could not determine modified files: {e}")
        return []

def read_file(path):
    try:
        with open(path, "r", encoding="utf-8") as f:
            return f.read()
    except Exception:
        return ""

def discover_files(directories):
    files = []
    for d in directories:
        if os.path.exists(d):
            for root, _, filenames in os.walk(d):
                for fname in filenames:
                    if fname.endswith(".rs") or fname.endswith(".md") or fname.endswith(".py"):
                        files.append(os.path.join(root, fname))
    return files

def call_llm(prompt):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("No OPENAI_API_KEY provided.")
        return "[]"

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": "You are an adversarial CTO. Return ONLY a JSON array of objects with keys: finding, assumption, attack_scenario, blast_radius, recommended_change. No markdown formatting, just raw JSON."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.2
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode("utf-8"))
            return result["choices"][0]["message"]["content"].strip()
    except Exception as e:
        print(f"LLM API call failed: {e}")
        return "[]"

def github_api_request(method, endpoint, data=None):
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print("Missing GITHUB_TOKEN or GITHUB_REPOSITORY")
        return None

    url = f"https://api.github.com/repos/{repo}{endpoint}"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "X-GitHub-Api-Version": "2022-11-28"
    }

    encoded_data = json.dumps(data).encode("utf-8") if data else None
    req = urllib.request.Request(url, data=encoded_data, headers=headers, method=method)

    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        print(f"GitHub API request failed: {e} - {e.read().decode('utf-8')}")
        return None
    except Exception as e:
        print(f"GitHub API request failed: {e}")
        return None

def get_existing_issues():
    issues = github_api_request("GET", "/issues?state=all&labels=adversarial")
    if not issues:
        return []
    return [issue["title"] for issue in issues if "title" in issue]

def main():
    print("Starting Adversarial CTO Analysis...")
    event_name = os.environ.get("EVENT_NAME", "push")

    core_context_files = ["ARCHITECTURE.md", "mock_audit_report.md"]
    context_content = ""
    for f in core_context_files:
        if os.path.exists(f):
            context_content += f"\n--- {f} ---\n{read_file(f)}\n"

    modified_files = get_modified_files()
    files_to_analyze = []

    if event_name == "schedule" or modified_files is None:
        files_to_analyze = discover_files(["mythrax-core/src", ".agents", "evals"])
    else:
        for mf in modified_files:
            if mf.startswith("mythrax-core/src/") or mf.startswith(".agents/") or mf.startswith("evals/") or mf == "ARCHITECTURE.md":
                if os.path.exists(mf):
                    files_to_analyze.append(mf)

    code_content = ""
    for f in files_to_analyze:
        code_content += f"\n--- {f} ---\n{read_file(f)}\n"

    if not code_content.strip() and event_name != "schedule":
        print("No relevant code files modified. Analyzing only context.")

    if len(code_content) > 60000:
        code_content = code_content[:60000] + "\n...[TRUNCATED]"

    prompt = f"""
You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax. You have no loyalty to the current design.

Your job is to find where the architecture will fail under load, under adversarial inputs, or under changing requirements. You are not here to validate — you are here to stress-fracture.

Based on the context and code provided, you must:
1. Challenge every documented architectural decision in ARCHITECTURE.md — ask "what assumption does this break if it's wrong?" for each one.
2. Identify single points of failure: any module, interface, or data path where failure has no graceful degradation path.
3. Evaluate the agent orchestration design for prompt injection vulnerabilities, unbounded recursion risk, and failure to enforce agent scope boundaries.
4. Assess whether the eval framework in evals/ actually tests adversarial inputs.
5. Identify coupling: any two modules that cannot be independently deployed, tested, or replaced without modifying both.
6. Project the architecture 18 months forward: name the top 3 decisions made today that will become re-architecture projects if the system scales 10x.
7. Also look for subtle bugs: off-by-one errors (esp. in agent context), panic paths (unwrap/expect with external data), integer overflow/underflow in scoring, race conditions, silent logic failures, and test coverage gaps. Provide file/line, minimal reproducible scenario, severity, and suggested fix.

Context Files:
{context_content}

Code Files:
{code_content}

Output format: Return ONLY a JSON array of objects. Do not include markdown formatting like ```json.
Each object must have exactly these string keys:
- finding (a short title)
- assumption (the current assumption being challenged)
- attack_scenario (how this fails or can be exploited)
- blast_radius (impact of the failure)
- recommended_change (recommended structural change or fix)
"""

    response_text = call_llm(prompt)
    if response_text.startswith("```json"):
        response_text = response_text[7:]
    if response_text.endswith("```"):
        response_text = response_text[:-3]

    try:
        findings = json.loads(response_text.strip())
    except json.JSONDecodeError as e:
        print(f"Failed to parse LLM response as JSON: {e}")
        return

    if not findings:
        print("No findings generated.")
        return

    existing_issues = get_existing_issues()

    for f in findings:
        title = f"Adversarial CTO Finding: {f.get('finding', 'Unknown Vulnerability')}"

        if any(title.lower() in ex.lower() for ex in existing_issues):
            print(f"Skipping duplicate issue: {title}")
            continue

        body = f"""### Red Team Architecture Brief

**Finding:** {f.get('finding', 'N/A')}

**Current Assumption:** {f.get('assumption', 'N/A')}

**Attack Scenario / Failure Mode:** {f.get('attack_scenario', 'N/A')}

**Blast Radius:** {f.get('blast_radius', 'N/A')}

**Recommended Structural Change:** {f.get('recommended_change', 'N/A')}

---
*Note: Never close this issue without a documented architectural decision record (ADR) response.*
"""
        issue_data = {
            "title": title,
            "body": body,
            "labels": ["architecture-review", "adversarial"]
        }

        res = github_api_request("POST", "/issues", issue_data)
        if res and "html_url" in res:
            print(f"Created issue: {res['html_url']}")
        else:
            print(f"Failed to create issue: {title}")

if __name__ == "__main__":
    main()
