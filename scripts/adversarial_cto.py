import os
import sys
import json
import urllib.request
import urllib.parse
import urllib.error
import subprocess

def get_modified_files():
    try:
        result = subprocess.run(['git', 'diff', '--name-only', 'HEAD^1', 'HEAD'], capture_output=True, text=True, check=True)
        return [f for f in result.stdout.split('\n') if f.strip()]
    except Exception as e:
        print(f"Error getting modified files: {e}")
        return []

def get_all_files():
    files = []
    for d in ['mythrax-core', 'evals', '.agents']:
        for root, _, filenames in os.walk(d):
            for filename in filenames:
                if filename.endswith(('.rs', '.py', '.json', '.toml', '.md')):
                    files.append(os.path.join(root, filename))
    return files

def read_file_content(path):
    if not os.path.exists(path):
        return None
    try:
        with open(path, 'r', encoding='utf-8') as f:
            return f.read()
    except Exception:
        return None

def analyze_with_openai(files_content_chunk, api_key):
    prompt = """You are an Adversarial CTO who has been burned by production incidents traced to subtle bugs that code review missed.
You are methodical and assume the code is wrong until proven otherwise.
Analyze the provided codebase files for logic bugs, edge case vulnerabilities, and correctness issues:
1. Off-by-one errors in any loop, index, or range operation — pay special attention to the AI agent scaffolding and context window management in .agents/
2. Panic paths: any .unwrap() or .expect() that could be triggered by realistic runtime inputs, especially those processing external data or LLM responses.
3. Integer overflow or underflow risks in any arithmetic involving agent scoring, token counting, or eval metrics.
4. Race conditions or incorrect assumptions about execution order in any async or concurrent code paths.
5. Logic branches where the code silently succeeds but produces an incorrect result (false positives worse than panics).
6. Test coverage gaps — identify any public functions in mythrax-core with no corresponding test in the eval or test suite.

For each bug found, provide:
- file: the file path
- line_number: the approximate line number or function name
- scenario: a minimal reproducible scenario
- severity: (Critical, High, Medium, Low)
- suggested_fix: a suggested fix

Return a JSON object with a single key "bugs" containing a list of these bug objects."""

    payload = {
        "model": "gpt-4-turbo",
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": "Here are the files:\n\n" + files_content_chunk}
        ],
        "response_format": {"type": "json_object"}
    }

    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=json.dumps(payload).encode('utf-8'),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        },
        method="POST"
    )

    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            return json.loads(result['choices'][0]['message']['content']).get("bugs", [])
    except urllib.error.URLError as e:
        print(f"OpenAI API Error: {e}")
        if hasattr(e, 'read'):
            print(e.read().decode('utf-8'))
        return []

def chunk_files(files, max_tokens=100000):
    chunks = []
    current_chunk = ""
    for file in files:
        content = read_file_content(file)
        if content is None:
            continue
        file_text = f"--- FILE: {file} ---\n{content}\n\n"
        if len(current_chunk) + len(file_text) > max_tokens * 4: # rough approximation
            if current_chunk:
                chunks.append(current_chunk)
            current_chunk = file_text
        else:
            current_chunk += file_text
    if current_chunk:
        chunks.append(current_chunk)
    return chunks

def get_existing_issues(repo, token):
    issues = []
    page = 1
    while True:
        req = urllib.request.Request(
            f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100&page={page}",
            headers={
                "Accept": "application/vnd.github.v3+json",
                "Authorization": f"token {token}",
                "User-Agent": "Adversarial-CTO-Script"
            }
        )
        try:
            with urllib.request.urlopen(req) as response:
                page_issues = json.loads(response.read().decode('utf-8'))
                if not page_issues:
                    break
                issues.extend(page_issues)
                page += 1
        except Exception as e:
            print(f"Error fetching issues: {e}")
            break
    return issues

def create_issue(repo, token, bug, existing_issues):
    title = f"Adversarial CTO Finding: {bug['severity']} bug in {bug['file']}"

    # Idempotency check
    for issue in existing_issues:
        if issue['title'] == title or (bug['file'] in issue['title'] and bug['severity'] in issue['title']):
            print(f"Issue already exists for {bug['file']}")
            return

    body = f"""**Severity:** {bug['severity']}
**File:** `{bug['file']}` (Line: {bug['line_number']})

**Minimal Reproducible Scenario:**
{bug['scenario']}

**Suggested Fix:**
{bug['suggested_fix']}

*Explicit Note: Never close this issue without a documented architectural decision record (ADR).*
"""

    payload = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    req = urllib.request.Request(
        f"https://api.github.com/repos/{repo}/issues",
        data=json.dumps(payload).encode('utf-8'),
        headers={
            "Accept": "application/vnd.github.v3+json",
            "Authorization": f"token {token}",
            "User-Agent": "Adversarial-CTO-Script"
        },
        method="POST"
    )

    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue for {bug['file']}")
    except Exception as e:
        print(f"Error creating issue: {e}")
        if hasattr(e, 'read'):
            print(e.read().decode('utf-8'))

def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    github_token = os.environ.get("GITHUB_TOKEN")
    github_repository = os.environ.get("GITHUB_REPOSITORY")
    event_name = os.environ.get("GITHUB_EVENT_NAME")

    if not api_key:
        print("OPENAI_API_KEY not set")
        return

    if event_name == "schedule":
        files_to_analyze = get_all_files()
    else:
        files_to_analyze = get_modified_files()
        # Always include core context files
        for core_file in ["ARCHITECTURE.md", "mock_audit_report.md"]:
            if core_file not in files_to_analyze:
                files_to_analyze.append(core_file)

    # Filter to only relevant files that exist
    files_to_analyze = [f for f in files_to_analyze if os.path.exists(f)]

    if not files_to_analyze:
        print("No files to analyze.")
        return

    print(f"Analyzing {len(files_to_analyze)} files...")
    chunks = chunk_files(files_to_analyze)

    all_bugs = []
    for chunk in chunks:
        bugs = analyze_with_openai(chunk, api_key)
        all_bugs.extend(bugs)

    print(f"Found {len(all_bugs)} bugs.")

    if github_token and github_repository:
        existing_issues = get_existing_issues(github_repository, github_token)
        for bug in all_bugs:
            create_issue(github_repository, github_token, bug, existing_issues)
    else:
        print("GitHub token or repository not set, skipping issue creation.")
        print(json.dumps(all_bugs, indent=2))

if __name__ == "__main__":
    main()
