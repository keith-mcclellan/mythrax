import os
import json
import urllib.request
import subprocess
import sys

def get_modified_files():
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "pull_request":
        # The easiest and most reliable way in GitHub Actions is to use the API
        # but since we are running in the context of the PR merge commit locally
        # we can just diff the merge commit against its first parent
        try:
            output = subprocess.check_output(["git", "diff", "--name-only", "HEAD^1", "HEAD"])
            return [line.strip() for line in output.decode("utf-8").split("\n") if line.strip()]
        except Exception as e:
            print(f"Error getting modified files: {e}")

            # Fallback to API if git diff fails
            repo = os.environ.get("GITHUB_REPOSITORY")
            token = os.environ.get("GITHUB_TOKEN")

            if repo and token:
                try:
                    # We need the PR number to use the files API directly
                    # A quick way to get the PR number for the current commit
                    commit_sha = os.environ.get("GITHUB_SHA")
                    url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
                    headers = {
                        "Authorization": f"Bearer {token}",
                        "Accept": "application/vnd.github+json",
                        "User-Agent": "Adversarial-CTO"
                    }
                    req = urllib.request.Request(url, headers=headers)
                    response = urllib.request.urlopen(req)
                    pulls = json.loads(response.read().decode("utf-8"))

                    if pulls:
                        pr_number = pulls[0]["number"]
                        files_url = f"https://api.github.com/repos/{repo}/pulls/{pr_number}/files"
                        files_req = urllib.request.Request(files_url, headers=headers)
                        files_response = urllib.request.urlopen(files_req)
                        files_data = json.loads(files_response.read().decode("utf-8"))
                        return [f["filename"] for f in files_data]
                except Exception as api_e:
                    print(f"Error falling back to API: {api_e}")

    return []

def get_all_files():
    files = []
    for d in ["mythrax-core/src", "evals", ".agents"]:
        if os.path.exists(d):
            for root, _, filenames in os.walk(d):
                for f in filenames:
                    if f.endswith(".rs") or f.endswith(".md") or f.endswith(".py"):
                        files.append(os.path.join(root, f))
    return files

def check_file(file_path):
    if not os.path.exists(file_path):
        return None
    try:
        with open(file_path, "r", encoding="utf-8") as f:
            return f.read()
    except Exception:
        return None

def analyze_with_llm(files_content):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found.")
        return []

    system_prompt = """You are a CTO who has been burned by production incidents traced to subtle bugs that code review missed. You are methodical and assume the code is wrong until proven otherwise.
Task: Analyze the provided code for logic bugs, edge case vulnerabilities, and correctness issues:
• Off-by-one errors in any loop, index, or range operation — pay special attention to the AI agent scaffolding and context window management in .agents/
• Panic paths: any .unwrap() or .expect() that could be triggered by realistic runtime inputs, especially those processing external data or LLM responses
• Integer overflow or underflow risks in any arithmetic involving agent scoring, token counting, or eval metrics
• Race conditions or incorrect assumptions about execution order in any async or concurrent code paths
• Logic branches where the code silently succeeds but produces an incorrect result (false positives worse than panics)
• Test coverage gaps — identify any public functions in mythrax-core with no corresponding test in the eval or test suite

Format your output exactly as a JSON list of objects. Do not include markdown formatting or backticks around the output.
Each object must have the following string or integer values:
- "file": string
- "line": integer
- "bug_type": string
- "minimal_reproducible_scenario": string
- "severity": string
- "suggested_fix": string"""

    chunks = []
    current_chunk = {}
    current_size = 0
    for path, content in files_content.items():
        size = len(content)
        if current_size + size > 200000 and current_chunk:
            chunks.append(current_chunk)
            current_chunk = {}
            current_size = 0
        current_chunk[path] = content
        current_size += size
    if current_chunk:
        chunks.append(current_chunk)

    all_bugs = []
    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    for i, chunk in enumerate(chunks):
        user_content = f"Files to analyze (Part {i+1}/{len(chunks)}):\n"
        for path, content in chunk.items():
            user_content += f"\n--- {path} ---\n{content}\n"

        data = {
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_content}
            ],
            "temperature": 0.2
        }

        req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
        try:
            response = urllib.request.urlopen(req)
            response_data = json.loads(response.read().decode("utf-8"))
            content_str = response_data["choices"][0]["message"]["content"].strip()

            if content_str.startswith("```json"):
                content_str = content_str[7:]
            elif content_str.startswith("```"):
                content_str = content_str[3:]
            if content_str.endswith("```"):
                content_str = content_str[:-3]

            bugs = json.loads(content_str.strip())
            if isinstance(bugs, list):
                all_bugs.extend(bugs)
        except Exception as e:
            print(f"LLM analysis failed for chunk {i+1}: {e}")

    return all_bugs

def get_existing_issues():
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        return []

    issues = []
    page = 1
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json",
        "User-Agent": "Adversarial-CTO"
    }

    while True:
        url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100&page={page}"
        try:
            req = urllib.request.Request(url, headers=headers)
            response = urllib.request.urlopen(req)
            page_issues = json.loads(response.read().decode("utf-8"))

            if not page_issues:
                break

            issues.extend([issue["title"] for issue in page_issues])

            # Check if there are more pages based on the Link header
            link_header = response.getheader('Link')
            if link_header and 'rel="next"' in link_header:
                page += 1
            else:
                break

        except Exception as e:
            print(f"Failed to fetch issues: {e}")
            break

    return issues

def create_issue(bug, existing_titles):
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        return

    title = f"Adversarial CTO: {bug.get('bug_type', 'Bug')} in {bug.get('file', 'Unknown')}"

    if any(title in ext_title for ext_title in existing_titles):
        print(f"Issue already exists (or similar): {title}")
        return

    body = f"**File:** {bug.get('file')} (Line {bug.get('line')})\n"
    body += f"**Severity:** {bug.get('severity')}\n"
    body += f"**Bug Type:** {bug.get('bug_type')}\n\n"
    body += f"**Minimal Reproducible Scenario:**\n{bug.get('minimal_reproducible_scenario')}\n\n"
    body += f"**Suggested Fix:**\n{bug.get('suggested_fix')}\n"

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json",
        "User-Agent": "Adversarial-CTO",
        "Content-Type": "application/json"
    }
    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }
    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
        urllib.request.urlopen(req)
        print(f"Created issue: {title}")
        existing_titles.append(title)
    except Exception as e:
        print(f"Failed to create issue {title}: {e}")

def main():
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "schedule":
        files_to_analyze = get_all_files()
    else:
        files_to_analyze = get_modified_files()

    core_files = ["ARCHITECTURE.md"]
    for f in core_files:
        if f not in files_to_analyze:
            files_to_analyze.append(f)

    files_content = {}
    for f in files_to_analyze:
        content = check_file(f)
        if content:
            files_content[f] = content

    if not files_content:
        print("No files to analyze.")
        return

    # In a chunked run, files could be split. While it can cause false positives for tests,
    # given the strict boundaries of 200,000 characters we must segment it.
    print(f"Analyzing {len(files_content)} files...")
    bugs = analyze_with_llm(files_content)

    if not bugs:
        print("No bugs found or analysis failed.")
        return

    existing_titles = get_existing_issues()
    for bug in bugs:
        create_issue(bug, existing_titles)

if __name__ == "__main__":
    main()
