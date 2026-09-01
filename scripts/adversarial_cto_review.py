import os
import json
import urllib.request
import urllib.error
import sys

def read_targets():
    targets = ['mythrax-core/src/', '.agents/', 'evals/', 'ARCHITECTURE.md']
    content = ""
    for target in targets:
        if os.path.isfile(target):
            with open(target, 'r') as f:
                content += f"--- {target} ---\n{f.read()}\n"
        elif os.path.isdir(target):
            for root, _, files in os.walk(target):
                for file in files:
                    filepath = os.path.join(root, file)
                    try:
                        with open(filepath, 'r') as f:
                            content += f"--- {filepath} ---\n{f.read()}\n"
                    except:
                        pass
    return content

def chunk_string(s, chunk_size=80000):
    return [s[i:i+chunk_size] for i in range(0, len(s), chunk_size)]

def call_llm(prompt, chunk):
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        print("No OPENAI_API_KEY found, skipping LLM call.")
        return []
    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": f"Analyze this codebase chunk and return JSON array of findings:\n{chunk}"}
        ],
        "response_format": {"type": "json_object"}
    }
    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            content = result['choices'][0]['message']['content']
            try:
                parsed = json.loads(content)
                if isinstance(parsed, dict) and 'findings' in parsed:
                    return parsed['findings']
                elif isinstance(parsed, list):
                    return parsed
                else:
                     return [parsed]
            except:
                return []
    except Exception as e:
        print(f"LLM API call failed: {e}")
        return []

def file_issue(finding):
    token = os.environ.get('GH_TOKEN')
    if not token:
        print("No GH_TOKEN found, skipping issue creation.")
        return
    repo = os.environ.get('GITHUB_REPOSITORY')
    if not repo:
        print("No GITHUB_REPOSITORY found, skipping issue creation.")
        return
    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json"
    }

    # Ensure AD response requirement is in the body
    body = finding.get('body', '') + "\n\n**Note**: Never close this issue without a documented architectural decision record (ADR) response."

    data = {
        "title": finding.get('title', 'Adversarial CTO Finding'),
        "body": body,
        "labels": finding.get('labels', ['architecture-review', 'adversarial'])
    }
    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Successfully filed issue: {data['title']}")
    except Exception as e:
        print(f"Failed to file issue: {e}")

def main():
    with open('.agents/adversarial_cto.json', 'r') as f:
        config = json.load(f)
    prompt = "\n".join(config['config']['customAgent']['systemPromptSections'])

    codebase = read_targets()
    chunks = chunk_string(codebase)

    all_findings = []
    for chunk in chunks:
        findings = call_llm(prompt, chunk)
        all_findings.extend(findings)

    for finding in all_findings:
        print(f"--- FINDING ---\nTitle: {finding.get('title')}\nBody: {finding.get('body')}\nLabels: {finding.get('labels')}\n")
        file_issue(finding)

if __name__ == "__main__":
    main()
