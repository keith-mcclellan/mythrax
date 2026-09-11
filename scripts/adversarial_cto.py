import os
import subprocess
import json
import urllib.request
import urllib.error

def get_modified_files(before_sha, after_sha):
    try:
        if before_sha == '0000000000000000000000000000000000000000':
            cmd = ["git", "diff", "--name-only", "HEAD~1", "HEAD"]
        else:
            cmd = ["git", "diff", "--name-only", before_sha, after_sha]
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        return [line.strip() for line in result.stdout.split('\n') if line.strip()]
    except Exception as e:
        print(f"Error getting modified files: {e}")
        return []

def get_all_files(directories):
    files = []
    for d in directories:
        for root, _, filenames in os.walk(d):
            for f in filenames:
                if f.endswith(('.rs', '.md', '.py', '.yml', '.json', '.sh')):
                    files.append(os.path.join(root, f))
    return files

def main():
    event_name = os.environ.get("EVENT_NAME", "schedule")
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")

    if not token or not repo:
        print("GITHUB_TOKEN or GITHUB_REPOSITORY not set. Issues will not be created, but analysis will run.")

    core_dirs = ["mythrax-core/src", ".agents", "evals"]

    files_to_analyze = ["ARCHITECTURE.md", "mock_audit_report.md"]

    if event_name == "push":
        before_sha = os.environ.get("BEFORE_SHA", "HEAD~1")
        after_sha = os.environ.get("AFTER_SHA", "HEAD")
        mod_files = get_modified_files(before_sha, after_sha)
        for f in mod_files:
            if any(f.startswith(d) for d in core_dirs) and os.path.exists(f):
                files_to_analyze.append(f)
    else:
        files_to_analyze.extend(get_all_files(core_dirs))

    codebase_content = ""
    for filepath in set(files_to_analyze):
        if os.path.exists(filepath):
            try:
                with open(filepath, 'r', encoding='utf-8') as f:
                    content = f.read()
                codebase_content += f"\n--- FILE: {filepath} ---\n{content[:10000]}\n"
            except Exception as e:
                print(f"Failed to read {filepath}: {e}")

    prompt = f"""You are an adversarial CTO brought in specifically to challenge every architectural decision in mythrax. You have no loyalty to the current design.
Your job is to find where the architecture will fail under load, under adversarial inputs, or under changing requirements. You are not here to validate — you are here to stress-fracture.

Review the codebase context and identify architectural flaws based on these criteria:
- Challenge documented architectural decisions in ARCHITECTURE.md — ask "what assumption does this break if it's wrong?"
- Identify single points of failure (no graceful degradation)
- Evaluate agent orchestration design for prompt injection, unbounded recursion, and scope boundaries
- Assess if the eval framework actually tests adversarial inputs
- Identify coupling (modules that cannot be independently deployed/tested/replaced)
- Project 18 months forward: name decisions that become re-architecture projects at 10x scale

Output a JSON array of findings. Each finding must be a JSON object with these EXACT keys:
- "title": A short, descriptive title for the issue.
- "finding": A detailed description of the finding.
- "current_assumption": What the current assumption is.
- "attack_scenario": The attack scenario or failure mode.
- "blast_radius": The impact or blast radius.
- "recommended_structural_change": The recommended structural change.

Codebase context:
{codebase_content[:80000]}
"""

    from openai import OpenAI
    client = OpenAI()
    response = client.chat.completions.create(
        model="gpt-4o",
        response_format={"type": "json_object"},
        messages=[
            {"role": "system", "content": "You are a JSON-producing AI that always wraps its array of findings inside an object with a 'findings' key."},
            {"role": "user", "content": prompt}
        ]
    )

    try:
        content = response.choices[0].message.content
        data = json.loads(content)
        findings = data.get("findings", [])
        if not findings and isinstance(data, list):
            findings = data
        elif not findings:
            for k, v in data.items():
                if isinstance(v, list):
                    findings = v
                    break
    except Exception as e:
        print(f"Failed to parse OpenAI response: {e}")
        return

    existing_issues = []
    if token and repo:
        try:
            url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100"
            req = urllib.request.Request(url, headers={
                "Authorization": f"Bearer {token}",
                "Accept": "application/vnd.github.v3+json"
            })
            with urllib.request.urlopen(req) as resp:
                issues_data = json.loads(resp.read())
                existing_issues = [iss["title"] for iss in issues_data if "pull_request" not in iss]
        except Exception as e:
            print(f"Failed to fetch existing issues: {e}")

    for finding in findings:
        title = finding.get("title", "Adversarial CTO Architecture Review Finding")
        if title in existing_issues:
            print(f"Issue already exists: {title}. Skipping.")
            continue

        body = f"**Finding:** {finding.get('finding', 'N/A')}\n\n"
        body += f"**Current Assumption:** {finding.get('current_assumption', 'N/A')}\n\n"
        body += f"**Attack Scenario:** {finding.get('attack_scenario', 'N/A')}\n\n"
        body += f"**Blast Radius:** {finding.get('blast_radius', 'N/A')}\n\n"
        body += f"**Recommended Structural Change:** {finding.get('recommended_structural_change', 'N/A')}\n\n"
        body += f"---\n*Note: Never close this issue without a documented architectural decision record (ADR) response.*"

        print(f"Creating issue: {title}")
        if token and repo:
            try:
                url = f"https://api.github.com/repos/{repo}/issues"
                req = urllib.request.Request(url, method="POST", headers={
                    "Authorization": f"Bearer {token}",
                    "Accept": "application/vnd.github.v3+json",
                    "Content-Type": "application/json"
                }, data=json.dumps({
                    "title": title,
                    "body": body,
                    "labels": ["architecture-review", "adversarial"]
                }).encode('utf-8'))
                with urllib.request.urlopen(req) as resp:
                    print(f"Created issue. Status: {resp.status}")
            except Exception as e:
                print(f"Failed to create issue: {e}")

if __name__ == "__main__":
    main()
