import os
import json
import urllib.request
import subprocess

def get_modified_files():
    base_ref = os.environ.get("GITHUB_BASE_REF")
    if not base_ref:
        return []
    subprocess.run(["git", "fetch", "--unshallow"], stderr=subprocess.DEVNULL)
    subprocess.run(["git", "fetch", "origin", base_ref], stderr=subprocess.DEVNULL)
    res = subprocess.run(["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"], capture_output=True, text=True)
    return [f for f in res.stdout.splitlines() if f.strip()]

def get_files_in_dir(directories):
    files = []
    for d in directories:
        for root, _, filenames in os.walk(d):
            for filename in filenames:
                if filename.endswith(".rs") or filename.endswith(".py") or filename.endswith(".md"):
                    files.append(os.path.join(root, filename))
    return files

def chunk_files(files, max_chars=40000):
    chunks = []
    current_chunk = ""
    for f in files:
        if not os.path.exists(f): continue
        try:
            with open(f, "r", encoding="utf-8") as file:
                lines = file.readlines()
                content = f"--- FILE: {f} ---\n"
                for i, line in enumerate(lines, 1):
                    content += f"{i}:{line}"
                if len(current_chunk) + len(content) > max_chars and current_chunk:
                    chunks.append(current_chunk)
                    current_chunk = ""
                current_chunk += content + "\n"
        except Exception:
            pass
    if current_chunk:
        chunks.append(current_chunk)
    return chunks

def call_llm(system_prompt, user_content):
    api_key = os.environ.get("OPENAI_API_KEY", "")
    if not api_key:
        print("No OPENAI_API_KEY found, returning empty findings.")
        return {"findings": []}

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    data = {
        "model": "gpt-4-turbo",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_content}
        ],
        "response_format": {"type": "json_object"}
    }
    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode())
            return json.loads(result["choices"][0]["message"]["content"])
    except Exception as e:
        print(f"Error calling LLM: {e}")
        return {"findings": []}

def create_github_issue(finding):
    token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print("Missing GH_TOKEN or GITHUB_REPOSITORY, skipping issue creation.")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json"
    }
    title = f"CTO Review Finding in {finding.get('file', 'unknown')}"
    body = (
        f"**File:** {finding.get('file')}:{finding.get('line_number')}\n"
        f"**Severity:** {finding.get('severity')}\n\n"
        f"**Scenario:**\n{finding.get('scenario')}\n\n"
        f"**Suggested Fix:**\n{finding.get('suggested_fix')}"
    )
    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }
    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
    try:
        urllib.request.urlopen(req)
    except Exception as e:
        print(f"Error creating issue: {e}")

def main():
    with open(".agents/cto_agent/config.json", "r") as f:
        config = json.load(f)

    system_prompt = "\n".join(config['config']['customAgent']['systemPromptSections'])
    event_name = os.environ.get("GITHUB_EVENT_NAME", "schedule")
    directories = ["mythrax-core", "evals", ".agents"]
    all_files = get_files_in_dir(directories)

    if event_name == "pull_request":
        modified = get_modified_files()
        files_to_analyze = [f for f in all_files if f in modified]
        if not files_to_analyze:
            print("No relevant files modified in this PR.")
            return
    else:
        files_to_analyze = all_files

    pub_fn_grep = subprocess.run(["grep", "-rn", "pub fn", "mythrax-core/src/"], capture_output=True, text=True).stdout

    chunks = chunk_files(files_to_analyze)
    all_findings = []

    for chunk in chunks:
        user_content = f"List of public functions:\n{pub_fn_grep}\n\nFiles to review:\n{chunk}"
        res = call_llm(system_prompt, user_content)
        all_findings.extend(res.get("findings", []))

    for finding in all_findings:
        print(f"FINDING DETAILED REPORT:\n{json.dumps(finding, indent=2)}")
        if finding.get("realistic_trigger_path", True):
            create_github_issue(finding)

if __name__ == "__main__":
    main()
