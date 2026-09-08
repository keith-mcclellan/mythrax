import os
import sys
import json
import urllib.request
import urllib.error
import subprocess

def get_modified_files():
    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name in ("push", "pull_request"):
        try:
            # In PRs or pushes, diff against the appropriate base.
            # For simplicity in this script, we'll diff HEAD against HEAD~1,
            # or if GITHUB_BASE_REF is available, diff against origin/base.
            base = os.environ.get("GITHUB_BASE_REF")
            if base:
                subprocess.run(["git", "fetch", "origin", base], check=True, capture_output=True)
                cmd = ["git", "diff", "--name-only", f"origin/{base}...HEAD"]
            else:
                cmd = ["git", "diff", "--name-only", "HEAD~1...HEAD"]

            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
            return [f for f in result.stdout.strip().split('\n') if f]
        except subprocess.CalledProcessError:
            return []
    else: # schedule
        # For scheduled runs, we consider key directories
        files = []
        for d in ["mythrax-core/src", "evals", ".agents"]:
            if os.path.exists(d):
                for root, _, filenames in os.walk(d):
                    for f in filenames:
                        files.append(os.path.join(root, f))
        return files

def get_file_content(filepath):
    try:
        with open(filepath, 'r') as f:
            return f.read()
    except Exception:
        return ""

def analyze_with_llm(context_str):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found. Skipping LLM analysis gracefully.")
        sys.exit(0)

    prompt = f"""
    You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax.
    You have no loyalty to the current design. You have read ARCHITECTURE.md and mock_audit_report.md in full,
    and your job is to find where the architecture will fail under load, under adversarial inputs, or under changing requirements.
    You are not here to validate — you are here to stress-fracture.

    Analyze the following files and provide a JSON array of findings.

    For architectural findings, challenge assumptions, identify single points of failure, evaluate agent orchestration,
    assess the evals framework, identify coupling, and project 18 months forward.
    Format each finding strictly as a Red Team Architecture Brief:
    {{
        "type": "architecture",
        "title": "Short Descriptive Title",
        "finding": "...",
        "current_assumption": "...",
        "attack_scenario": "...",
        "blast_radius": "...",
        "recommended_structural_change": "..."
    }}

    For code-level subtle bugs (off-by-one, panic paths, integer overflows, race conditions):
    {{
        "type": "bug",
        "title": "Short Descriptive Title",
        "file": "path/to/file",
        "line": "approximate line or context",
        "minimal_reproducible_scenario": "...",
        "severity": "...",
        "suggested_fix": "..."
    }}

    Context files:
    {context_str}

    Return ONLY a valid JSON array.
    """

    data = {
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.2
    }
    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=json.dumps(data).encode('utf-8'),
        headers={"Content-Type": "application/json", "Authorization": f"Bearer {api_key}"}
    )
    try:
        with urllib.request.urlopen(req) as response:
            res_body = response.read()
            res_json = json.loads(res_body)
            content = res_json["choices"][0]["message"]["content"]
            # strip markdown code blocks if present
            if content.startswith("```json"):
                content = content[7:-3]
            elif content.startswith("```"):
                content = content[3:-3]
            return json.loads(content.strip())
    except Exception as e:
        print(f"Error calling LLM: {e}")
        return []

def get_existing_issues():
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        return []

    url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100"
    req = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}", "Accept": "application/vnd.github+json"})
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read())
            return [issue.get("title") for issue in issues]
    except Exception as e:
        print(f"Error fetching issues: {e}")
        return []

def create_issue(finding):
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print("Missing GitHub token/repo, cannot file issue.")
        return

    title = finding.get("title", "Adversarial Finding")

    if finding.get("type") == "architecture":
        labels = ["architecture-review", "adversarial"]
        body = f"## Red Team Architecture Brief\n\n"
        body += f"**Finding:** {finding.get('finding')}\n\n"
        body += f"**Current Assumption:** {finding.get('current_assumption')}\n\n"
        body += f"**Attack Scenario:** {finding.get('attack_scenario')}\n\n"
        body += f"**Blast Radius:** {finding.get('blast_radius')}\n\n"
        body += f"**Recommended Structural Change:** {finding.get('recommended_structural_change')}\n\n"
        body += "---\n*Note: Do not close this issue without a documented architectural decision record (ADR) response.*\n"
    else:
        labels = ["bug", "agent-found"]
        body = f"## Adversarial Code Bug\n\n"
        body += f"**File:** {finding.get('file')} (Line: {finding.get('line')})\n\n"
        body += f"**Minimal Reproducible Scenario:** {finding.get('minimal_reproducible_scenario')}\n\n"
        body += f"**Severity:** {finding.get('severity')}\n\n"
        body += f"**Suggested Fix:** {finding.get('suggested_fix')}\n"

    data = {
        "title": f"Adversarial CTO: {title}",
        "body": body,
        "labels": labels
    }

    req = urllib.request.Request(
        f"https://api.github.com/repos/{repo}/issues",
        data=json.dumps(data).encode("utf-8"),
        headers={"Authorization": f"Bearer {token}", "Accept": "application/vnd.github+json", "Content-Type": "application/json"}
    )
    try:
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Successfully created issue: {title}")
            else:
                print(f"Failed to create issue: {title}")
    except Exception as e:
        print(f"Error creating issue {title}: {e}")

def main():
    modified_files = get_modified_files()
    # Always include core context
    context_files = {"ARCHITECTURE.md", "mock_audit_report.md"}
    context_files.update(modified_files)

    context_str = ""
    for filepath in context_files:
        if os.path.exists(filepath):
            content = get_file_content(filepath)
            # limit content size to avoid context bloat for huge files if necessary,
            # but for now include it all.
            context_str += f"\n--- FILE: {filepath} ---\n{content[:10000]}\n"

    if not context_str:
        print("No relevant context found.")
        return

    findings = analyze_with_llm(context_str)
    existing_titles = get_existing_issues()

    for finding in findings:
        title = f"Adversarial CTO: {finding.get('title', 'Finding')}"
        if title not in existing_titles:
            create_issue(finding)
        else:
            print(f"Issue already exists: {title}")

if __name__ == '__main__':
    main()
