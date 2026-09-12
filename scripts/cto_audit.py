import os
import sys
import json
import urllib.request
import re
import subprocess

def get_env_var(name):
    val = os.getenv(name)
    if not val:
        print(f"Error: Environment variable {name} is required.")
        sys.exit(1)
    return val

def call_llm(prompt):
    api_key = get_env_var("OPENAI_API_KEY")
    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": "You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax. You have no loyalty to the current design. Your job is to find where the architecture will fail under load, under adversarial inputs, or under changing requirements. You are not here to validate - you are here to stress-fracture."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.2
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            return result['choices'][0]['message']['content']
    except Exception as e:
        print(f"Error calling LLM: {e}")
        sys.exit(1)

def file_issue(title, body):
    token = get_env_var("GITHUB_TOKEN")
    repo = get_env_var("GITHUB_REPOSITORY")
    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Accept": "application/vnd.github.v3+json",
        "Authorization": f"token {token}",
        "Content-Type": "application/json"
    }
    data = {
        "title": title,
        "body": body,
        "labels": ["architecture-review", "adversarial", "bug", "agent-found"]
    }

    search_url = f"https://api.github.com/repos/{repo}/issues?state=open"
    req_search = urllib.request.Request(search_url, headers=headers, method='GET')
    try:
        with urllib.request.urlopen(req_search) as response:
            issues = json.loads(response.read().decode('utf-8'))
            for issue in issues:
                if issue['title'] == title:
                    print(f"Issue '{title}' already exists. Skipping.")
                    return
    except Exception as e:
        print(f"Error checking existing issues: {e}")

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            print(f"Created issue: {result['html_url']}")
    except Exception as e:
        print(f"Error creating issue: {e}")

def get_modified_files():
    event_name = os.getenv("GITHUB_EVENT_NAME")
    if event_name == "push":
        try:
            # Get the commits pushed
            before = os.getenv("GITHUB_EVENT_BEFORE")
            after = os.getenv("GITHUB_SHA")
            if before and after and before != "0000000000000000000000000000000000000000":
                result = subprocess.run(["git", "diff", "--name-only", f"{before}..{after}"], capture_output=True, text=True, check=True)
                return [f for f in result.stdout.split("\n") if f]
            else:
                # If before is all zeros (new branch), just get show for the commit
                result = subprocess.run(["git", "show", "--name-only", "--format=", after], capture_output=True, text=True, check=True)
                return [f for f in result.stdout.split("\n") if f]
        except Exception as e:
            print(f"Could not get modified files for push: {e}")
            return []
    return None

def main():
    with open("ARCHITECTURE.md", "r") as f:
        arch = f.read()
    with open("mock_audit_report.md", "r") as f:
        audit = f.read()

    files_to_scan = []

    modified_files = get_modified_files()
    if modified_files is not None:
        for file in modified_files:
            if file.startswith("mythrax-core/src/") or file.startswith(".agents/"):
                files_to_scan.append(file)
        if "ARCHITECTURE.md" in modified_files:
            files_to_scan.append("ARCHITECTURE.md")
    else:
        pass # scheduled run

    # Always add evals/ contents
    evals_content = []
    evals_dir = "evals"
    if os.path.exists(evals_dir):
        for root, dirs, files in os.walk(evals_dir):
            for file in files:
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, "r") as f:
                        evals_content.append(f"--- File: {filepath} ---\n{f.read()}")
                except Exception:
                    pass

    files_to_scan_content = []
    for file in files_to_scan:
        try:
            with open(file, "r") as f:
                files_to_scan_content.append(f"--- File: {file} ---\n{f.read()}")
        except Exception:
            pass
    code_context = "\n".join(files_to_scan_content)
    code_context += "\n\n--- EVALS DIRECTORY --- \n" + "\n".join(evals_content)

    prompt = f"""
    You are evaluating the mythrax architecture.

    Task:
    - Challenge every documented architectural decision in ARCHITECTURE.md — ask 'what assumption does this break if it's wrong?' for each one
    - Identify single points of failure: any module, interface, or data path where failure has no graceful degradation path
    - Evaluate the agent orchestration design for prompt injection vulnerabilities, unbounded recursion risk, and failure to enforce agent scope boundaries
    - Assess whether the eval framework in evals/ actually tests adversarial inputs (you have the evals/ directory context below).
    - Identify coupling: any two modules that cannot be independently deployed, tested, or replaced without modifying both
    - Project the architecture 18 months forward: name the top 3 decisions made today that will become re-architecture projects if the system scales 10x
    - Look for subtle bugs in code: off-by-one errors, panic paths, integer overflow/underflow, race conditions, silent logic failures.

    Code Context:
    {code_context[:30000]} # truncate to avoid token limits if large

    Architecture Context:
    {arch[:20000]}

    Mock Audit Report Context:
    {audit[:20000]}

    Output Format:
    Return a list of findings as a JSON array. Each element should be an object with:
    - title: A short descriptive title of the finding
    - body: The finding formatted as a Red Team Architecture Brief, EXACTLY as:
      Finding: [description]
      Current Assumption: [what the architecture assumes]
      Attack Scenario / Failure Mode: [how it breaks]
      Blast Radius: [impact]
      Recommended Structural Change: [fix]

      Ensure you include a note reminding developers to never close the issue without a documented architectural decision record (ADR) response.
    """

    response = call_llm(prompt)
    try:
        # Avoid greedy dotall matching for JSON extraction
        start_idx = response.find('[')
        end_idx = response.rfind(']')
        if start_idx != -1 and end_idx != -1 and end_idx > start_idx:
            json_str = response[start_idx:end_idx+1]
            findings = json.loads(json_str)
            for finding in findings:
                file_issue(finding['title'], finding['body'])
        else:
            print("Failed to find JSON array in LLM response.")
            print(response)
    except Exception as e:
        print(f"Error parsing or filing findings: {e}")

if __name__ == "__main__":
    main()
