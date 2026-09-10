#!/usr/bin/env python3
import os
import subprocess
import urllib.request
import json

def get_modified_files():
    base_ref = os.environ.get("GITHUB_BASE_REF")
    if not base_ref:
        try:
            res = subprocess.run(["git", "diff", "--name-only", "HEAD~1"], capture_output=True, text=True, check=True)
            return res.stdout.splitlines()
        except subprocess.CalledProcessError:
            return []
    try:
        subprocess.run(["git", "fetch", "origin", base_ref], check=False)
        res = subprocess.run(["git", "diff", "--name-only", f"origin/{base_ref}"], capture_output=True, text=True, check=True)
        return res.stdout.splitlines()
    except subprocess.CalledProcessError:
        return []

def get_all_files():
    files = []
    with subprocess.Popen(["git", "ls-tree", "-r", "--name-only", "HEAD"], stdout=subprocess.PIPE, text=True) as proc:
        for line in proc.stdout:
            files.append(line.strip())
    return files

def get_files_to_analyze():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "schedule")
    if event_name in ("pull_request", "push"):
        files = get_modified_files()
    else:
        files = get_all_files()

    targets = []
    for f in files:
        if not os.path.exists(f):
            continue
        if f.startswith("mythrax-core/") or f.startswith(".agents/") or f.startswith("evals/"):
            if f.endswith(".rs") or f.endswith(".py") or f.endswith(".md"):
                targets.append(f)

    for core_file in ["ARCHITECTURE.md", "mock_audit_report.md"]:
        if core_file not in targets and os.path.exists(core_file):
            targets.append(core_file)

    return targets

def call_llm(prompt):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found. Bypassing LLM logic gracefully.")
        return []

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    data = {
        "model": "gpt-4",
        "messages": [
            {
                "role": "system",
                "content": (
                    "You are a CTO performing an adversarial code review. "
                    "Format findings as a JSON list of objects with 'title' and 'body'. "
                    "The body MUST use the 'Red Team Architecture Brief' format: "
                    "Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change. "
                    "Also provide the file and line number, a minimal reproducible scenario, the severity, and a suggested fix."
                )
            },
            {"role": "user", "content": prompt}
        ]
    }

    req = urllib.request.Request(url, headers=headers, data=json.dumps(data).encode('utf-8'))
    try:
        with urllib.request.urlopen(req) as response:
            res_body = response.read()
            res_json = json.loads(res_body)
            content = res_json['choices'][0]['message']['content'].strip()
            if content.startswith("```json"):
                content = content[7:-3]
            elif content.startswith("```"):
                content = content[3:-3]
            return json.loads(content.strip())
    except Exception as e:
        print(f"LLM call failed: {e}")
        return []

def get_existing_issues(repo, token):
    if not token:
        return []
    url = f"https://api.github.com/repos/{repo}/issues?state=all&labels=adversarial"
    req = urllib.request.Request(url, headers={"Authorization": f"token {token}", "User-Agent": "Adversarial-CTO"})
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read())
            return [issue['title'] for issue in issues]
    except Exception as e:
        print(f"Failed to get existing issues: {e}")
        return []

def file_issue(repo, token, title, body):
    if not token:
        print(f"No GITHUB_TOKEN. Would have filed issue: {title}")
        return
    url = f"https://api.github.com/repos/{repo}/issues"

    adr_note = "\n\n**Note:** Do not close this issue without a documented architectural decision record (ADR) response."

    data = {
        "title": title,
        "body": body + adr_note,
        "labels": ["bug", "agent-found", "architecture-review", "adversarial"]
    }

    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json",
        "User-Agent": "Adversarial-CTO"
    }

    req = urllib.request.Request(url, headers=headers, data=json.dumps(data).encode('utf-8'), method="POST")
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {title}")
    except Exception as e:
        print(f"Failed to create issue {title}: {e}")

def analyze_chunk(files_chunk):
    content_payload = ""
    for f in files_chunk:
        try:
            with open(f, 'r', encoding='utf-8') as file_obj:
                content_payload += f"\n--- File: {f} ---\n"
                content_payload += file_obj.read()
        except Exception:
            pass

    if not content_payload.strip():
        return

    prompt = f"""
Analyze the following files for logic bugs, edge case vulnerabilities, and correctness issues:
1. Off-by-one errors in loops, indexes, or range operations.
2. Panic paths: .unwrap() or .expect() that could be triggered by external data.
3. Integer overflow or underflow risks in scoring, counting, or metrics.
4. Race conditions or incorrect assumptions about execution order in async/concurrent code.
5. Logic branches where code silently succeeds but produces incorrect results.
6. Test coverage gaps (public functions with no corresponding test).

Files:
{content_payload}

Return ONLY a valid JSON list of objects containing 'title' and 'body'.
If no issues found, return [].
"""
    findings = call_llm(prompt)
    if not findings:
        return

    repo = os.environ.get("GITHUB_REPOSITORY", "user/repo")
    token = os.environ.get("GITHUB_TOKEN")
    existing_issues = get_existing_issues(repo, token)

    for finding in findings:
        title = finding.get('title')
        body = finding.get('body')
        if not title or not body:
            continue

        if title in existing_issues:
            print(f"Issue already exists: {title}")
            continue

        file_issue(repo, token, title, body)
        existing_issues.append(title)

def main():
    targets = get_files_to_analyze()
    print(f"Found {len(targets)} files to analyze.")

    chunk_size = 5
    for i in range(0, len(targets), chunk_size):
        chunk = targets[i:i+chunk_size]
        print(f"Analyzing chunk {i//chunk_size + 1}...")
        analyze_chunk(chunk)

if __name__ == "__main__":
    main()
