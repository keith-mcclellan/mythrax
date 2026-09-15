import os
import sys
import json
import urllib.request
import subprocess

def run_cmd(cmd):
    try:
        return subprocess.check_output(cmd, shell=True, text=True, stderr=subprocess.STDOUT).strip()
    except subprocess.CalledProcessError:
        return ""

def get_modified_files(event_name):
    files = set()
    if event_name == "pull_request":
        # using git diff --name-only HEAD^1 HEAD as per instructions
        diff_output = run_cmd("git diff --name-only HEAD^1 HEAD")
        for f in diff_output.splitlines():
            if f.strip() and os.path.exists(f.strip()):
                files.add(f.strip())
    else:
        # scheduled: get all files in mythrax-core, evals, and .agents
        directories = ["mythrax-core", "evals", ".agents"]
        for d in directories:
            if os.path.exists(d):
                all_files = run_cmd(f"find {d} -type f -name '*.rs' -o -name '*.py' -o -name '*.md'").splitlines()
                for f in all_files:
                    if f.strip() and os.path.exists(f.strip()):
                        files.add(f.strip())

    return list(files)

def get_existing_issues(repo, token):
    url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100"
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json"
    })
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode())
            return [issue["title"] for issue in issues]
    except Exception as e:
        print(f"Error fetching issues: {e}")
        return []

def create_issue(repo, token, title, body):
    url = f"https://api.github.com/repos/{repo}/issues"
    data = json.dumps({
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }).encode("utf-8")

    req = urllib.request.Request(url, data=data, headers={
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    })

    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {title}")
    except Exception as e:
        print(f"Error creating issue: {e}")

def call_llm(prompt, api_key):
    url = "https://api.openai.com/v1/chat/completions"
    data = json.dumps({
        "model": "gpt-4-turbo",
        "messages": [
            {"role": "system", "content": "You are an Adversarial CTO. Find logic bugs, edge cases, off-by-one errors, panic paths, integer overflows, race conditions, silent logic failures, and test coverage gaps. For each bug, output a JSON object with 'title', 'file', 'line', 'scenario', 'severity', 'fix'."},
            {"role": "user", "content": prompt}
        ],
        "response_format": {"type": "json_object"}
    }).encode("utf-8")

    req = urllib.request.Request(url, data=data, headers={
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json"
    })

    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode())
            return json.loads(result["choices"][0]["message"]["content"])
    except Exception as e:
        print(f"Error calling LLM: {e}")
        return {"bugs": []}

def process_in_chunks(files, repo, token, api_key, existing_issues):
    # A simple chunking strategy based on a rough character count (approx 10,000 characters per chunk)
    MAX_CHARS_PER_CHUNK = 20000
    current_chunk = ""
    current_chunk_size = 0

    for f in files:
        try:
            with open(f, "r") as file:
                content = file.read()
                file_text = f"\n--- {f} ---\n{content}\n"

                if current_chunk_size + len(file_text) > MAX_CHARS_PER_CHUNK and current_chunk_size > 0:
                    # process the current chunk
                    prompt = f"Analyze the following codebase files for bugs based on the instructions. Output a JSON object with a 'bugs' key containing a list of objects.\n\n{current_chunk}"
                    llm_result = call_llm(prompt, api_key)
                    process_llm_result(llm_result, repo, token, existing_issues)

                    # reset chunk
                    current_chunk = ""
                    current_chunk_size = 0

                current_chunk += file_text
                current_chunk_size += len(file_text)
        except Exception:
            pass

    # process any remaining chunk
    if current_chunk_size > 0:
        prompt = f"Analyze the following codebase files for bugs based on the instructions. Output a JSON object with a 'bugs' key containing a list of objects.\n\n{current_chunk}"
        llm_result = call_llm(prompt, api_key)
        process_llm_result(llm_result, repo, token, existing_issues)

def process_llm_result(llm_result, repo, token, existing_issues):
    for bug in llm_result.get("bugs", []):
        title = f"Adversarial CTO Finding: {bug.get('title', 'Bug')}"
        if title in existing_issues:
            print(f"Skipping duplicate issue: {title}")
            continue

        body = f"**File:** {bug.get('file')} (Line {bug.get('line')})\n**Severity:** {bug.get('severity')}\n\n### Minimal Reproducible Scenario\n{bug.get('scenario')}\n\n### Suggested Fix\n{bug.get('fix')}\n\n---\n*Note: Do not close this issue without a documented architectural decision record (ADR).*\n"
        create_issue(repo, token, title, body)
        existing_issues.append(title) # Prevent duplicates within the same run

def main():
    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")
    api_key = os.environ.get("OPENAI_API_KEY")
    event_name = os.environ.get("GITHUB_EVENT_NAME")

    if not all([repo, token, api_key]):
        print("Missing required environment variables")
        return

    files_to_analyze = get_modified_files(event_name)
    print(f"Analyzing {len(files_to_analyze)} files")

    existing_issues = get_existing_issues(repo, token)
    process_in_chunks(files_to_analyze, repo, token, api_key, existing_issues)

if __name__ == "__main__":
    main()
