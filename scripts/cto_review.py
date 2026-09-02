import json
import os
import sys
import subprocess
import urllib.request
import urllib.error
import glob

def main():
    print("CTO Review Script Started")

    try:
        with open('.agents/cto_agent/config.json', 'r') as f:
            config = json.load(f)
            sections = config['config']['customAgent']['systemPromptSections']
            system_prompt = f"{sections.get('role', '')}\n\n{sections.get('task', '')}\n\n{sections.get('output', '')}"
    except Exception as e:
        print(f"Failed to read agent config: {e}")
        return

    def read_dir(path):
        contents = ""
        if os.path.exists(path):
            if os.path.isdir(path):
                for root, dirs, files in os.walk(path):
                    for file in files:
                        filepath = os.path.join(root, file)
                        try:
                            with open(filepath, 'r', encoding='utf-8') as f:
                                contents += f"--- {filepath} ---\n{f.read()}\n\n"
                        except Exception:
                            pass
            else:
                 try:
                    with open(path, 'r', encoding='utf-8') as f:
                        contents += f"--- {path} ---\n{f.read()}\n\n"
                 except Exception:
                     pass
        return contents

    context = ""
    context += read_dir("ARCHITECTURE.md")
    context += read_dir("mock_audit_report.md")
    context += read_dir("mythrax-core/src/")
    context += read_dir(".agents/")
    context += read_dir("evals/")

    print(f"Ingested codebase context")

    openai_key = os.environ.get("OPENAI_API_KEY")
    findings = []

    if openai_key:
        print("Calling LLM API for analysis...")
        system_msg = system_prompt + "\n\nYou must return a valid JSON object with a single key 'findings' that contains a list of finding objects. Each finding object must have: finding, current_assumption, attack_scenario, blast_radius, recommended_structural_change, and labels (a list of strings)."
        max_context = 100000
        if len(context) > max_context:
            context = context[:max_context] + "\n...[TRUNCATED]"

        payload = {
            "model": "gpt-4-turbo",
            "messages": [
                {"role": "system", "content": system_msg},
                {"role": "user", "content": f"Review this codebase context:\n{context}"}
            ],
            "response_format": {"type": "json_object"}
        }

        req = urllib.request.Request(
            "https://api.openai.com/v1/chat/completions",
            data=json.dumps(payload).encode('utf-8'),
            headers={
                "Authorization": f"Bearer {openai_key}",
                "Content-Type": "application/json"
            },
            method="POST"
        )

        try:
            response = urllib.request.urlopen(req)
            result = json.loads(response.read().decode('utf-8'))
            content = result['choices'][0]['message']['content']
            parsed = json.loads(content)
            findings = parsed.get('findings', [])
        except Exception as e:
            print(f"Failed to call OpenAI API: {e}")
    else:
        print("OPENAI_API_KEY not set. Cannot perform actual LLM analysis.")

    if not findings:
        print("No findings generated.")
        return

    print("Detailed Findings:")
    print(json.dumps(findings, indent=2))

    os.makedirs("issues", exist_ok=True)

    for i, finding in enumerate(findings):
        issue_path = f"issues/issue_{i}.md"
        with open(issue_path, "w", encoding='utf-8') as f:
            f.write(f"## Red Team Architecture Brief: {finding.get('finding', 'Unknown')}\n\n")
            f.write(f"**Finding**: {finding.get('finding', 'Unknown')}\n")
            f.write(f"**Current Assumption**: {finding.get('current_assumption', 'Unknown')}\n")
            f.write(f"**Attack Scenario**: {finding.get('attack_scenario', 'Unknown')}\n")
            f.write(f"**Blast Radius**: {finding.get('blast_radius', 'Unknown')}\n")
            f.write(f"**Recommended Structural Change**: {finding.get('recommended_structural_change', 'Unknown')}\n")

        print(f"Created mock issue: {issue_path}")

    token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")

    if token and repo:
        print(f"Filing issues to {repo}...")
        for finding in findings:
            body = (f"## Red Team Architecture Brief\n\n"
                    f"**Finding**: {finding.get('finding', 'Unknown')}\n"
                    f"**Current Assumption**: {finding.get('current_assumption', 'Unknown')}\n"
                    f"**Attack Scenario**: {finding.get('attack_scenario', 'Unknown')}\n"
                    f"**Blast Radius**: {finding.get('blast_radius', 'Unknown')}\n"
                    f"**Recommended Structural Change**: {finding.get('recommended_structural_change', 'Unknown')}\n\n"
                    f"**Action Required**: Do not close this issue without a documented architectural decision record (ADR) response.")

            payload = {
                "title": f"Adversarial Review: {finding.get('finding', 'Unknown')}",
                "body": body,
                "labels": finding.get('labels', ["architecture-review", "adversarial"])
            }

            req = urllib.request.Request(
                f"https://api.github.com/repos/{repo}/issues",
                data=json.dumps(payload).encode('utf-8'),
                headers={
                    "Authorization": f"Bearer {token}",
                    "Accept": "application/vnd.github.v3+json",
                    "Content-Type": "application/json"
                },
                method="POST"
            )

            try:
                response = urllib.request.urlopen(req)
                if response.status in [200, 201]:
                    print(f"Successfully filed issue: {finding.get('finding', 'Unknown')}")
                else:
                    print(f"Failed to file issue. Status: {response.status}")
            except urllib.error.URLError as e:
                print(f"Failed to file issue to GitHub API: {e}")
    else:
        print("GH_TOKEN or GITHUB_REPOSITORY not set. Mock issues were generated locally in issues/.")

if __name__ == "__main__":
    main()
