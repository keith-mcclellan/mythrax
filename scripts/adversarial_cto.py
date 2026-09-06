import os
import sys
import json
import urllib.request
import subprocess

def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found. Exiting gracefully.")
        sys.exit(0)

    github_token = os.environ.get("GITHUB_TOKEN")
    if not github_token:
        print("GITHUB_TOKEN not found. Exiting gracefully.")
        sys.exit(0)

    repo = os.environ.get("GITHUB_REPOSITORY")
    if not repo:
        print("GITHUB_REPOSITORY not found.")
        sys.exit(1)

    event_name = os.environ.get("GITHUB_EVENT_NAME", "")

    target_paths = ["ARCHITECTURE.md", "mock_audit_report.md", "mythrax-core/src", ".agents", "evals"]
    files_to_scan = set()

    # Always include core architectural context
    for core_file in ["ARCHITECTURE.md", "mock_audit_report.md"]:
        if os.path.exists(core_file):
            files_to_scan.add(core_file)

    if event_name == "pull_request":
        base_ref = os.environ.get("GITHUB_BASE_REF", "main")
        try:
            subprocess.run(["git", "fetch", "origin", base_ref], check=True)
            output = subprocess.check_output(["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"], text=True)
            changed_files = output.splitlines()
            for f in changed_files:
                for tp in target_paths:
                    if f.startswith(tp):
                        files_to_scan.add(f)
        except Exception as e:
            print(f"Error finding changed files: {e}")
    else:
        for tp in target_paths:
            if os.path.exists(tp):
                if os.path.isfile(tp):
                    files_to_scan.add(tp)
                else:
                    for root, dirs, files in os.walk(tp):
                        for file in files:
                            if file.endswith(".rs") or file.endswith(".md") or file.endswith(".json"):
                                files_to_scan.add(os.path.join(root, file))

    if not files_to_scan:
        print("No files to scan.")
        sys.exit(0)

    prompt = """You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax. You have no loyalty to the current design. Your job is to find where the architecture will fail under load, under adversarial inputs, or under changing requirements.
Review the following code and documentation.
- Challenge every documented architectural decision — ask “what assumption does this break if it’s wrong?” for each one.
- Identify single points of failure: any module, interface, or data path where failure has no graceful degradation path.
- Evaluate the agent orchestration design for prompt injection vulnerabilities, unbounded recursion risk, and failure to enforce agent scope boundaries.
- Assess whether the eval framework actually tests adversarial inputs.
- Identify coupling: any two modules that cannot be independently deployed, tested, or replaced without modifying both.
- Project the architecture 18 months forward: name the top 3 decisions made today that will become re-architecture projects if the system scales 10x.

Output as a Red Team Architecture Brief formatted strictly as a JSON object containing a single key "findings", which is an array of objects. Each object must have the following keys:
- "title": A short, clear title for the finding.
- "assumption": Current Assumption
- "scenario": Attack Scenario
- "blast_radius": Blast Radius
- "recommendation": Recommended Structural Change

Code/Docs:
"""

    code_content = ""
    for f in files_to_scan:
        if os.path.exists(f):
             try:
                 with open(f, 'r', encoding='utf-8') as file:
                      content = file.read()
                      if len(content) > 10000:
                          content = content[:10000] + "... (truncated)"
                      code_content += f"\n--- {f} ---\n{content}\n"
             except Exception as e:
                 print(f"Error reading {f}: {e}")

    prompt += code_content

    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    data = {
        "model": "gpt-4o-mini",
        "response_format": { "type": "json_object" },
        "messages": [
            {"role": "system", "content": "You are an adversarial CTO returning findings in JSON format."},
            {"role": "user", "content": prompt}
        ]
    }

    req = urllib.request.Request("https://api.openai.com/v1/chat/completions", data=json.dumps(data).encode("utf-8"), headers=headers, method="POST")

    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode("utf-8"))
            content = result["choices"][0]["message"]["content"]
            findings_data = json.loads(content)
            findings = findings_data.get("findings", [])
    except Exception as e:
        print(f"Error calling LLM: {e}")
        sys.exit(0) # fail gracefully

    for finding in findings:
        title = f"Adversarial CTO: {finding.get('title', 'Architecture Finding')}"
        body = f"### Current Assumption\n{finding.get('assumption', '')}\n\n### Attack Scenario\n{finding.get('scenario', '')}\n\n### Blast Radius\n{finding.get('blast_radius', '')}\n\n### Recommended Structural Change\n{finding.get('recommendation', '')}\n\n---\n**Note:** Never close these issues without a documented architectural decision record (ADR) response."

        issue_data = {
            "title": title,
            "body": body,
            "labels": ["architecture-review", "adversarial"]
        }

        issue_req = urllib.request.Request(
            f"https://api.github.com/repos/{repo}/issues",
            data=json.dumps(issue_data).encode("utf-8"),
            headers={
                "Authorization": f"token {github_token}",
                "Accept": "application/vnd.github.v3+json"
            },
            method="POST"
        )

        try:
             with urllib.request.urlopen(issue_req) as issue_res:
                  print(f"Created issue: {title}")
        except Exception as e:
             print(f"Failed to create issue: {e}")

if __name__ == "__main__":
    main()
