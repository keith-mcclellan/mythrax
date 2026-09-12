import os
import sys
import subprocess
import json
import urllib.request
import re

def get_file_content(path):
    try:
        with open(path, 'r', encoding='utf-8') as f:
            return f.read()
    except Exception as e:
        return ""

def run_git_cmd(cmd):
    try:
        output = subprocess.check_output(cmd, shell=True, text=True)
        return [line.strip() for line in output.split('\n') if line.strip()]
    except Exception:
        return []

def main():
    event_name = os.environ.get('GITHUB_EVENT_NAME', 'schedule')
    base_ref = os.environ.get('GITHUB_BASE_REF', '')

    context_files = ['ARCHITECTURE.md', 'mock_audit_report.md']
    context_data = ""
    for cf in context_files:
        if os.path.exists(cf):
            context_data += f"\n--- {cf} ---\n{get_file_content(cf)}\n"

    target_files = []
    if event_name == 'pull_request' and base_ref:
        subprocess.run(f"git fetch origin {base_ref}", shell=True)
        mod_files = run_git_cmd(f"git diff --name-only origin/{base_ref}...HEAD")
        for f in mod_files:
            if f.startswith('mythrax-core/') or f.startswith('.agents/') or f.startswith('evals/'):
                if os.path.isfile(f):
                    target_files.append(f)
    else:
        all_f = run_git_cmd("git ls-tree -r HEAD --name-only")
        for f in all_f:
            if f.startswith('mythrax-core/src/') or f.startswith('.agents/') or f.startswith('evals/'):
                if f.endswith('.rs') or f.endswith('.py') or f.endswith('.md'):
                    if os.path.isfile(f):
                        target_files.append(f)

    if not target_files:
        print("No target files to analyze.")
        return

    test_funcs_cmd = "grep -rhE 'fn test_|async fn test_' mythrax-core/tests evals 2>/dev/null || true"
    test_funcs = subprocess.getoutput(test_funcs_cmd)

    from openai import OpenAI
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        print("Missing OPENAI_API_KEY")
        return

    client = OpenAI(api_key=api_key)

    all_findings = []
    batch_size = 5
    for i in range(0, len(target_files), batch_size):
        batch = target_files[i:i+batch_size]
        batch_data = ""
        for tf in batch:
            batch_data += f"\n--- {tf} ---\n{get_file_content(tf)}\n"

        prompt = f"""You are an Adversarial CTO who has been burned by production incidents traced to subtle bugs that code review missed. You are methodical and assume the code is wrong until proven otherwise.

Your task is to analyze the following files for logic bugs, edge case vulnerabilities, and correctness issues.
Pay special attention to:
- Off-by-one errors in any loop, index, or range operation (especially in AI agent scaffolding and context window management in .agents/)
- Panic paths: any `.unwrap()` or `.expect()` that could be triggered by realistic runtime inputs, especially those processing external data or LLM responses
- Integer overflow or underflow risks in any arithmetic involving agent scoring, token counting, or eval metrics
- Race conditions or incorrect assumptions about execution order in any async or concurrent code paths
- Logic branches where the code silently succeeds but produces an incorrect result (false positives worse than panics)
- Test coverage gaps — identify any public functions in `mythrax-core` with no corresponding test in the eval or test suite.

Additionally, evaluate architectural decisions by challenging assumptions, identifying single points of failure without graceful degradation, assessing agent orchestration for vulnerabilities (e.g., prompt injection, unbounded recursion), verifying adversarial inputs in evals, flagging module coupling, and projecting 10x scaling limitations 18 months forward.

Context Files:
{context_data}

Existing Test Functions (for coverage analysis):
{test_funcs}

Files to analyze in this batch:
{batch_data}

Return your findings as a JSON object containing a single key "findings", which is an array of objects. Each object in the array must have the following string keys:
- "title": A short title of the bug or finding
- "severity": High, Medium, or Low
- "file": The file path
- "line": The line number or range
- "minimal_reproducible_scenario": A brief description of how to reproduce it
- "suggested_fix": A suggested fix
- "finding": The finding description (for the Red Team Architecture Brief)
- "current_assumption": The current assumption being challenged
- "attack_scenario": The attack scenario
- "blast_radius": The blast radius
- "recommended_structural_change": The recommended structural change

If no bugs or issues are found, return {{"findings": []}}.
Respond ONLY with valid JSON.
"""
        try:
            response = client.chat.completions.create(
                model="gpt-4o",
                messages=[{"role": "user", "content": prompt}],
                temperature=0.2,
                response_format={"type": "json_object"}
            )
            data = json.loads(response.choices[0].message.content)
            all_findings.extend(data.get("findings", []))
        except Exception as e:
            print(f"Error processing batch {batch}: {e}")

    if not all_findings:
        print("No findings to report.")
        return

    github_token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    if not github_token or not repo:
        print("Missing GITHUB_TOKEN or GITHUB_REPOSITORY. Cannot file issues.")
        return

    headers = {
        "Authorization": f"token {github_token}",
        "Accept": "application/vnd.github.v3+json"
    }

    existing_issues = []
    page = 1
    while True:
        url = f"https://api.github.com/repos/{repo}/issues?state=all&labels=agent-found&per_page=100&page={page}"
        req = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(req) as response:
                issues = json.loads(response.read().decode())
                if not issues:
                    break
                existing_issues.extend([iss['title'] for iss in issues])
                page += 1
        except Exception as e:
            print(f"Failed to fetch existing issues: {e}")
            break

    for finding in all_findings:
        issue_title = f"[Adversarial CTO] {finding.get('title', 'Bug Found')}"

        if issue_title in existing_issues:
            print(f"Issue already exists: {issue_title}")
            continue

        body = f"""## Red Team Architecture Brief

**Finding:** {finding.get('finding')}
**Current Assumption:** {finding.get('current_assumption')}
**Attack Scenario:** {finding.get('attack_scenario')}
**Blast Radius:** {finding.get('blast_radius')}
**Recommended Structural Change:** {finding.get('recommended_structural_change')}

---

### Bug Details
- **File:** `{finding.get('file')}`
- **Line:** `{finding.get('line')}`
- **Severity:** {finding.get('severity')}

**Minimal Reproducible Scenario:**
{finding.get('minimal_reproducible_scenario')}

**Suggested Fix:**
{finding.get('suggested_fix')}

---
*Note: Never close this issue without a documented architectural decision record (ADR) response.*
"""
        payload = {
            "title": issue_title,
            "body": body,
            "labels": ["bug", "agent-found"]
        }

        post_url = f"https://api.github.com/repos/{repo}/issues"
        req = urllib.request.Request(post_url, data=json.dumps(payload).encode('utf-8'), headers=headers, method='POST')
        try:
            with urllib.request.urlopen(req) as response:
                print(f"Created issue: {issue_title}")
        except Exception as e:
            print(f"Failed to create issue {issue_title}: {e}")

if __name__ == "__main__":
    main()
