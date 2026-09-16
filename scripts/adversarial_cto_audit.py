import os
import json
import urllib.request
import urllib.error
import subprocess

OPENAI_API_KEY = os.environ.get("OPENAI_API_KEY")
GITHUB_TOKEN = os.environ.get("GITHUB_TOKEN")
GITHUB_REPOSITORY = os.environ.get("GITHUB_REPOSITORY")
EVENT_NAME = os.environ.get("GITHUB_EVENT_NAME")

def get_modified_files():
    try:
        # Assuming we are in a PR context where actions/checkout fetches HEAD^1
        result = subprocess.run(
            ["git", "diff", "--name-only", "HEAD^1", "HEAD"],
            capture_output=True, text=True, check=True
        )
        files = result.stdout.strip().split('\n')
        return [f for f in files if f]
    except Exception as e:
        print(f"Error getting modified files: {e}")
        return []

def get_all_files():
    directories = ["mythrax-core", ".agents", "evals"]
    files = []
    for d in directories:
        for root, _, filenames in os.walk(d):
            for filename in filenames:
                if filename.endswith(('.rs', '.py', '.md', '.json', '.toml')):
                    files.append(os.path.join(root, filename))
    return files

def get_existing_issues():
    if not GITHUB_TOKEN or not GITHUB_REPOSITORY:
        return []

    url = f"https://api.github.com/repos/{GITHUB_REPOSITORY}/issues?state=open&per_page=100"
    req = urllib.request.Request(url, headers={
        "Authorization": f"token {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json"
    })
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode())
            return [issue['title'] for issue in issues]
    except Exception as e:
        print(f"Failed to fetch existing issues: {e}")
        return []

def create_issue(issue_data):
    if not GITHUB_TOKEN or not GITHUB_REPOSITORY:
        print("Missing GitHub token or repo, cannot create issue.")
        print(json.dumps(issue_data, indent=2))
        return

    title = f"Adversarial CTO: {issue_data['title']}"

    body = f"""**File:** `{issue_data['file']}:{issue_data['line']}`
**Severity:** {issue_data['severity']}

**Minimal Reproducible Scenario:**
{issue_data['scenario']}

**Suggested Fix:**
{issue_data['fix']}

**Note:** Never close this issue without a documented architectural decision record (ADR)."""

    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    url = f"https://api.github.com/repos/{GITHUB_REPOSITORY}/issues"
    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers={
        "Authorization": f"token {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    })
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {title}")
    except Exception as e:
        print(f"Failed to create issue {title}: {e}")

def analyze_chunk(chunk):
    if not OPENAI_API_KEY:
        print("No OpenAI API Key, skipping analysis.")
        return []

    system_prompt = """You are a CTO who has been burned by production incidents traced to subtle bugs that code review missed. You are methodical and assume the code is wrong until proven otherwise. Evaluate architectural decisions by challenging assumptions, identifying single points of failure without graceful degradation, assessing agent orchestration for vulnerabilities (e.g., prompt injection, unbounded recursion, failure to enforce agent scope boundaries), verifying adversarial inputs in evals, flagging module coupling, and projecting 10x scaling limitations 18 months forward.

Analyze the following code for:
- Off-by-one errors in any loop, index, or range operation (especially AI agent scaffolding and context window management in `.agents/`)
- Panic paths: `.unwrap()` or `.expect()` triggered by realistic inputs
- Integer overflow/underflow risks in agent scoring, token counting, eval metrics
- Race conditions or async execution order assumptions
- Logic branches with silent incorrect results
- Test coverage gaps: public functions in `mythrax-core` with no corresponding test

Return JSON with a list of issues:
{
  "issues": [
    {
      "title": "Short descriptive title",
      "file": "filename",
      "line": 123,
      "scenario": "Minimal reproducible scenario",
      "severity": "Critical",
      "fix": "Suggested fix",
      "realistic_trigger": true
    }
  ]
}"""

    data = {
        "model": "gpt-4-turbo",
        "response_format": {"type": "json_object"},
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": f"Analyze this code:\n\n{chunk}"}
        ]
    }

    req = urllib.request.Request("https://api.openai.com/v1/chat/completions", data=json.dumps(data).encode("utf-8"), headers={
        "Authorization": f"Bearer {OPENAI_API_KEY}",
        "Content-Type": "application/json"
    })

    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode())
            content = result['choices'][0]['message']['content']
            return json.loads(content).get('issues', [])
    except Exception as e:
        print(f"Error calling OpenAI API: {e}")
        return []

def main():
    if EVENT_NAME == "schedule":
        files_to_scan = get_all_files()
    else:
        modified = get_modified_files()
        # Keep only files in our target directories
        files_to_scan = [f for f in modified if any(f.startswith(d) for d in ["mythrax-core/", ".agents/", "evals/"])]

        # Always include core context files
        for core_file in ["ARCHITECTURE.md", "mock_audit_report.md"]:
            if os.path.exists(core_file) and core_file not in files_to_scan:
                files_to_scan.append(core_file)

    if not files_to_scan:
        print("No files to scan.")
        return

    print(f"Scanning {len(files_to_scan)} files...")

    existing_issues = get_existing_issues()

    chunk = ""
    chunk_size = 0
    max_chunk_size = 50000 # Approx 12k tokens

    issues_found = []

    for filepath in files_to_scan:
        if not os.path.exists(filepath):
            continue

        try:
            with open(filepath, "r", encoding="utf-8") as f:
                content = f.read()
        except UnicodeDecodeError:
            continue

        file_content = f"\n\n--- FILE: {filepath} ---\n{content}\n"

        if chunk_size + len(file_content) > max_chunk_size and chunk:
            issues_found.extend(analyze_chunk(chunk))
            chunk = ""
            chunk_size = 0

        chunk += file_content
        chunk_size += len(file_content)

    if chunk:
        issues_found.extend(analyze_chunk(chunk))

    for issue in issues_found:
        if not issue.get('realistic_trigger', False):
            continue

        title = f"Adversarial CTO: {issue['title']}"
        if title in existing_issues:
            print(f"Issue already exists: {title}")
            continue

        create_issue(issue)
        existing_issues.append(title) # deduplicate within the same run

if __name__ == "__main__":
    main()
