import os
import sys
import json
import subprocess
import urllib.request
import urllib.error

def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("Warning: OPENAI_API_KEY not set. Bypassing LLM analysis.")
        return

    # Load agent configuration
    config_path = ".agents/cto_agent/config.json"
    if not os.path.exists(config_path):
        print(f"Error: {config_path} not found.")
        sys.exit(1)

    with open(config_path, "r") as f:
        config = json.load(f)

    try:
        system_prompt = "\n".join(config['config']['customAgent']['systemPromptSections'])
    except KeyError as e:
        print(f"Error parsing config.json: {e}")
        sys.exit(1)

    event_name = os.environ.get("GITHUB_EVENT_NAME")

    files_to_analyze = []
    if event_name == "pull_request":
        base_ref = os.environ.get("GITHUB_BASE_REF")
        if not base_ref:
            print("Warning: GITHUB_BASE_REF not set for PR. Analyzing all relevant files.")
            files_to_analyze = get_all_relevant_files()
        else:
            try:
                # Fetch base ref
                subprocess.run(["git", "fetch", "origin", base_ref], check=True, capture_output=True)
                # Get modified files
                result = subprocess.run(["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"], check=True, capture_output=True, text=True)
                modified_files = result.stdout.strip().split('\n')
                for file in modified_files:
                    if file.startswith("mythrax-core/") or file.startswith("evals/") or file.startswith(".agents/"):
                        if os.path.exists(file) and file.endswith(('.rs', '.py', '.json', '.toml')):
                            files_to_analyze.append(file)
            except subprocess.CalledProcessError as e:
                print(f"Error getting changed files: {e}")
                files_to_analyze = get_all_relevant_files()
    else:
        # Scheduled run - analyze everything
        files_to_analyze = get_all_relevant_files()

    if not files_to_analyze:
        print("No relevant files to analyze.")
        return

    print(f"Analyzing {len(files_to_analyze)} files...")

    all_findings = []

    # Chunking files to avoid context limits
    chunk_size = 5
    for i in range(0, len(files_to_analyze), chunk_size):
        chunk = files_to_analyze[i:i+chunk_size]
        findings = analyze_chunk(chunk, system_prompt, api_key)
        all_findings.extend(findings)

    print(f"\nFound {len(all_findings)} potential issues.")

    # Process findings
    for finding in all_findings:
        print(f"\n--- Finding in {finding.get('file_and_line', 'Unknown')} ---")
        print(f"Scenario: {finding.get('scenario', '')}")
        print(f"Severity: {finding.get('severity', '')}")
        print(f"Suggested Fix: {finding.get('suggested_fix', '')}")
        print(f"Realistic Trigger: {finding.get('realistic_trigger', False)}")

        if finding.get('realistic_trigger', False):
            file_issue(finding)

def get_all_relevant_files():
    relevant = []
    directories = ["mythrax-core", "evals", ".agents"]
    for directory in directories:
        if os.path.exists(directory):
            for root, _, files in os.walk(directory):
                for file in files:
                    if file.endswith(('.rs', '.py', '.json', '.toml')):
                        relevant.append(os.path.join(root, file))
    return relevant

def analyze_chunk(files, system_prompt, api_key):
    content = ""
    for file in files:
        with open(file, 'r', encoding='utf-8', errors='ignore') as f:
            content += f"\n\n--- File: {file} ---\n"
            content += f.read()

    # Call OpenAI API
    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    data = {
        "model": "gpt-4-turbo-preview",
        "response_format": { "type": "json_object" },
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            {
                "role": "user",
                "content": f"Analyze the following code:\n{content}"
            }
        ]
    }

    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            response_text = result['choices'][0]['message']['content']
            parsed = json.loads(response_text)
            return parsed.get("findings", [])
    except Exception as e:
        print(f"Error calling LLM API: {e}")
        return []

def file_issue(finding):
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")

    if not token or not repo:
        print("Skipping issue creation: GITHUB_TOKEN or GITHUB_REPOSITORY not set.")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json"
    }

    title = f"Bug Found: Potential issue in {finding.get('file_and_line', 'Unknown')}"
    body = f"## Automated CTO Agent Finding\n\n**File and Line:** {finding.get('file_and_line', 'Unknown')}\n**Severity:** {finding.get('severity', 'Unknown')}\n\n### Minimal Reproducible Scenario\n{finding.get('scenario', '')}\n\n### Suggested Fix\n{finding.get('suggested_fix', '')}"

    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Successfully filed issue for {finding.get('file_and_line', 'Unknown')}")
            else:
                print(f"Failed to file issue. Status: {response.status}")
    except Exception as e:
        print(f"Error filing issue: {e}")

if __name__ == "__main__":
    main()
