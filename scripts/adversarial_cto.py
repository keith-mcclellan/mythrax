import os
import json
import urllib.request
import urllib.parse
import subprocess
import sys

def get_modified_files(base_sha, head_sha):
    cmd = ["git", "diff", "--name-only", f"{base_sha}...{head_sha}"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        return []
    files = result.stdout.splitlines()
    return [f for f in files if f.startswith("mythrax-core/") or f.startswith("evals/") or f.startswith(".agents/")]

def get_all_files():
    cmd = ["git", "ls-tree", "-r", "--name-only", "HEAD"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        return []
    files = result.stdout.splitlines()
    return [f for f in files if (f.startswith("mythrax-core/") or f.startswith("evals/") or f.startswith(".agents/")) and f.endswith((".rs", ".py", ".md"))]

def read_file(path):
    if not os.path.exists(path):
        return ""
    with open(path, "r", encoding="utf-8") as f:
        return f.read()

def ask_llm(prompt):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not set")
        return "[]"
    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": "You are an Adversarial CTO. You output ONLY valid JSON format containing a list of findings. Do not output markdown code blocks. Just the raw JSON string."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.2
    }
    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=120) as response:
            result = json.loads(response.read().decode("utf-8"))
            return result["choices"][0]["message"]["content"].strip()
    except Exception as e:
        print(f"LLM request failed: {e}")
        return "[]"

def github_api_request(method, endpoint, data=None):
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print(f"Missing GitHub token or repo. Would call {method} {endpoint}")
        return None
    url = f"https://api.github.com/repos/{repo}/{endpoint}"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "User-Agent": "Adversarial-CTO-Script"
    }
    req_data = json.dumps(data).encode("utf-8") if data else None
    req = urllib.request.Request(url, data=req_data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        print(f"GitHub API HTTPError {method} {endpoint}: {e.code} {e.reason}")
        return None
    except Exception as e:
        print(f"GitHub API {method} {endpoint} failed: {e}")
        return None

def get_existing_issues():
    issues = github_api_request("GET", "issues?state=all&labels=bug,agent-found")
    if not issues:
        return []
    return [issue["title"] for issue in issues]

def create_issue(finding):
    title = f"Adversarial CTO Finding: {finding.get('file', 'Unknown')} - {finding.get('severity', 'Unknown')} - {finding.get('title', 'Unknown')}"
    existing = get_existing_issues()
    if title in existing:
        print(f"Issue already exists: {title}")
        return
    body = f"## Red Team Architecture Brief\n\n**Finding:** {finding.get('title', 'Unknown')}\n**Current Assumption:** {finding.get('current_assumption', 'N/A')}\n**Attack Scenario:** {finding.get('attack_scenario', 'N/A')}\n**Blast Radius:** {finding.get('blast_radius', 'N/A')}\n**Recommended Structural Change:** {finding.get('recommended_structural_change', 'N/A')}\n\n---\n**File and Line Number:** `{finding.get('file', 'Unknown')}:{finding.get('line', 'Unknown')}`\n**Severity:** {finding.get('severity', 'Unknown')}\n**Minimal Reproducible Scenario:**\n{finding.get('minimal_reproducible_scenario', 'N/A')}\n\n**Suggested Fix:**\n{finding.get('suggested_fix', 'N/A')}\n\n> **Note to Developers:** Never close this issue without a documented architectural decision record (ADR) response.\n"
    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found", "architecture-review", "adversarial"]
    }
    github_api_request("POST", "issues", data)
    print(f"Created issue: {title}")

def main():
    event_name = os.environ.get("EVENT_NAME", "schedule")
    if event_name == "pull_request":
        base_sha = os.environ.get("BASE_SHA")
        head_sha = os.environ.get("HEAD_SHA")
        if not base_sha or not head_sha:
            print("Missing SHA for PR event")
            sys.exit(1)
        files_to_scan = get_modified_files(base_sha, head_sha)
    else:
        files_to_scan = get_all_files()

    if not files_to_scan:
        print("No files to scan")
        return

    # Check for core changes that require a full run
    core_changed = any(f == "ARCHITECTURE.md" or f.startswith("mythrax-core/src/") or f.startswith(".agents/") for f in files_to_scan)
    if event_name == "pull_request" and core_changed:
        # Full scan is required if core changed
        files_to_scan = get_all_files()

    context_files = ["ARCHITECTURE.md", "mock_audit_report.md"]
    context_data = ""
    for cf in context_files:
        if os.path.exists(cf):
            context_data += f"\n--- {cf} ---\n{read_file(cf)}"

    chunk_size = 10
    for i in range(0, len(files_to_scan), chunk_size):
        chunk = files_to_scan[i:i+chunk_size]
        code_data = ""
        for f in chunk:
            code_data += f"\n--- {f} ---\n{read_file(f)}"

        prompt = f"You are the Adversarial CTO. Review the following code for subtle bugs, edge case vulnerabilities, correctness issues, and architectural flaws.\nFocus on:\n1. Off-by-one errors in loops, indices, range operations (especially agent scaffolding/context window management).\n2. Panic paths (.unwrap() or .expect() with realistic external inputs/LLM responses).\n3. Integer overflow/underflow in scoring, token counting, or evals.\n4. Race conditions/incorrect async execution assumptions.\n5. Logic branches where code silently succeeds but is incorrect.\n6. Test coverage gaps (public functions in mythrax-core with no tests).\n7. Architectural decisions (single points of failure, agent orchestration vulns, prompt injection, module coupling, 10x scaling limitations).\n\nCode context:\n{context_data}\n\nFiles to analyze:\n{code_data}\n\nReturn ONLY a JSON array of objects, with each object having exactly these keys:\n- \"title\": A short description of the finding\n- \"file\": The file path\n- \"line\": The approximate line number\n- \"severity\": Critical, High, Medium, or Low\n- \"current_assumption\": What the code currently assumes\n- \"attack_scenario\": How this can be exploited or broken\n- \"blast_radius\": The impact if this fails\n- \"recommended_structural_change\": Architectural fix\n- \"minimal_reproducible_scenario\": Steps or code to reproduce\n- \"suggested_fix\": Specific code change\n\nIf no issues are found, return `[]`.\nEnsure your output is strictly valid JSON with no markdown formatting or extra text.\n"
        response_text = ask_llm(prompt)
        if response_text.startswith("```json"):
            response_text = response_text[7:]
        if response_text.startswith("```"):
            response_text = response_text[3:]
        if response_text.endswith("```"):
            response_text = response_text[:-3]
        response_text = response_text.strip()

        try:
            findings = json.loads(response_text)
            if not isinstance(findings, list):
                print(f"Unexpected JSON format: {findings}")
                continue
            for finding in findings:
                create_issue(finding)
        except json.JSONDecodeError as e:
            print(f"Failed to parse LLM response: {e}\nResponse was:\n{response_text}")

if __name__ == "__main__":
    main()
