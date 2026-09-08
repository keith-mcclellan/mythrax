import os
import sys
import json
import subprocess
import urllib.request
import urllib.error

def get_files_to_analyze():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "schedule")
    all_files = []
    if event_name == "pull_request":
        base_ref = os.environ.get("GITHUB_BASE_REF")
        if base_ref:
            subprocess.run(['git', 'fetch', 'origin', base_ref], check=False)
            res = subprocess.run(['git', 'diff', '--name-only', f'origin/{base_ref}'], stdout=subprocess.PIPE, text=True)
            all_files.extend([f for f in res.stdout.split() if f])
        else:
            res = subprocess.run(['git', 'diff', '--name-only', 'HEAD~1'], stdout=subprocess.PIPE, text=True)
            all_files.extend([f for f in res.stdout.split() if f])
    else:
        for root, dirs, files in os.walk('.'):
            if '.git' in root or 'target' in root:
                continue
            for f in files:
                if f.endswith(('.rs', '.py', '.ts', '.js')):
                    all_files.append(os.path.join(root, f))

    for f in ['ARCHITECTURE.md', 'mock_audit_report.md']:
        if os.path.exists(f) and f not in all_files:
            all_files.append(f)

    return [f for f in all_files if os.path.exists(f)]

def query_llm(prompt, api_key):
    url = "https://api.openai.com/v1/chat/completions"
    headers = {"Content-Type": "application/json", "Authorization": f"Bearer {api_key}"}
    data = {"model": "gpt-4o", "messages": [{"role": "user", "content": prompt}], "temperature": 0.2}
    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read())["choices"][0]["message"]["content"]
    except Exception as e:
        print(f"LLM request failed: {e}")
        return ""

def file_issue(repo, token, title, body):
    url = f"https://api.github.com/repos/{repo}/issues?state=all"
    req = urllib.request.Request(url, headers={"Authorization": f"token {token}", "Accept": "application/vnd.github.v3+json"})
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read())
            for issue in issues:
                if issue.get("title") == title:
                    print(f"Issue '{title}' already exists. Skipping.")
                    return
    except Exception as e:
        print(f"Failed to fetch issues: {e}")

    create_url = f"https://api.github.com/repos/{repo}/issues"
    if "**Note:** Do not close this issue without a documented architectural decision record (ADR) response." not in body:
        body += "\n\n**Note:** Do not close this issue without a documented architectural decision record (ADR) response."

    data = {"title": title, "body": body, "labels": ["bug", "agent-found", "architecture-review", "adversarial"]}
    req = urllib.request.Request(create_url, data=json.dumps(data).encode('utf-8'), headers={"Authorization": f"token {token}", "Accept": "application/vnd.github.v3+json", "Content-Type": "application/json"})
    try:
        urllib.request.urlopen(req)
    except Exception as e:
        print(f"Failed to create issue: {e}")

def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found. Degrading gracefully.")
        sys.exit(0)

    github_token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    files = get_files_to_analyze()

    prompt_template = """You are an Adversarial CTO. Analyze code for:
1. Off-by-one errors (loops, index, range)
2. Panic paths (.unwrap() / .expect())
3. Integer overflow/underflow
4. Race conditions
5. Silent logic failures
6. Test coverage gaps

Format as JSON array of objects:
[
  {{"finding": "...", "file": "...", "line": "...", "current_assumption": "...", "attack_scenario": "...", "blast_radius": "...", "recommended_structural_change": "...", "severity": "...", "reproducible_scenario": "...", "suggested_fix": "..."}}
]

File: {filepath}
Content: {content}
"""
    findings = []
    for filepath in files:
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                content = f.read()[:20000]
            if not content.strip():
                continue
            res = query_llm(prompt_template.format(filepath=filepath, content=content), api_key)
            if res:
                clean = res.strip()
                if clean.startswith("```json"): clean = clean[7:-3].strip()
                elif clean.startswith("```"): clean = clean[3:-3].strip()
                findings.extend(json.loads(clean))
        except Exception:
            continue

    for f in findings:
        title = f"Adversarial CTO: {f.get('finding')} in {f.get('file')}"
        body = f"### Red Team Architecture Brief\n\n**Finding:** {f.get('finding')}\n**File:** {f.get('file')}:{f.get('line')}\n**Severity:** {f.get('severity')}\n\n**Current Assumption:** {f.get('current_assumption')}\n**Attack Scenario:** {f.get('attack_scenario')}\n**Blast Radius:** {f.get('blast_radius')}\n**Recommended Structural Change:** {f.get('recommended_structural_change')}\n\n**Minimal Reproducible Scenario:**\n{f.get('reproducible_scenario')}\n\n**Suggested Fix:**\n{f.get('suggested_fix')}"
        if github_token and repo:
            file_issue(repo, github_token, title, body)
        else:
            print(f"Would file issue: {title}")

if __name__ == "__main__":
    main()
