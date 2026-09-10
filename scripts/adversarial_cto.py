import os
import sys
import json
import urllib.request
import urllib.error
import subprocess

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout.strip()

def main():
    openai_api_key = os.environ.get("OPENAI_API_KEY")
    if not openai_api_key:
        print("Notice: OPENAI_API_KEY not found. Gracefully degrading and exiting.")
        sys.exit(0)

    github_token = os.environ.get("GITHUB_TOKEN")
    if not github_token:
        print("Notice: GITHUB_TOKEN not found. Exiting.")
        sys.exit(0)

    repo = os.environ.get("GITHUB_REPOSITORY")
    if not repo:
        print("Notice: GITHUB_REPOSITORY not found. Exiting.")
        sys.exit(0)

    event_name = os.environ.get("GITHUB_EVENT_NAME", "schedule")

    # Determine files to analyze
    files_to_analyze = set()
    always_include = ["ARCHITECTURE.md", "mock_audit_report.md"]

    for f in always_include:
        if os.path.exists(f):
            files_to_analyze.add(f)

    if event_name == "schedule":
        # Full scan
        dirs = ["mythrax-core/src", ".agents"]
        for d in dirs:
            if os.path.exists(d):
                for root, _, filenames in os.walk(d):
                    for filename in filenames:
                        files_to_analyze.add(os.path.join(root, filename))
    else:
        # PR or push -> use git diff
        diff_cmd = "git diff --name-only HEAD~1 HEAD"
        if event_name == "pull_request":
            diff_cmd = "git diff --name-only origin/main...HEAD || git diff --name-only HEAD~1 HEAD"

        modified = run_cmd(diff_cmd).split("\n")
        for f in modified:
            if f and os.path.exists(f):
                if f.startswith("mythrax-core/src") or f.startswith(".agents"):
                    files_to_analyze.add(f)

    code_context = ""
    for f in files_to_analyze:
        try:
            with open(f, "r", encoding="utf-8") as file:
                content = file.read()
                code_context += f"\n--- {f} ---\n{content}\n"
        except Exception as e:
            pass

    if not code_context:
        print("No relevant files found. Exiting.")
        sys.exit(0)

    prompt = f"""
You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax.
You have no loyalty to the current design. Your job is to find where the architecture will fail under load,
under adversarial inputs, or under changing requirements. You are not here to validate — you are here to stress-fracture.

Evaluate the codebase with the following goals:
1. Challenge every documented architectural decision (ask "what assumption does this break if it's wrong?")
2. Identify single points of failure (module, interface, or data path where failure has no graceful degradation)
3. Evaluate the agent orchestration design for prompt injection vulnerabilities, unbounded recursion risk, and failure to enforce scope boundaries.
4. Assess whether the eval framework actually tests adversarial inputs.
5. Identify coupling (modules that cannot be independently deployed/tested/replaced).
6. Project the architecture 18 months forward (top 3 decisions that will become re-architecture projects if the system scales 10x).

Also analyze code for subtle bugs (off-by-one errors, panic paths, integer overflow/underflow, race conditions, silent logic failures, test coverage gaps).

Return the results as a strictly formatted JSON object with the following schema:
{{
  "findings": [
    {{
      "title": "Finding Title",
      "finding": "...",
      "current_assumption": "...",
      "attack_scenario": "...",
      "blast_radius": "...",
      "recommended_structural_change": "..."
    }}
  ]
}}

Codebase Context:
{code_context[:200000]}
"""

    # Call OpenAI API
    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=json.dumps({
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": "You are a specialized code analyzer that outputs only JSON."},
                {"role": "user", "content": prompt}
            ],
            "response_format": {"type": "json_object"}
        }).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {openai_api_key}"
        }
    )

    try:
        response = urllib.request.urlopen(req)
        result = json.loads(response.read().decode("utf-8"))
        content = json.loads(result["choices"][0]["message"]["content"])
        findings = content.get("findings", [])
    except Exception as e:
        print(f"Error calling LLM or parsing response: {e}")
        sys.exit(0)

    if not findings:
        print("No findings to report.")
        sys.exit(0)

    # Deduplicate and file issues
    issues_url = f"https://api.github.com/repos/{repo}/issues"

    req_issues = urllib.request.Request(
        f"{issues_url}?state=all&per_page=100",
        headers={
            "Authorization": f"token {github_token}",
            "Accept": "application/vnd.github.v3+json",
            "User-Agent": "Adversarial-CTO-Bot"
        }
    )

    existing_titles = set()
    try:
        res = urllib.request.urlopen(req_issues)
        issues = json.loads(res.read().decode("utf-8"))
        for issue in issues:
            existing_titles.add(issue.get("title", "").strip().lower())
    except Exception as e:
        print(f"Notice: Failed to fetch existing issues: {e}")

    for finding in findings:
        title = finding.get("title", "Adversarial CTO Finding")
        if title.strip().lower() in existing_titles:
            print(f"Skipping duplicate issue: {title}")
            continue

        body = f"""**Finding:** {finding.get('finding')}

**Current Assumption:** {finding.get('current_assumption')}

**Attack Scenario:** {finding.get('attack_scenario')}

**Blast Radius:** {finding.get('blast_radius')}

**Recommended Structural Change:** {finding.get('recommended_structural_change')}

*Note: Never close this issue without a documented architectural decision record (ADR) response.*
"""

        issue_data = {
            "title": title,
            "body": body,
            "labels": ["architecture-review", "adversarial"]
        }

        req_create = urllib.request.Request(
            issues_url,
            data=json.dumps(issue_data).encode("utf-8"),
            headers={
                "Authorization": f"token {github_token}",
                "Accept": "application/vnd.github.v3+json",
                "Content-Type": "application/json",
                "User-Agent": "Adversarial-CTO-Bot"
            }
        )

        try:
            res = urllib.request.urlopen(req_create)
            print(f"Created issue: {title}")
        except Exception as e:
            print(f"Failed to create issue {title}: {e}")

if __name__ == "__main__":
    main()
