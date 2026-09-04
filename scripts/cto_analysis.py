import json
import os
import sys
import urllib.request
import subprocess

def chunk_files(files, max_size=100000):
    chunks = []
    current_chunk = []
    current_size = 0
    # Add a special context map for tests so the LLM knows about coverage
    for f in files:
        try:
            with open(f, 'r') as file:
                content = file.read()
                size = len(content)
                if current_size + size > max_size and current_chunk:
                    chunks.append(current_chunk)
                    current_chunk = []
                    current_size = 0
                current_chunk.append((f, content))
                current_size += size
        except Exception as e:
            print(f"Skipping {f}: {e}")
    if current_chunk:
        chunks.append(current_chunk)
    return chunks

def file_issue(repo, finding, token):
    title = f"CTO Finding: {finding.get('severity', 'Issue')} in {finding.get('file', 'unknown')}"
    body = f"""
## Adversarial CTO Finding

**File:** `{finding.get('file', 'unknown')}`
**Line:** {finding.get('line', 'unknown')}
**Severity:** {finding.get('severity', 'unknown')}

### Scenario
{finding.get('scenario', 'No scenario provided')}

### Suggested Fix
{finding.get('fix', 'No fix provided')}

### Effort
{finding.get('effort', 'unknown')}
"""
    labels = ["bug", "agent-found"]

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json"
    }
    data = {
        "title": title,
        "body": body,
        "labels": labels
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Filed issue for {finding.get('file')}:{finding.get('line')}")
            else:
                print(f"Failed to file issue, status: {response.status}")
    except Exception as e:
        print(f"Exception filing issue: {e}")


def analyze_diff():
    # Only PR triggers
    try:
        base_ref = os.environ.get("GITHUB_BASE_REF")
        if base_ref:
            print(f"PR trigger, base_ref: {base_ref}")
            # Fix subprocess injection vulnerability
            diff_files_output = subprocess.check_output(["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"]).decode()
            files_to_analyze = diff_files_output.strip().split('\n')
            files_to_analyze = [f for f in files_to_analyze if f and os.path.exists(f)]
        else:
            print("Weekly/Full trigger")
            files_to_analyze = []
            for d in ["mythrax-core/src", "evals", ".agents"]:
                if os.path.exists(d):
                    for root, _, files in os.walk(d):
                        for file in files:
                            if file.endswith(".rs") or file.endswith(".py") or file.endswith(".json") or file.endswith(".md"):
                                files_to_analyze.append(os.path.join(root, file))
    except Exception as e:
        print(f"Failed to get diff files: {e}")
        return

    # Ingest CTO prompt
    try:
        with open(".agents/cto_agent/config.json", "r") as f:
            agent_config = json.load(f)
            system_prompt = agent_config["config"]["customAgent"]["systemPromptSections"]
    except Exception as e:
        print(f"Failed to read agent config: {e}")
        return

    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found. Bypassing LLM logic for graceful degradation.")
        return

    print(f"Analyzing {len(files_to_analyze)} files...")

    chunks = chunk_files(files_to_analyze)
    all_findings = []

    # Pre-compute list of all files to mitigate context splitting false positives for test coverage
    all_files_list = "\n".join(files_to_analyze)

    for chunk in chunks:
        prompt_content = f"Note: The codebase contains the following files which may not be in this chunk. Consider this when analyzing test coverage:\n{all_files_list}\n\nAnalyze the following files:\n\n"
        for f_path, content in chunk:
            prompt_content += f"--- {f_path} ---\n{content}\n\n"

        data = {
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": prompt_content}
            ],
            "response_format": {"type": "json_object"}
        }

        req = urllib.request.Request(
            "https://api.openai.com/v1/chat/completions",
            data=json.dumps(data).encode('utf-8'),
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {api_key}"
            }
        )

        try:
            with urllib.request.urlopen(req) as response:
                result = json.loads(response.read().decode('utf-8'))
                findings = json.loads(result['choices'][0]['message']['content']).get("findings", [])
                all_findings.extend(findings)
        except Exception as e:
            print(f"Error calling LLM API: {e}")
            continue

    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")

    for finding in all_findings:
        # Full, detailed findings directly to stdout
        print(json.dumps(finding, indent=2))
        if finding.get("trigger_path_realistic", False) and repo and token:
            file_issue(repo, finding, token)

if __name__ == "__main__":
    analyze_diff()
