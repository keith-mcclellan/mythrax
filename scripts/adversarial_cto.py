#!/usr/bin/env python3
import json
import os
import subprocess
import urllib.request
import urllib.parse
from typing import List, Dict, Any

def get_modified_files() -> List[str]:
    """Gets files modified in the PR, or all files if not a PR."""
    if os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        base_ref = os.environ.get("GITHUB_BASE_REF")
        if base_ref:
            try:
                subprocess.run(["git", "fetch", "--depth=1", "origin", base_ref], check=True)
                result = subprocess.run(
                    ["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"],
                    capture_output=True,
                    text=True,
                    check=True
                )
                return [f for f in result.stdout.split('\n') if f]
            except subprocess.CalledProcessError as e:
                print(f"Error getting modified files: {e}")
                return []

    # Weekly run on main - analyze relevant directories
    return [] # We'll fall back to target directories

def read_file_chunks(filepath: str, chunk_size: int = 20000) -> List[str]:
    """Reads a file and yields chunks of it."""
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
            return [content[i:i+chunk_size] for i in range(0, len(content), chunk_size)]
    except Exception as e:
        print(f"Error reading file {filepath}: {e}")
        return []

def call_llm(system_prompt: str, content: str) -> Dict[str, Any]:
    """Calls OpenAI API to analyze code."""
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not set.")
        return {}

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    data = {
        "model": "gpt-4o",
        "response_format": { "type": "json_object" },
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": f"Analyze this code for bugs as specified:\n\n{content}"}
        ],
        "temperature": 0.2
    }

    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            message_content = result['choices'][0]['message']['content']
            return json.loads(message_content)
    except Exception as e:
        print(f"Error calling LLM API: {e}")
        return {}

def create_github_issue(finding: Dict[str, Any]) -> None:
    """Creates a GitHub issue for a specific finding."""
    token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print("GH_TOKEN or GITHUB_REPOSITORY not set. Cannot create issue.")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }

    title = f"Bug: {finding.get('severity', 'Unknown')} in {finding.get('file', 'Unknown')}"
    body = (
        f"**File:** {finding.get('file', 'Unknown')}\n"
        f"**Line Number:** {finding.get('line_number', 'Unknown')}\n"
        f"**Severity:** {finding.get('severity', 'Unknown')}\n\n"
        f"**Scenario:**\n{finding.get('scenario', finding.get('minimal_reproducible_scenario', 'Unknown'))}\n\n"
        f"**Suggested Fix:**\n{finding.get('suggested_fix', 'Unknown')}\n"
    )

    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Successfully created issue for {finding.get('file', 'Unknown')}")
            else:
                print(f"Failed to create issue. Status code: {response.status}")
    except Exception as e:
        print(f"Error creating GitHub issue: {e}")

def get_untested_public_functions() -> List[Dict[str, Any]]:
    """Analyzes test coverage gaps using static analysis."""
    findings = []
    try:
        # Find all public functions in src
        pub_fns_result = subprocess.run(
            ["grep", "-rn", r"pub fn", "mythrax-core/src"],
            capture_output=True,
            text=True,
            check=True
        )

        for line in pub_fns_result.stdout.split('\n'):
            if not line:
                continue

            parts = line.split(':')
            if len(parts) >= 3:
                file_path = parts[0]
                line_number = parts[1]
                code = ':'.join(parts[2:]).strip()

                # Extract function name
                fn_name_parts = code.split('pub fn ')
                if len(fn_name_parts) > 1:
                    fn_name_with_args = fn_name_parts[1]
                    fn_name = fn_name_with_args.split('(')[0].split('<')[0].strip()

                    # Check if tests exist for this function name
                    test_grep_result = subprocess.run(
                        ["grep", "-r", fn_name, "mythrax-core/tests", "evals/"],
                        capture_output=True,
                        text=True
                    )

                    if test_grep_result.returncode != 0:
                        findings.append({
                            "file": file_path,
                            "line_number": line_number,
                            "severity": "Low",
                            "scenario": f"Public function `{fn_name}` has no corresponding tests in test suite or evals.",
                            "suggested_fix": f"Add comprehensive unit/integration tests for `{fn_name}`."
                        })
    except subprocess.CalledProcessError as e:
        print(f"Error during static analysis: {e}")

    return findings

def main():
    # 1. Load configuration
    try:
        with open(".agents/cto_agent/config.json", "r") as f:
            config_data = json.load(f)
            custom_agent = config_data.get("config", {}).get("customAgent", {})
            sys_sections = custom_agent.get("systemPromptSections", {})
            target_dirs = custom_agent.get("targetDirectories", [])

            system_prompt = (
                f"{sys_sections.get('persona', '')}\n\n"
                f"Task: {sys_sections.get('task', '')}\n\n"
                f"Focus Areas:\n- " + "\n- ".join(sys_sections.get('focusAreas', [])) + "\n\n"
                f"{sys_sections.get('outputFormat', '')}"
            )
    except Exception as e:
        print(f"Failed to load agent configuration: {e}")
        return

    all_findings = []

    # 2. Add static analysis findings (untested pub fns)
    untested_fns = get_untested_public_functions()
    for fn in untested_fns:
        print(f"Found untested public function: {json.dumps(fn, indent=2)}")
        all_findings.append(fn)

    # 3. Determine files to analyze with LLM
    modified_files = get_modified_files()
    files_to_analyze = set()

    # Allowed extensions for analysis
    allowed_exts = ('.rs', '.py', '.json', '.ts', '.js', '.md')

    if modified_files:
        files_to_analyze.update([f for f in modified_files if f.endswith(allowed_exts)])
    else:
        # Fallback to scanning target directories
        for d in target_dirs:
            if os.path.exists(d):
                for root, _, files in os.walk(d):
                    for file in files:
                        if file.endswith(allowed_exts):
                            files_to_analyze.add(os.path.join(root, file))

    # 4. Analyze files
    for file_path in files_to_analyze:
        print(f"Analyzing {file_path}...")
        chunks = read_file_chunks(file_path)
        for i, chunk in enumerate(chunks):
            result = call_llm(system_prompt, chunk)
            findings = result.get('findings', [])
            for finding in findings:
                # Ensure the finding includes the file path if missing
                if 'file' not in finding or not finding['file'] or finding['file'] == "Unknown":
                    finding['file'] = file_path
                print(f"LLM Finding in {file_path}: {json.dumps(finding, indent=2)}")
                all_findings.append(finding)

    # 5. File issues for realistic findings
    for finding in all_findings:
        # Static analysis ones have a "scenario", LLM might use either
        scenario = finding.get('scenario', finding.get('minimal_reproducible_scenario', ''))
        # Create issues for everything found in this automated run
        create_github_issue(finding)

if __name__ == "__main__":
    main()
