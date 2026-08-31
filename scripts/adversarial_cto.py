import json
import os
import urllib.request
import urllib.error
import urllib.parse
from datetime import datetime

def load_agent_config(filepath):
    with open(filepath, 'r') as f:
        return json.load(f)

def run_cto_review():
    config = load_agent_config('.agents/agents/adversarial_cto.json')
    system_prompt = "\n".join(config['config']['customAgent']['systemPromptSections'])

    # Read relevant contexts
    arch_md = ""
    if os.path.exists('ARCHITECTURE.md'):
        with open('ARCHITECTURE.md', 'r') as f:
            arch_md = f.read()

    mock_audit = ""
    if os.path.exists('mock_audit_report.md'):
        with open('mock_audit_report.md', 'r') as f:
            mock_audit = f.read()

    evals_context = ""
    if os.path.exists('evals'):
        evals_context = "evals directory exists. "
        for root, _, files in os.walk('evals'):
            for file in files:
                filepath = os.path.join(root, file)
                evals_context += f"File: {filepath}\n"
                try:
                    with open(filepath, 'r', encoding='utf-8') as eval_file:
                        content = eval_file.read()
                        if len(content) > 1000:
                            content = content[:500] + "\n...[truncated]...\n" + content[-500:]
                        evals_context += f"Content:\n{content}\n---\n"
                except Exception as e:
                    evals_context += f"Could not read content: {e}\n---\n"

    src_context = "mythrax-core/src/ directory exists."

    prompt = f"""
{system_prompt}

Context:
ARCHITECTURE.md:
{arch_md}

mock_audit_report.md:
{mock_audit}

Evals directory info:
{evals_context}

Mythrax-core src info:
{src_context}
"""

    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("Error: OPENAI_API_KEY environment variable is not set. Cannot run review.", flush=True)
        return

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    data = {
        "model": "gpt-4o-mini",
        "messages": [
            {"role": "system", "content": "You are a highly adversarial CTO."},
            {"role": "user", "content": prompt}
        ]
    }

    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
        response = urllib.request.urlopen(req)
        result = json.loads(response.read().decode('utf-8'))
        content = result['choices'][0]['message']['content']

        # Clean up output to extract JSON array
        if "```json" in content:
            content = content.split("```json")[1].split("```")[0].strip()
        elif "```" in content:
            content = content.split("```")[1].strip()

        findings = json.loads(content)

        for finding in findings:
            title = finding.get('title', 'Architecture Review Finding')
            body = f"""**Finding:** {finding.get('finding')}
**Current Assumption:** {finding.get('current_assumption')}
**Attack Scenario:** {finding.get('attack_scenario')}
**Blast Radius:** {finding.get('blast_radius')}
**Recommended Structural Change:** {finding.get('recommended_structural_change')}

*Note: Never close this issue without a documented architectural decision record (ADR) response.*
"""
            print(f"Submitting issue: {title}")
            print(body)
            submit_github_issue(title, body, ["architecture-review", "adversarial"])

    except Exception as e:
        print(f"Error during LLM API call or parsing: {e}", flush=True)


def submit_github_issue(title, body, labels):
    gh_token = os.environ.get("GH_TOKEN")
    if not gh_token:
        print("Warning: GH_TOKEN not set, skipping issue creation. Printing to stdout instead:")
        print(f"Title: {title}")
        print(body)
        return

    repo = os.environ.get("GITHUB_REPOSITORY")
    if not repo:
        print("Warning: GITHUB_REPOSITORY not set. Cannot file issue.")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"token {gh_token}",
        "Accept": "application/vnd.github.v3+json"
    }
    data = {
        "title": title,
        "body": body,
        "labels": labels
    }

    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
        response = urllib.request.urlopen(req)
        print(f"Issue filed successfully.")
    except Exception as e:
        print(f"Failed to file issue: {e}")

if __name__ == "__main__":
    run_cto_review()
