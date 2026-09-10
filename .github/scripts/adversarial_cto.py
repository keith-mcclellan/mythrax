import os
import json
import subprocess
import urllib.request
import urllib.error

github_token = os.environ.get('GITHUB_TOKEN')
openai_api_key = os.environ.get('OPENAI_API_KEY')
github_repo = os.environ.get('GITHUB_REPOSITORY', 'local/repo')
event_name = os.environ.get('GITHUB_EVENT_NAME', 'manual')
base_ref = os.environ.get('GITHUB_BASE_REF')
sha = os.environ.get('GITHUB_SHA', 'HEAD')

MAX_FILE_SIZE = 100000

def run_cmd(cmd):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True).stdout.strip()

files_to_scan = []
always_include = ['ARCHITECTURE.md', 'mock_audit_report.md']

if event_name == 'schedule' or event_name == 'manual':
    out = run_cmd("git ls-tree -r --name-only HEAD")
    for f in out.splitlines():
        if f.startswith('mythrax-core/') or f.startswith('.agents/'):
            if f.endswith('.rs') or f.endswith('.md') or f.endswith('.json') or f.endswith('.py'):
                files_to_scan.append(f)
else:
    if base_ref:
        out = run_cmd(f"git diff --name-only origin/{base_ref} {sha}")
    else:
        out = run_cmd(f"git diff --name-only HEAD~1 HEAD")
    for f in out.splitlines():
        if os.path.exists(f):
            files_to_scan.append(f)
    for f in always_include:
        if os.path.exists(f) and f not in files_to_scan:
            files_to_scan.append(f)

existing_files = [f for f in files_to_scan if os.path.isfile(f)]

def llm_analyze(content, filename):
    prompt = f"""
You are a CTO who has been burned by production incidents traced to subtle bugs. You are methodical and assume the code is wrong until proven otherwise.
Analyze the following code from '{filename}' for logic bugs, edge case vulnerabilities, and correctness issues:
1. Off-by-one errors in any loop, index, or range operation (especially agent context windows)
2. Panic paths: any .unwrap() or .expect() triggered by realistic inputs/external data
3. Integer overflow or underflow risks in scoring, tokens, evals
4. Race conditions or incorrect assumptions about execution order
5. Logic branches where code silently succeeds but produces incorrect result
6. Test coverage gaps in public functions

Evaluate architectural decisions by challenging assumptions, identifying single points of failure without graceful degradation, assessing agent orchestration for vulnerabilities (e.g., prompt injection, unbounded recursion), verifying adversarial inputs in evals, flagging module coupling, and projecting 10x scaling limitations 18 months forward.

If you find a bug, format your response EXACTLY as a JSON array of objects. Keys:
- "title": A concise title
- "finding": Description of the bug
- "file": '{filename}'
- "line": Line number or context
- "reproducible_scenario": Minimal reproducible scenario
- "severity": CRITICAL, HIGH, MEDIUM, or LOW
- "suggested_fix": Suggested code fix
- "current_assumption": Current Assumption
- "attack_scenario": Attack Scenario
- "blast_radius": Blast Radius
- "recommended_structural_change": Recommended Structural Change

Return ONLY valid JSON array. If none, return [].

Code to analyze:
{content}
"""
    if not openai_api_key:
        print("No OPENAI_API_KEY, skipping LLM call")
        return []

    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        headers={
            "Authorization": f"Bearer {openai_api_key}",
            "Content-Type": "application/json"
        },
        data=json.dumps({
            "model": "gpt-4o",
            "messages": [{"role": "system", "content": prompt}]
        }).encode('utf-8')
    )
    try:
        with urllib.request.urlopen(req) as response:
            res = json.loads(response.read().decode('utf-8'))
            text = res['choices'][0]['message']['content'].strip()
            if text.startswith('```json'): text = text[7:]
            if text.startswith('```'): text = text[3:]
            if text.endswith('```'): text = text[:-3]
            return json.loads(text.strip())
    except Exception as e:
        print(f"Error calling LLM for {filename}: {e}")
        return []

def get_existing_issues():
    if not github_token or github_repo == 'local/repo':
        return []
    url = f"https://api.github.com/repos/{github_repo}/issues?state=all&labels=adversarial"
    req = urllib.request.Request(url, headers={
        "Authorization": f"token {github_token}",
        "Accept": "application/vnd.github.v3+json"
    })
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode('utf-8'))
            return [issue['title'] for issue in issues]
    except Exception as e:
        print(f"Error fetching issues: {e}")
        return []

def file_issue(bug):
    if not github_token or github_repo == 'local/repo':
        print(f"Would file issue: {bug.get('title', '')}")
        return

    title = f"Adversarial CTO: {bug.get('title', 'Bug')} in {bug.get('file', 'Unknown')}"

    body = f"""### Red Team Architecture Brief

**Finding**: {bug.get('finding', '')}
**File**: {bug.get('file', '')}
**Line**: {bug.get('line', '')}
**Severity**: {bug.get('severity', '')}

**Current Assumption**: {bug.get('current_assumption', '')}
**Attack Scenario**: {bug.get('attack_scenario', '')}
**Minimal Reproducible Scenario**: {bug.get('reproducible_scenario', '')}
**Blast Radius**: {bug.get('blast_radius', '')}

**Recommended Structural Change**: {bug.get('recommended_structural_change', '')}
**Suggested Fix**:
```rust
{bug.get('suggested_fix', '')}
```

---
*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
"""

    url = f"https://api.github.com/repos/{github_repo}/issues"
    req = urllib.request.Request(url, headers={
        "Authorization": f"token {github_token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }, data=json.dumps({
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found", "architecture-review", "adversarial"]
    }).encode('utf-8'))

    try:
        with urllib.request.urlopen(req) as response:
            res = json.loads(response.read().decode('utf-8'))
            print(f"Filed issue: {res['html_url']}")
    except Exception as e:
        print(f"Error filing issue: {e}")

existing_issues = get_existing_issues()

for f in existing_files:
    if os.path.getsize(f) > MAX_FILE_SIZE:
        continue

    try:
        with open(f, 'r') as file:
            content = file.read()
    except Exception as e:
        continue

    bugs = llm_analyze(content, f)
    if not isinstance(bugs, list):
        continue

    for bug in bugs:
        expected_title = f"Adversarial CTO: {bug.get('title', 'Bug')} in {bug.get('file', 'Unknown')}"
        if expected_title not in existing_issues:
            file_issue(bug)
            existing_issues.append(expected_title)
        else:
            print(f"Issue already exists: {expected_title}")

print("Adversarial CTO review complete.")
