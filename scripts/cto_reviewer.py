import json
import os
import sys
import subprocess
import urllib.request
import urllib.error
import re

def get_config():
    with open(".agents/cto_agent/config.json", "r") as f:
        config = json.load(f)
    return "\n".join(config["config"]["customAgent"]["systemPromptSections"])

def get_modified_files():
    base_ref = os.environ.get("GITHUB_BASE_REF")
    if base_ref:
        try:
            subprocess.check_call(["git", "fetch", "origin", base_ref])
            diff_output = subprocess.check_output(
                ["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"],
                text=True
            )
            files = [f for f in diff_output.splitlines() if f.endswith(".rs") or f.endswith(".py") or f.endswith(".json")]
            return files
        except subprocess.CalledProcessError:
            pass
    # Default: return all target files for weekly scan
    files = []
    targets = ["mythrax-core/src", "evals", ".agents"]
    for target in targets:
        for root, _, filenames in os.walk(target):
            for filename in filenames:
                if filename.endswith(".rs") or filename.endswith(".py") or filename.endswith(".json"):
                    files.append(os.path.join(root, filename))
    return files

def chunk_content(content, chunk_size=25000):
    chunks = []
    lines = content.splitlines()
    current_chunk = []
    current_len = 0

    for line in lines:
        current_chunk.append(line)
        current_len += len(line) + 1
        if current_len > chunk_size:
            chunks.append("\n".join(current_chunk))
            current_chunk = []
            current_len = 0

    if current_chunk:
        chunks.append("\n".join(current_chunk))
    return chunks

def analyze_chunk_with_llm(system_prompt, file_path, chunk):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("Warning: No OPENAI_API_KEY found. Skipping LLM analysis.")
        return []

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    user_prompt = f"Analyze this chunk of {file_path}:\n\n```\n{chunk}\n```\n\nReturn a JSON object containing a single key `findings` which holds a JSON array of your results. Each item must have: `title`, `body` (incorporating file, line, scenario, severity, fix, and code snippet), and `labels` (e.g., ['bug', 'agent-found'])."

    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "response_format": {"type": "json_object"}
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)

    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            content = result["choices"][0]["message"]["content"]
            findings_data = json.loads(content)
            return findings_data.get("findings", [])
    except urllib.error.URLError as e:
        print(f"LLM API request failed: {e}")
        return []
    except json.JSONDecodeError as e:
        print(f"Failed to parse LLM response JSON: {e}")
        return []

def programmatically_find_untested_pub_fns():
    findings = []
    files_to_check = get_modified_files()
    try:
        grep_pub = subprocess.check_output(["grep", "-rn", "pub fn", "mythrax-core/src"], text=True)
        test_content = {}
        for root, _, files in os.walk("mythrax-core/src"):
            for file in files:
                if file.endswith(".rs"):
                    full_path = os.path.join(root, file)
                    with open(full_path, "r") as f:
                        test_content[full_path] = f.read()
        for root, _, files in os.walk("evals"):
            for file in files:
                if file.endswith(".rs") or file.endswith(".py"):
                    full_path = os.path.join(root, file)
                    with open(full_path, "r") as f:
                        test_content[full_path] = f.read()

        for line in grep_pub.splitlines():
            match = re.match(r"([^:]+):(\d+):(.*pub fn (\w+).*)", line)
            if match:
                file_path, line_num, code, fn_name = match.groups()

                if file_path not in files_to_check and not os.environ.get("GITHUB_EVENT_NAME") == "schedule":
                    continue

                if "test" in file_path or "eval" in file_path:
                    continue
                found = False
                for tpath, tcontent in test_content.items():
                    if "test" in tpath or "eval" in tpath or "#[test]" in tcontent:
                        if fn_name in tcontent:
                            found = True
                            break
                if not found:
                    findings.append({
                        "title": f"Test coverage gap: untested public function `{fn_name}`",
                        "body": f"**File:** `{file_path}`\n**Line:** {line_num}\n\n**Scenario:** The public function `{fn_name}` lacks corresponding tests in the suite.\n\n**Severity:** Medium\n\n**Suggested Fix:** Add a unit test for `{fn_name}`.\n\n**Code Snippet:**\n```rust\n{code.strip()}\n```",
                        "labels": ["bug", "agent-found"]
                    })
    except subprocess.CalledProcessError:
        pass
    return findings

def get_existing_issues():
    token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        return []

    all_issues = []
    page = 1

    while True:
        url = f"https://api.github.com/repos/{repo}/issues?state=open&per_page=100&page={page}"
        headers = {
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28"
        }

        req = urllib.request.Request(url, headers=headers)

        try:
            with urllib.request.urlopen(req) as response:
                issues = json.loads(response.read().decode('utf-8'))
                if not issues:
                    break
                all_issues.extend([issue["title"] for issue in issues])
                page += 1
        except urllib.error.URLError as e:
            print(f"Failed to fetch existing issues: {e}")
            break

    return all_issues

def create_github_issue(finding, existing_issues):
    token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")

    if finding["title"] in existing_issues:
        print(f"Issue already exists, skipping: {finding['title']}")
        return

    if not token or not repo:
        print(f"Would create issue: {finding['title']}\n{finding['body']}\nLabels: {finding.get('labels', [])}")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28"
    }

    data = {
        "title": finding["title"],
        "body": finding["body"],
        "labels": finding.get("labels", ["bug", "agent-found"])
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)

    try:
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Successfully created issue: {finding['title']}")
                existing_issues.append(finding["title"])
            else:
                print(f"Failed to create issue: {response.status}")
    except urllib.error.URLError as e:
        print(f"Failed to create issue: {e}")

def main():
    system_prompt = get_config()
    print("CTO Agent System Prompt Loaded.")

    files_to_analyze = get_modified_files()
    print(f"Analyzing {len(files_to_analyze)} files...")

    existing_issues = get_existing_issues()
    all_findings = []

    for file_path in files_to_analyze:
        try:
            with open(file_path, "r") as f:
                content = f.read()
        except Exception as e:
            print(f"Failed to read {file_path}: {e}")
            continue

        chunks = chunk_content(content)
        for i, chunk in enumerate(chunks):
            print(f"Analyzing {file_path} chunk {i+1}/{len(chunks)}...")
            findings = analyze_chunk_with_llm(system_prompt, file_path, chunk)
            all_findings.extend(findings)

    print("\nScanning for test coverage gaps...")
    coverage_findings = programmatically_find_untested_pub_fns()
    all_findings.extend(coverage_findings)

    print(f"\nAnalysis complete. Found {len(all_findings)} issues.")

    for finding in all_findings:
        print(f"Finding: {finding['title']}")
        print(finding['body'])
        print("-" * 60)
        create_github_issue(finding, existing_issues)

if __name__ == "__main__":
    main()
