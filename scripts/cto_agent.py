import os
import json
import subprocess
import urllib.request
import sys

def get_config():
    with open(".agents/cto_agent/config.json", "r") as f:
        config = json.load(f)
    return " ".join(config["config"]["customAgent"]["systemPromptSections"])

def get_files_to_analyze():
    base_ref = os.environ.get("GITHUB_BASE_REF")
    if base_ref:
        try:
            subprocess.run(["git", "fetch", "origin", base_ref], check=True)
            output = subprocess.check_output(["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"]).decode('utf-8')
            files = [f for f in output.split("\n") if f and (f.startswith("mythrax-core/") or f.startswith("evals/"))]
            return files
        except subprocess.CalledProcessError:
            pass

    files = []
    for d in ["mythrax-core", "evals"]:
        if os.path.exists(d):
            for root, _, filenames in os.walk(d):
                for filename in filenames:
                    if filename.endswith(".rs") or filename.endswith(".py"):
                        files.append(os.path.join(root, filename))
    return files

def analyze_file(filepath, system_prompt, api_key):
    try:
        with open(filepath, "r") as f:
            content = f.read()
    except Exception as e:
        print(f"Error reading {filepath}: {e}")
        return []

    if not content.strip():
        return []

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    # Try to chunk by line to avoid cutting in the middle of a token
    lines = content.split('\n')
    chunks = []
    current_chunk = ""
    for line in lines:
        if len(current_chunk) + len(line) > 8000 * 4:
            chunks.append(current_chunk)
            current_chunk = line + "\n"
        else:
            current_chunk += line + "\n"
    if current_chunk:
        chunks.append(current_chunk)

    all_findings = []

    for chunk in chunks:
        data = {
            "model": "gpt-4o",
            "response_format": { "type": "json_object" },
            "messages": [
                {"role": "system", "content": system_prompt + "\n\nYou MUST return a JSON object with a single key 'findings' containing a list of findings."},
                {"role": "user", "content": f"Analyze the following code from {filepath}:\n\n```\n{chunk}\n```"}
            ]
        }

        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
        try:
            with urllib.request.urlopen(req) as response:
                res = json.loads(response.read().decode('utf-8'))
                result_text = res['choices'][0]['message']['content']
                try:
                    result_json = json.loads(result_text)
                    if 'findings' in result_json:
                        all_findings.extend(result_json['findings'])
                except json.JSONDecodeError:
                    print(f"Failed to parse JSON response for {filepath}")
        except Exception as e:
            print(f"API call failed for {filepath}: {e}")
    return all_findings

def create_github_issue(title, body, github_token, repository):
    if not github_token or not repository:
        print("Missing GITHUB_TOKEN or GITHUB_REPOSITORY. Cannot create issue.")
        return

    url = f"https://api.github.com/repos/{repository}/issues"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {github_token}",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Successfully created issue: {title}")
            else:
                print(f"Failed to create issue. Status: {response.status}")
    except Exception as e:
        print(f"Error creating GitHub issue: {e}")

def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found. Degrading gracefully.")
        return

    github_token = os.environ.get("GITHUB_TOKEN")
    repository = os.environ.get("GITHUB_REPOSITORY")

    system_prompt = get_config()
    files = get_files_to_analyze()

    if not files:
        print("No files to analyze.")
        return

    for filepath in files:
        findings = analyze_file(filepath, system_prompt, api_key)
        if findings:
            for i, finding in enumerate(findings):
                print(f"Finding in {filepath}:")
                print(json.dumps(finding, indent=2))

                if finding.get('realistic_trigger'):
                    title = f"{finding.get('severity', 'Bug')} in {filepath}:{finding.get('line', 'unknown')}"
                    body = f"**File:** {filepath}:{finding.get('line', 'unknown')}\n"
                    body += f"**Severity:** {finding.get('severity', 'unknown')}\n\n"
                    body += f"**Scenario:**\n{finding.get('scenario', 'unknown')}\n\n"
                    body += f"**Suggested Fix:**\n{finding.get('fix', 'unknown')}\n"

                    if github_token and repository:
                        create_github_issue(title, body, github_token, repository)

if __name__ == "__main__":
    main()
