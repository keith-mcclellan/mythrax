import os
import sys
import json
import urllib.request
import urllib.error
import subprocess

def get_modified_files():
    try:
        base_ref = os.environ.get("GITHUB_BASE_REF", "main")
        subprocess.run(["git", "fetch", "origin", base_ref], check=True)
        result = subprocess.run(
            ["git", "diff", "--name-only", f"origin/{base_ref}"],
            capture_output=True, text=True, check=True
        )
        files = result.stdout.strip().split('\n')
        return [f for f in files if f and os.path.exists(f)]
    except Exception as e:
        print(f"Error getting modified files: {e}")
        return []

def get_all_files():
    files = []
    for root, dirs, filenames in os.walk('mythrax-core'):
        for filename in filenames:
            if filename.endswith('.rs'):
                files.append(os.path.join(root, filename))
    for root, dirs, filenames in os.walk('.agents'):
        for filename in filenames:
            if filename.endswith('.md') or filename.endswith('.py'):
                files.append(os.path.join(root, filename))
    return files

def chunk_files(files, max_chars=100000):
    chunks = []
    current_chunk = []
    current_size = 0
    for f in files:
        try:
            with open(f, 'r', encoding='utf-8') as file:
                content = file.read()
                size = len(content)
                if current_size + size > max_chars and current_chunk:
                    chunks.append(current_chunk)
                    current_chunk = [(f, content)]
                    current_size = size
                else:
                    current_chunk.append((f, content))
                    current_size += size
        except Exception as e:
            print(f"Failed to read {f}: {e}")
    if current_chunk:
        chunks.append(current_chunk)
    return chunks

def analyze_chunk(chunk, api_key):
    code_content = ""
    for f, content in chunk:
        code_content += f"\n--- {f} ---\n{content}\n"

    prompt = f"""
You are an Adversarial CTO who has been burned by production incidents traced to subtle bugs that code review missed. You are methodical and assume the code is wrong until proven otherwise.
Analyze the following code for logic bugs, edge case vulnerabilities, and correctness issues:
- Off-by-one errors in any loop, index, or range operation (pay special attention to AI agent scaffolding and context window management)
- Panic paths: any .unwrap() or .expect() that could be triggered by realistic runtime inputs, especially processing external data or LLM responses
- Integer overflow or underflow risks in any arithmetic involving agent scoring, token counting, or eval metrics
- Race conditions or incorrect assumptions about execution order in any async or concurrent code paths
- Logic branches where the code silently succeeds but produces an incorrect result (false positives worse than panics)
- Test coverage gaps: identify any public functions in mythrax-core with no corresponding test in the eval or test suite

Format your output strictly as a JSON array of objects. Each object must have:
"title": short string,
"file": string (file path),
"line": string or number,
"severity": "Critical", "High", "Medium", or "Low",
"current_assumption": string,
"attack_scenario": string (minimal reproducible scenario),
"blast_radius": string,
"recommended_change": string (suggested fix)

Return ONLY a valid JSON array.

Code:
{code_content}
"""

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    data = {
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.2
    }

    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
        with urllib.request.urlopen(req) as response:
            res_body = response.read().decode('utf-8')
            res_json = json.loads(res_body)
            content = res_json['choices'][0]['message']['content'].strip()

            if content.startswith("```json"):
                content = content[7:-3].strip()
            elif content.startswith("```"):
                content = content[3:-3].strip()

            if not content:
                return []

            return json.loads(content)
    except urllib.error.HTTPError as e:
        print(f"LLM API HTTP error: {e.code} {e.read().decode('utf-8')}")
        return []
    except Exception as e:
        print(f"LLM API error: {e}")
        return []

def analyze_with_llm(files):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found. Degrading gracefully.")
        return []

    if not files:
        return []

    chunks = chunk_files(files, max_chars=100000)
    all_findings = []
    for i, chunk in enumerate(chunks):
        print(f"Analyzing chunk {i+1}/{len(chunks)} with {len(chunk)} files...")
        findings = analyze_chunk(chunk, api_key)
        all_findings.extend(findings)

    return all_findings

def get_existing_issues(repo, token):
    url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json",
        "User-Agent": "Adversarial-CTO-Agent"
    }
    try:
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode('utf-8'))
            return [issue['title'] for issue in issues if 'title' in issue]
    except Exception as e:
        print(f"Failed to fetch issues: {e}")
        return []

def create_issue(repo, token, finding):
    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json",
        "User-Agent": "Adversarial-CTO-Agent"
    }

    body = f"""## Red Team Architecture Brief

**Finding:** {finding.get('title')} (File: {finding.get('file')}, Line: {finding.get('line')}, Severity: {finding.get('severity')})
**Current Assumption:** {finding.get('current_assumption')}
**Attack Scenario:** {finding.get('attack_scenario')}
**Blast Radius:** {finding.get('blast_radius')}
**Recommended Structural Change:** {finding.get('recommended_change')}

---
*Note: Never close this issue without a documented architectural decision record (ADR) response.*
"""

    data = {
        "title": f"Adversarial CTO: {finding.get('title')}",
        "body": body,
        "labels": ["bug", "agent-found", "architecture-review", "adversarial"]
    }

    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
        with urllib.request.urlopen(req) as response:
            res = json.loads(response.read().decode('utf-8'))
            print(f"Created issue: {res.get('html_url')}")
    except Exception as e:
        print(f"Failed to create issue: {e}")

def local_create_issue(finding):
    os.makedirs("issues", exist_ok=True)
    safe_title = str(finding.get('title', 'issue')).replace(' ', '_').replace('/', '_').replace('\\', '_')
    file_path = f"issues/{safe_title}.md"
    body = f"""## Red Team Architecture Brief

**Finding:** {finding.get('title')} (File: {finding.get('file')}, Line: {finding.get('line')}, Severity: {finding.get('severity')})
**Current Assumption:** {finding.get('current_assumption')}
**Attack Scenario:** {finding.get('attack_scenario')}
**Blast Radius:** {finding.get('blast_radius')}
**Recommended Structural Change:** {finding.get('recommended_change')}

---
*Note: Never close this issue without a documented architectural decision record (ADR) response.*
"""
    try:
        with open(file_path, "w", encoding='utf-8') as f:
            f.write(body)
        print(f"Created local issue: {file_path}")
    except Exception as e:
        print(f"Failed to create local issue: {e}")

def main():
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")

    files_to_analyze = []

    if event_name == "pull_request":
        files_to_analyze = get_modified_files()
    else:
        # Schedule or local run
        files_to_analyze = get_all_files()

    # Ensure core context files are always included
    for core_file in ["ARCHITECTURE.md", "mock_audit_report.md"]:
        if core_file not in files_to_analyze and os.path.exists(core_file):
            files_to_analyze.append(core_file)

    findings = analyze_with_llm(files_to_analyze)

    existing_titles = []
    if repo and token:
        existing_titles = get_existing_issues(repo, token)

    for finding in findings:
        title = f"Adversarial CTO: {finding.get('title')}"
        if title in existing_titles:
            print(f"Issue already exists: {title}")
            continue

        if repo and token:
            create_issue(repo, token, finding)
        else:
            local_create_issue(finding)

if __name__ == "__main__":
    main()
