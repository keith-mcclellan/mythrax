import os
import sys
import json
import urllib.request
import urllib.error
import subprocess

def get_modified_files():
    base_sha = os.environ.get('BASE_SHA')
    head_sha = os.environ.get('HEAD_SHA')
    if not base_sha or not head_sha:
        return []
    try:
        result = subprocess.run(['git', 'diff', '--name-only', base_sha, head_sha], capture_output=True, text=True, check=True)
        return [f for f in result.stdout.split('\n') if f]
    except subprocess.CalledProcessError:
        return []

def get_all_files():
    try:
        result = subprocess.run(['git', 'ls-files'], capture_output=True, text=True, check=True)
        return [f for f in result.stdout.split('\n') if f]
    except subprocess.CalledProcessError:
        return []

def read_file_content(filepath):
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            return f.read()
    except Exception:
        return ""

def chunk_content(content, max_length=100000):
    return [content[i:i+max_length] for i in range(0, len(content), max_length)]

def call_openai(prompt):
    api_key = os.environ.get('OPENAI_API_KEY')
    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    data = {
        "model": "gpt-4-turbo-preview",
        "messages": [
            {"role": "system", "content": "You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax. You have no loyalty to the current design. You are not here to validate - you are here to stress-fracture. Return a valid JSON object containing a single key 'findings' which holds a JSON array of results. Each result must have 'type' (either 'architecture' or 'bug'), 'title', 'finding', 'current_assumption' (for architecture), 'attack_scenario' (for architecture), 'blast_radius' (for architecture), 'recommended_structural_change' (for architecture), 'file_line' (for bugs), 'scenario' (for bugs), 'severity' (for bugs), and 'suggested_fix' (for bugs)."},
            {"role": "user", "content": prompt}
        ],
        "response_format": {"type": "json_object"}
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            return json.loads(result['choices'][0]['message']['content'])
    except Exception as e:
        print(f"Error calling OpenAI: {e}")
        return {"findings": []}

def create_github_issue(title, body, labels):
    token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    if not token or not repo:
        print("Missing GITHUB_TOKEN or GITHUB_REPOSITORY")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    data = {
        "title": title,
        "body": body,
        "labels": labels
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {title}")
    except Exception as e:
        print(f"Error creating issue: {e}")

def main():
    if not os.environ.get('OPENAI_API_KEY'):
        print("OPENAI_API_KEY not found. Exiting gracefully.")
        sys.exit(0)

    event_name = os.environ.get('EVENT_NAME', 'schedule')

    files_to_analyze = ['ARCHITECTURE.md', 'mock_audit_report.md']

    if event_name in ['push', 'pull_request']:
        files_to_analyze.extend(get_modified_files())
    else:
        files_to_analyze.extend(get_all_files())

    files_to_analyze = list(set(f for f in files_to_analyze if os.path.exists(f)))

    combined_content = ""
    for f in files_to_analyze:
        content = read_file_content(f)
        if content:
            combined_content += f"\n\n--- File: {f} ---\n{content}"

    chunks = chunk_content(combined_content)

    all_findings = []
    for chunk in chunks:
        prompt = f"Analyze the following codebase context:\n\n{chunk}\n\nTask: Challenge architectural decisions, identify single points of failure, evaluate agent orchestration design, assess eval framework, identify coupling, and project 18 months forward. Also find subtle bugs."
        result = call_openai(prompt)
        if 'findings' in result:
            all_findings.extend(result['findings'])

    for finding in all_findings:
        if finding.get('type') == 'architecture':
            title = f"Adversarial CTO: {finding.get('title')}"
            body = f"**Finding:** {finding.get('finding')}\n\n**Current Assumption:** {finding.get('current_assumption')}\n\n**Attack Scenario:** {finding.get('attack_scenario')}\n\n**Blast Radius:** {finding.get('blast_radius')}\n\n**Recommended Structural Change:** {finding.get('recommended_structural_change')}\n\n*Never close this issue without a documented architectural decision record (ADR) response.*"
            labels = ['architecture-review', 'adversarial']
        else:
            title = f"Adversarial CTO: Bug in {finding.get('file_line')}"
            body = f"**Finding:** {finding.get('finding')}\n\n**File/Line:** {finding.get('file_line')}\n\n**Scenario:** {finding.get('scenario')}\n\n**Severity:** {finding.get('severity')}\n\n**Suggested Fix:** {finding.get('suggested_fix')}"
            labels = ['bug', 'agent-found']

        create_github_issue(title, body, labels)

if __name__ == '__main__':
    main()
