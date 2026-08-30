import os
import urllib.request
import json
import glob

def get_file_contents(filepath):
    try:
        with open(filepath, 'r') as f:
            return f.read()
    except:
        return ""

def load_agent_config():
    config_path = ".agents/agents/adversarial_cto/agent.json"
    try:
        with open(config_path, 'r') as f:
            config = json.load(f)
            return config['config']['customAgent']['systemPromptSections'][0]['content']
    except Exception as e:
        print(f"Error loading agent config: {e}")
        return ""

def call_llm(system_prompt, prompt):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not set. Using mock findings.")
        return json.dumps([
            {
                "title": "Finding: Centralized Metadata Store Single Point of Failure",
                "body": "Current Assumption: The metadata store will always be available and low-latency.\nAttack Scenario: High read/write contention during peak traffic causes DB lockups.\nBlast Radius: System-wide outage as new metadata cannot be recorded.\nRecommended Structural Change: Implement distributed caching and asynchronous write queues.\n\n**Note:** Never close this issue without a documented architectural decision record (ADR) response."
            }
        ])

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    full_system_prompt = system_prompt + """

Output your findings STRICTLY as a JSON object with a single key 'findings' containing an array of objects. Each object must have a 'title' (string) starting with 'Finding: ' and a 'body' (string) formatted EXACTLY as:
Current Assumption: ...
Attack Scenario: ...
Blast Radius: ...
Recommended Structural Change: ...

Include the following line at the very end of each body:
**Note:** Never close this issue without a documented architectural decision record (ADR) response.
"""

    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": full_system_prompt},
            {"role": "user", "content": prompt}
        ],
        "response_format": {"type": "json_object"}
    }

    req = urllib.request.Request(url, headers=headers, data=json.dumps(data).encode('utf-8'))
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            content = result['choices'][0]['message']['content']
            parsed = json.loads(content)
            return json.dumps(parsed.get('findings', []))
    except Exception as e:
        print(f"Error calling LLM: {e}")
        return "[]"

def create_github_issue(title, body):
    token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print(f"Dry run (no GH_TOKEN):\nTitle: {title}\nBody: {body}\n---")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Accept": "application/vnd.github.v3+json",
        "Authorization": f"token {token}",
        "Content-Type": "application/json"
    }
    data = {
        "title": title,
        "body": body,
        "labels": ["architecture-review", "adversarial"]
    }

    req = urllib.request.Request(url, headers=headers, data=json.dumps(data).encode('utf-8'))
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            print(f"Created issue: {result.get('html_url', 'unknown URL')}")
    except Exception as e:
        print(f"Error creating issue: {e}")
        if hasattr(e, 'read'):
            print(e.read().decode('utf-8'))

def main():
    print("Starting Adversarial CTO Architecture Review...")

    system_prompt = load_agent_config()

    arch_content = get_file_contents("ARCHITECTURE.md")
    audit_content = get_file_contents("mock_audit_report.md")

    # Read evals context
    evals_context = ""
    for filepath in glob.glob("evals/**/*.py", recursive=True) + glob.glob("evals/**/*.sh", recursive=True):
        evals_context += f"\nFile: {filepath}\n{get_file_contents(filepath)[:2000]}\n"

    prompt = f"Please review the following architecture context:\n\nARCHITECTURE.md:\n{arch_content[:15000]}\n\nmock_audit_report.md:\n{audit_content[:10000]}\n\nEvals Context:\n{evals_context[:10000]}"

    print("Invoking LLM for analysis...")
    findings_json = call_llm(system_prompt, prompt)

    try:
        findings = json.loads(findings_json)
        for finding in findings:
            title = finding.get('title', 'Architecture Finding')
            body = finding.get('body', 'No details provided.')

            print(f"\n--- Finding: {title} ---")
            print(body)
            create_github_issue(title, body)

    except Exception as e:
        print(f"Failed to parse findings: {e}")
        print("Raw output:", findings_json)

if __name__ == "__main__":
    main()
