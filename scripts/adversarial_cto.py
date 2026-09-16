import os
import json
import urllib.request
import subprocess

def get_modified_files():
    try:
        # Fetching modified files for push events
        result = subprocess.run(
            ["git", "diff", "--name-only", "HEAD^1", "HEAD"],
            capture_output=True,
            text=True,
            check=True
        )
        return [line.strip() for line in result.stdout.strip().split('\n') if line.strip()]
    except Exception as e:
        print(f"Failed to get modified files: {e}")
        return []

def get_all_target_files():
    targets = []
    for root, dirs, files in os.walk("."):
        if ".git" in root or "target" in root or "node_modules" in root:
            continue
        for file in files:
            path = os.path.join(root, file).lstrip("./")
            if path.startswith("mythrax-core/src/") or path.startswith(".agents/") or path.startswith("evals/"):
                targets.append(path)
    return targets

def chunk_files(files, max_bytes=50000):
    chunks = []
    current_chunk = []
    current_size = 0
    for file in files:
        if not os.path.isfile(file):
            continue
        try:
            size = os.path.getsize(file)
            if current_size + size > max_bytes and current_chunk:
                chunks.append(current_chunk)
                current_chunk = []
                current_size = 0
            current_chunk.append(file)
            current_size += size
        except Exception:
            pass
    if current_chunk:
        chunks.append(current_chunk)
    return chunks

def call_openai(system_prompt, file_contents):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not set")
        return None

    content = "Please analyze the following files based on your system instructions:\n\n"
    for k, v in file_contents.items():
        # Limit individual file size to avoid too many tokens
        content += f"--- {k} ---\n{v[:15000]}\n\n"

    data = {
        "model": "gpt-4-turbo",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": content}
        ],
        "response_format": {"type": "json_object"}
    }

    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=json.dumps(data).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        }
    )

    try:
        with urllib.request.urlopen(req) as response:
            res_data = json.loads(response.read().decode("utf-8"))
            return json.loads(res_data["choices"][0]["message"]["content"])
    except Exception as e:
        print(f"OpenAI API call failed: {e}")
        return None

def fetch_existing_issues(repo, token):
    url = f"https://api.github.com/repos/{repo}/issues?state=all&labels=architecture-review,adversarial&per_page=100"
    req = urllib.request.Request(
        url,
        headers={
            "Authorization": f"token {token}",
            "Accept": "application/vnd.github.v3+json"
        }
    )
    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read().decode("utf-8"))
    except Exception as e:
        print(f"Failed to fetch issues: {e}")
        return []

def create_github_issue(repo, token, title, body):
    url = f"https://api.github.com/repos/{repo}/issues"
    data = {
        "title": title,
        "body": body,
        "labels": ["architecture-review", "adversarial"]
    }
    req = urllib.request.Request(
        url,
        data=json.dumps(data).encode("utf-8"),
        headers={
            "Authorization": f"token {token}",
            "Accept": "application/vnd.github.v3+json"
        }
    )
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {title}")
    except Exception as e:
        print(f"Failed to create issue {title}: {e}")

def main():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "schedule")
    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")

    if not repo or not token:
        print("Missing GITHUB_REPOSITORY or GITHUB_TOKEN")
        return

    files_to_scan = ["ARCHITECTURE.md", "mock_audit_report.md"]
    if event_name == "push":
        mod_files = get_modified_files()
        for f in mod_files:
            if f.startswith("mythrax-core/src/") or f.startswith(".agents/") or f.startswith("evals/"):
                if f not in files_to_scan:
                    files_to_scan.append(f)
    else:
        for f in get_all_target_files():
            if f not in files_to_scan:
                files_to_scan.append(f)

    existing_issues = fetch_existing_issues(repo, token)
    existing_titles = set(i["title"] for i in existing_issues)

    chunks = chunk_files(files_to_scan, max_bytes=80000)

    system_prompt = (
        "You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax. "
        "You have no loyalty to the current design. You are not here to validate — you are here to stress-fracture.\n\n"
        "Tasks:\n"
        "1. Challenge every documented architectural decision — ask 'what assumption does this break if it’s wrong?'\n"
        "2. Identify single points of failure: any module, interface, or data path where failure has no graceful degradation path\n"
        "3. Evaluate the agent orchestration design for prompt injection vulnerabilities, unbounded recursion risk, and failure to enforce agent scope boundaries\n"
        "4. Assess whether the eval framework actually tests adversarial inputs\n"
        "5. Identify coupling: any two modules that cannot be independently deployed, tested, or replaced without modifying both\n"
        "6. Project the architecture 18 months forward: name the top 3 decisions made today that will become re-architecture projects if the system scales 10x\n\n"
        "Return a JSON object with a 'findings' list. Each finding must have:\n"
        "- 'title': A short, descriptive title for the GitHub Issue\n"
        "- 'finding': The issue discovered\n"
        "- 'current_assumption': The current assumption being challenged\n"
        "- 'attack_scenario': The attack scenario or failure mode\n"
        "- 'blast_radius': The impact blast radius\n"
        "- 'recommended_structural_change': The recommended structural change"
    )

    for chunk in chunks:
        file_contents = {}
        for f in chunk:
            if os.path.exists(f):
                with open(f, "r") as file:
                    file_contents[f] = file.read()

        result = call_openai(system_prompt, file_contents)
        if result and "findings" in result:
            for finding in result["findings"]:
                title = finding.get("title")
                if not title or title in existing_titles:
                    continue

                body = (
                    "### Red Team Architecture Brief\n\n"
                    f"**Finding:** {finding.get('finding')}\n\n"
                    f"**Current Assumption:** {finding.get('current_assumption')}\n\n"
                    f"**Attack Scenario:** {finding.get('attack_scenario')}\n\n"
                    f"**Blast Radius:** {finding.get('blast_radius')}\n\n"
                    f"**Recommended Structural Change:** {finding.get('recommended_structural_change')}\n\n"
                    "---\n"
                    "*Note: Never close these issues without a documented architectural decision record (ADR) response.*"
                )

                create_github_issue(repo, token, title, body)
                existing_titles.add(title)

if __name__ == "__main__":
    main()
