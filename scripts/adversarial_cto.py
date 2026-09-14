import os
import json
import urllib.request
import subprocess

def get_modified_files():
    try:
        result = subprocess.run(['git', 'diff', '--name-only', 'HEAD^1', 'HEAD'], capture_output=True, text=True)
        if result.returncode == 0 and result.stdout:
            return [line.strip() for line in result.stdout.split('\n') if line.strip()]
    except Exception as e:
        print(f"Error getting modified files: {e}")
    return []

def read_file(path):
    if os.path.exists(path):
        with open(path, 'r', encoding='utf-8') as f:
            return f.read()
    return ""

def call_openai(prompt):
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        print("OPENAI_API_KEY not set")
        return ""

    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=json.dumps({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": "You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax. You have no loyalty to the current design. You have read ARCHITECTURE.md and mock_audit_report.md in full, and your job is to find where the architecture will fail under load, under adversarial inputs, or under changing requirements. You are not here to validate — you are here to stress-fracture."
                },
                {"role": "user", "content": prompt}
            ]
        }).encode('utf-8'),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        }
    )

    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            return result['choices'][0]['message']['content']
    except Exception as e:
        print(f"Error calling OpenAI: {e}")
        return ""

def get_existing_issues(repo, token):
    issues = []
    page = 1
    while True:
        url = f"https://api.github.com/repos/{repo}/issues?state=all&labels=adversarial&per_page=100&page={page}"
        req = urllib.request.Request(url, headers={
            "Authorization": f"token {token}",
            "Accept": "application/vnd.github.v3+json"
        })
        try:
            with urllib.request.urlopen(req) as response:
                batch = json.loads(response.read().decode('utf-8'))
                if not batch:
                    break
                issues.extend(batch)
                page += 1
        except Exception as e:
            print(f"Error fetching existing issues: {e}")
            break
    return [issue['title'] for issue in issues]

def create_issue(repo, token, title, body, labels):
    url = f"https://api.github.com/repos/{repo}/issues"
    req = urllib.request.Request(
        url,
        data=json.dumps({
            "title": title,
            "body": body,
            "labels": labels
        }).encode('utf-8'),
        headers={
            "Authorization": f"token {token}",
            "Accept": "application/vnd.github.v3+json",
            "Content-Type": "application/json"
        },
        method="POST"
    )
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {title}")
    except Exception as e:
        print(f"Error creating issue: {e}")

def main():
    event_name = os.environ.get('GITHUB_EVENT_NAME', 'schedule')
    repo = os.environ.get('GITHUB_REPOSITORY')
    token = os.environ.get('GITHUB_TOKEN')

    files_to_analyze = []
    if event_name == 'schedule':
        for d in ['mythrax-core/src', '.agents', 'evals']:
            if os.path.exists(d):
                for root, _, files in os.walk(d):
                    for f in files:
                        if f.endswith('.rs') or f.endswith('.md') or f.endswith('.py') or f.endswith('.json'):
                            files_to_analyze.append(os.path.join(root, f))
    else:
        modified_files = get_modified_files()
        for f in modified_files:
            if f.startswith('mythrax-core/src/') or f.startswith('.agents/') or f.startswith('evals/'):
                files_to_analyze.append(f)

    for core_file in ['ARCHITECTURE.md', 'mock_audit_report.md']:
        if core_file not in files_to_analyze:
            files_to_analyze.append(core_file)

    code_context = ""
    for f in files_to_analyze:
        content = read_file(f)
        if content:
            code_context += f"\n--- {f} ---\n{content[:2000]}\n"

    prompt = f"""
Based on the provided codebase context, generate a Red Team Architecture Brief.

Focus areas:
1. Challenge every documented architectural decision in ARCHITECTURE.md — ask "what assumption does this break if it's wrong?"
2. Identify single points of failure: any module, interface, or data path where failure has no graceful degradation path.
3. Evaluate the agent orchestration design for prompt injection vulnerabilities, unbounded recursion risk, and failure to enforce agent scope boundaries.
4. Assess whether the eval framework in evals/ actually tests adversarial inputs.
5. Identify coupling: any two modules that cannot be independently deployed, tested, or replaced without modifying both.
6. Project the architecture 18 months forward: name the top 3 decisions made today that will become re-architecture projects if the system scales 10x.
7. Also evaluate for off-by-one errors (especially in .agents/), panic paths (.unwrap()/.expect()), integer overflow/underflow, race conditions, silent logic failures, and test coverage gaps.

You MUST respond ONLY with a JSON array of findings. Do not include markdown formatting like ```json in the output. Each finding should be a JSON object with the following string fields:
- "title": A short, descriptive title for the finding.
- "finding": The description of the finding.
- "current_assumption": The current assumption being challenged.
- "attack_scenario": The attack scenario or failure mode.
- "blast_radius": The impact/blast radius.
- "recommended_change": The recommended structural change or fix.
- "file_line": File and line number (or general location) where this occurs.
- "severity": "CRITICAL", "HIGH", "MEDIUM", or "LOW".
- "reproducible_scenario": A minimal reproducible scenario or trigger path.

Code Context:
{code_context}
"""

    response = call_openai(prompt)
    if not response:
        return

    try:
        response = response.strip()
        if response.startswith("```json"):
            response = response.split("```json")[1].split("```")[0].strip()
        elif response.startswith("```"):
            response = response.split("```")[1].split("```")[0].strip()

        findings = json.loads(response)
    except Exception as e:
        print(f"Error parsing JSON from LLM: {e}\nResponse: {response}")
        return

    if not repo or not token:
        print("Missing GITHUB_REPOSITORY or GITHUB_TOKEN, skipping issue creation.")
        return

    existing_issues = get_existing_issues(repo, token)

    for finding in findings:
        title = f"{finding.get('title', 'Architecture Finding')}"
        if any(title.lower() in existing.lower() for existing in existing_issues):
            print(f"Skipping duplicate finding: {title}")
            continue

        body = f"### Red Team Architecture Brief\n\n"
        body += f"**Finding**: {finding.get('finding')}\n"
        body += f"**Current Assumption**: {finding.get('current_assumption')}\n"
        body += f"**Attack Scenario / Failure Mode**: {finding.get('attack_scenario')}\n"
        body += f"**Blast Radius**: {finding.get('blast_radius')}\n"
        body += f"**Recommended Structural Change**: {finding.get('recommended_change')}\n\n"
        body += f"**Location**: {finding.get('file_line')}\n"
        body += f"**Severity**: {finding.get('severity')}\n"
        body += f"**Reproducible Scenario**: {finding.get('reproducible_scenario')}\n\n"
        body += f"> **Note to Developers**: Never close this issue without a documented architectural decision record (ADR) response.\n"

        labels = ["architecture-review", "adversarial", "bug", "agent-found"]
        create_issue(repo, token, title, body, labels)

if __name__ == "__main__":
    main()
