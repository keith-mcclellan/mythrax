import os
import sys
import json
import subprocess
import urllib.request
import urllib.parse
from pathlib import Path
import re

def get_files_to_analyze():
    # If running in a PR context, only analyze changed files to prevent issue spam
    is_pr = os.environ.get("GITHUB_EVENT_NAME") == "pull_request"
    base_ref = os.environ.get("GITHUB_BASE_REF")

    if is_pr and base_ref:
        try:
            subprocess.run(["git", "fetch", "origin", base_ref], check=False)
            diff_output = subprocess.check_output(
                f"git diff --name-only origin/{base_ref} HEAD", shell=True, text=True
            )
            files = []
            for file in diff_output.splitlines():
                if file.startswith(('mythrax-core/', 'evals/', '.agents/')) and file.endswith(('.rs', '.py', '.json', '.md')):
                    if os.path.exists(file):
                        files.append(file)
            print(f"Analyzing {len(files)} changed files for PR.")
            return files
        except Exception as e:
            print(f"Error getting changed files: {e}. Falling back to full scan.")

    # Full scan for scheduled runs (weekly)
    files = []
    for root_dir in ['mythrax-core', 'evals', '.agents']:
        if not os.path.exists(root_dir):
            continue
        for root, _, filenames in os.walk(root_dir):
            for filename in filenames:
                if filename.endswith(('.rs', '.py', '.json', '.md')):
                    files.append(os.path.join(root, filename))
    return files

def chunk_content(files):
    chunks = []
    current_chunk = []
    current_size = 0
    max_size = 50000  # Characters per chunk to avoid context limit

    for file in files:
        try:
            with open(file, 'r', encoding='utf-8') as f:
                content = f.read()
                file_repr = f"\n--- File: {file} ---\n{content}"

                if current_size + len(file_repr) > max_size and current_chunk:
                    chunks.append("".join(current_chunk))
                    current_chunk = []
                    current_size = 0

                # Handling extremely large files by splitting them if needed
                if len(file_repr) > max_size:
                    for i in range(0, len(file_repr), max_size):
                        chunks.append(file_repr[i:i+max_size])
                else:
                    current_chunk.append(file_repr)
                    current_size += len(file_repr)
        except Exception:
            pass

    if current_chunk:
        chunks.append("".join(current_chunk))
    return chunks

def find_untested_public_functions():
    try:
        findings = []
        # basic regex for public functions ignoring comments
        # this is heuristic but better than blind grep
        fn_regex = re.compile(r'^\s*pub\s+(?:async\s+)?fn\s+([a-zA-Z0-9_]+)', re.MULTILINE)

        for root, _, filenames in os.walk('mythrax-core/src'):
            for filename in filenames:
                if not filename.endswith('.rs'): continue
                filepath = os.path.join(root, filename)

                with open(filepath, 'r', encoding='utf-8') as f:
                    content = f.read()

                lines = content.split('\n')
                for idx, line in enumerate(lines):
                    match = fn_regex.search(line)
                    if match:
                        fn_name = match.group(1)
                        # More strict check: does the string "test_" + fn_name exist, or is fn_name inside a #[test] block somehow?
                        # Easiest heuristic: look for test_fn_name in the file
                        test_str = f"test_{fn_name}"
                        has_inline_test = test_str in content

                        has_external_test = False
                        try:
                            # Safe subprocess call, no shell=True
                            grep_proc = subprocess.run(
                                ['grep', '-r', '-l', fn_name, 'mythrax-core/tests', 'evals/'],
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True
                            )
                            if grep_proc.returncode == 0 and grep_proc.stdout.strip():
                                has_external_test = True
                        except Exception:
                            pass

                        if not has_inline_test and not has_external_test:
                            findings.append({
                                "file": filepath,
                                "line": idx + 1,
                                "scenario": f"Public function `{fn_name}` has no corresponding test. Test coverage gap.",
                                "severity": "medium",
                                "fix": f"Add a unit test (`test_{fn_name}`) or integration test for `{fn_name}` to verify edge cases."
                            })
        return findings
    except Exception as e:
        print(f"Error finding untested functions: {e}")
        return []

def call_llm(prompt_sections, code_chunk):
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        print("OPENAI_API_KEY not set. Skipping actual LLM call and generating mock output.")
        return []

    system_prompt = "\n".join(prompt_sections)
    user_prompt = f"Analyze the following codebase chunk for bugs based on your instructions. Return ONLY a JSON array of findings.\n\nCode:\n{code_chunk}"

    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "response_format": {"type": "json_object"}
    }

    req = urllib.request.Request("https://api.openai.com/v1/chat/completions",
                                 data=json.dumps(data).encode('utf-8'),
                                 headers={'Content-Type': 'application/json',
                                          'Authorization': f'Bearer {api_key}'})
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            content = result['choices'][0]['message']['content']
            parsed = json.loads(content)
            if 'findings' in parsed:
                return parsed['findings']
            elif isinstance(parsed, list):
                return parsed
            else:
                return []
    except Exception as e:
        print(f"Error calling LLM: {e}")
        return []

def issue_exists(repo, token, title):
    page = 1
    while True:
        url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100&page={page}"
        req = urllib.request.Request(url, headers={
            'Authorization': f'token {token}',
            'Accept': 'application/vnd.github.v3+json'
        })
        try:
            with urllib.request.urlopen(req) as response:
                issues = json.loads(response.read().decode('utf-8'))
                if not issues:
                    break
                for issue in issues:
                    if issue.get('title') == title:
                        return True
                page += 1
        except Exception as e:
            print(f"Error checking existing issues: {e}")
            break
    return False

def file_issue(finding):
    token = os.environ.get('GH_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    if not token or not repo:
        print(f"Missing GH_TOKEN or GITHUB_REPOSITORY. Cannot file issue for: {finding.get('scenario')}")
        return

    title = f"CTO Finding: Issue in {finding.get('file', 'Unknown')}:{finding.get('line', '?')}"

    # Deduplication
    if issue_exists(repo, token, title):
        print(f"Issue already exists: {title}")
        return

    body = (
        f"**File:** `{finding.get('file', 'Unknown')}`\n"
        f"**Line:** {finding.get('line', '?')}\n"
        f"**Severity:** {finding.get('severity', 'Unknown')}\n\n"
        f"### Scenario\n{finding.get('scenario', 'Unknown')}\n\n"
        f"### Suggested Fix\n{finding.get('fix', 'Unknown')}\n"
    )

    url = f"https://api.github.com/repos/{repo}/issues"
    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    req = urllib.request.Request(url,
                                 data=json.dumps(data).encode('utf-8'),
                                 headers={'Content-Type': 'application/json',
                                          'Authorization': f'token {token}',
                                          'Accept': 'application/vnd.github.v3+json'})
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Filed issue: {title}")
    except Exception as e:
        print(f"Failed to file issue {title}: {e}")

def main():
    config_path = '.agents/cto_agent/config.json'
    if not os.path.exists(config_path):
        print(f"Config not found at {config_path}")
        sys.exit(1)

    with open(config_path, 'r') as f:
        config = json.load(f)

    prompt_sections = config.get('config', {}).get('customAgent', {}).get('systemPromptSections', [])
    if not prompt_sections:
        print("No system prompt sections found in config.")
        sys.exit(1)

    prompt_sections.append("Ensure your output is a JSON object with a single key 'findings' containing a list of finding objects. Keys should be: file, line, scenario, severity, fix.")

    files = get_files_to_analyze()
    chunks = chunk_content(files)

    all_findings = []
    for i, chunk in enumerate(chunks):
        print(f"Analyzing chunk {i+1}/{len(chunks)}...")
        findings = call_llm(prompt_sections, chunk)
        if findings:
            all_findings.extend(findings)

    # Check for untested public functions
    all_findings.extend(find_untested_public_functions())

    for finding in all_findings:
        print("---")
        print(f"File: {finding.get('file')}:{finding.get('line')}")
        print(f"Severity: {finding.get('severity')}")
        print(f"Scenario: {finding.get('scenario')}")
        print(f"Fix: {finding.get('fix')}")
        print("---")
        if finding.get('severity', '').lower() in ['high', 'critical', 'medium']:
             file_issue(finding)

if __name__ == '__main__':
    main()
