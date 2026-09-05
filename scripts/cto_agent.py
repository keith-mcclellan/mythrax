import os
import sys
import json
import subprocess
import urllib.request
import re

def parse_config():
    try:
        with open('.agents/cto_agent/config.json', 'r') as f:
            config = json.load(f)
            return "\n".join(config['config']['customAgent']['systemPromptSections'])
    except Exception:
        return "You are an Adversarial CTO."

def get_modified_files():
    base_ref = os.environ.get('GITHUB_BASE_REF')
    if not base_ref:
        return None
    try:
        # We assume actions/checkout@v4 with fetch-depth: 0 has been run
        result = subprocess.run(['git', 'diff', '--name-only', f'origin/{base_ref}', 'HEAD'], capture_output=True, text=True, check=True)
        return result.stdout.splitlines()
    except Exception as e:
        print(f"Error getting modified files: {e}")
        return None

def chunk_text(text, max_length=100000):
    """Split text into chunks of roughly max_length characters."""
    return [text[i:i+max_length] for i in range(0, len(text), max_length)]

def run_llm_analysis(system_prompt, files_content):
    openai_api_key = os.environ.get('OPENAI_API_KEY')
    if not openai_api_key:
        return []

    findings = []

    # We chunk the files content to avoid hitting context window limits
    chunks = chunk_text(files_content)

    for chunk in chunks:
        # Construct the payload for the OpenAI API
        payload = {
            "model": "gpt-4-turbo-preview",  # Example model
            "response_format": { "type": "json_object" },
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt + "\n\nAnalyze the provided codebase content for logic bugs, off-by-one errors (especially in .agents/), panic paths (unwrap/expect), integer overflows, race conditions, false positives, and untested public functions. \n\nYou MUST return a valid JSON object containing a single key 'findings' which holds a JSON array of result objects. Each result object must have the following keys: 'file', 'line', 'type', 'severity', 'scenario', 'fix', 'code'."
                },
                {
                    "role": "user",
                    "content": f"Here is the codebase content to analyze:\n\n{chunk}"
                }
            ]
        }

        req = urllib.request.Request(
            "https://api.openai.com/v1/chat/completions",
            data=json.dumps(payload).encode('utf-8'),
            headers={
                "Authorization": f"Bearer {openai_api_key}",
                "Content-Type": "application/json"
            }
        )

        try:
            with urllib.request.urlopen(req, timeout=60) as response:
                result_data = json.loads(response.read().decode('utf-8'))
                content = result_data['choices'][0]['message']['content']
                parsed_content = json.loads(content)

                # Check for the single 'findings' key requirement
                if 'findings' in parsed_content and isinstance(parsed_content['findings'], list):
                    findings.extend(parsed_content['findings'])
                else:
                    print(f"Warning: LLM response did not contain the expected 'findings' array. Got keys: {list(parsed_content.keys())}")
        except Exception as e:
            print(f"Error querying LLM: {e}")

    return findings

def analyze_code():
    print("Running Adversarial CTO Analysis...")
    findings = []
    system_prompt = parse_config()

    modified_files = get_modified_files()

    # In automated CI scripts that rely on external LLMs (e.g., the 'Adversarial CTO' Python agent),
    # ensure the script degrades gracefully if the API key environment variable is not present
    # (which often happens in PRs from forks), bypassing the LLM logic instead of triggering a fatal exit.
    openai_api_key = os.environ.get('OPENAI_API_KEY')
    if openai_api_key:
        print("OPENAI_API_KEY found, running LLM analysis...")

        def read_dir(directory):
            content = ""
            if not os.path.isdir(directory):
                return content
            for root, _, files in os.walk(directory):
                for f in files:
                    if f.endswith('.rs') or f.endswith('.py') or f.endswith('.md'):
                        path = os.path.join(root, f)
                        if modified_files is not None and path not in modified_files:
                            continue
                        try:
                            with open(path, 'r') as file:
                                content += f"--- {path} ---\n{file.read()}\n\n"
                        except Exception:
                            pass
            return content

        # Explicitly read and ingest the contents of any targeted directories (e.g., evals/) into the LLM context to perform a valid review.
        evals_content = read_dir('evals')
        src_content = read_dir('mythrax-core/src')
        agents_content = read_dir('.agents')

        all_content = evals_content + src_content + agents_content

        if modified_files is not None and not all_content:
            print("No relevant files modified. Skipping LLM analysis.")
        elif all_content:
            llm_findings = run_llm_analysis(system_prompt, all_content)
            findings.extend(llm_findings)
    else:
        print("OPENAI_API_KEY not found. Degrading gracefully to static analysis.")

    # Always run static panic check as a baseline
    try:
        find_cmd = ['find']
        for dir_name in ['mythrax-core/src', 'evals']:
            if os.path.isdir(dir_name):
                find_cmd.append(dir_name)

        if len(find_cmd) > 1: # We have directories to search
            find_cmd.extend(['-type', 'f', '-name', '*.rs'])
            result = subprocess.run(find_cmd + ['-exec', 'grep', '-HnE', r'\b(unwrap|expect)\b', '{}', '+'], capture_output=True, text=True)
            if result.returncode == 0:
                for line in result.stdout.splitlines():
                    if ('.unwrap(' in line or '.expect(' in line) and 'tests/' not in line and '#[test]' not in line and 'assert' not in line:
                        parts = line.split(':', 2)
                        if len(parts) >= 3:
                            file_path = parts[0]
                            if modified_files is not None and file_path not in modified_files:
                                continue
                            findings.append({
                                'file': file_path,
                                'line': parts[1],
                                'type': 'Panic Path',
                                'severity': 'High',
                                'scenario': 'Runtime unwrap/expect could trigger panic on unexpected input.',
                                'fix': 'Replace unwrap()/expect() with proper Result/Option propagation (?) or match statements.',
                                'code': parts[2].strip()
                            })
    except Exception as e:
        print(f"Error checking panic paths: {e}")

    # Output detailed findings to stdout
    if findings:
        print("\n--- FINDINGS ---")
        for f in findings:
            print(f"File: {f.get('file', 'N/A')}:{f.get('line', 'N/A')}")
            print(f"Type: {f.get('type', 'N/A')} (Severity: {f.get('severity', 'N/A')})")
            print(f"Scenario: {f.get('scenario', 'N/A')}")
            print(f"Suggested Fix: {f.get('fix', 'N/A')}")
            print(f"Code: {f.get('code', 'N/A')}")
            print("-" * 20)

        # File issues if GITHUB_TOKEN is present
        github_token = os.environ.get('GITHUB_TOKEN')
        github_repo = os.environ.get('GITHUB_REPOSITORY')

        if github_token and github_repo:
            print("\nFiling issues to GitHub...")
            for f in findings:
                title = f"Adversarial CTO: {f.get('type', 'Issue')} in {f.get('file', 'unknown')}:{f.get('line', 'unknown')}"
                body = f"**File:** `{f.get('file', 'unknown')}:{f.get('line', 'unknown')}`\n**Severity:** {f.get('severity', 'unknown')}\n**Scenario:** {f.get('scenario', 'unknown')}\n**Code:**\n```rust\n{f.get('code', '')}\n```\n**Suggested Fix:** {f.get('fix', 'unknown')}"

                req = urllib.request.Request(
                    f"https://api.github.com/repos/{github_repo}/issues",
                    data=json.dumps({"title": title, "body": body, "labels": ["bug", "agent-found"]}).encode('utf-8'),
                    headers={
                        "Authorization": f"token {github_token}",
                        "Accept": "application/vnd.github.v3+json",
                        "Content-Type": "application/json"
                    }
                )
                try:
                    with urllib.request.urlopen(req, timeout=60) as response:
                        print(f"Created issue: {response.status}")
                except Exception as e:
                    print(f"Failed to create issue: {e}")
        else:
            print("\nSkipping issue creation: GITHUB_TOKEN or GITHUB_REPOSITORY not set.")
    else:
        print("No issues found.")

if __name__ == '__main__':
    analyze_code()
