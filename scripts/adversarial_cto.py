import os
import sys
import json
import urllib.request
import urllib.error
import subprocess
import re

def get_modified_files():
    try:
        base_ref = os.environ.get("GITHUB_BASE_REF")
        if base_ref:
            subprocess.run(["git", "fetch", "origin", base_ref], check=False)
            cmd = ["git", "diff", "--name-only", f"origin/{base_ref}...HEAD"]
        else:
            cmd = ["git", "diff", "--name-only", "HEAD~1", "HEAD"]
        output = subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL)
        files = output.strip().split('\n')
        return [f for f in files if f and os.path.exists(f)]
    except Exception as e:
        print(f"Error getting modified files: {e}")
        return []

def get_all_files():
    targets = ["mythrax-core", ".agents", "evals"]
    files = []
    for target in targets:
        if os.path.exists(target):
            for root, _, filenames in os.walk(target):
                for filename in filenames:
                    if filename.endswith(".rs") or filename.endswith(".py") or filename.endswith(".md"):
                        files.append(os.path.join(root, filename))
    return files

def get_files_to_analyze(event_name):
    files = []
    if event_name in ("pull_request", "push"):
        files = get_modified_files()
        for core_file in ["ARCHITECTURE.md", "mock_audit_report.md"]:
            if os.path.exists(core_file) and core_file not in files:
                files.append(core_file)
    else:
        files = get_all_files()
        for core_file in ["ARCHITECTURE.md", "mock_audit_report.md"]:
            if os.path.exists(core_file) and core_file not in files:
                files.append(core_file)
    return files

def check_test_coverage(files_to_analyze):
    findings = []
    src_files = [f for f in files_to_analyze if f.startswith("mythrax-core/src/") and f.endswith(".rs")]

    paths_to_check = []
    for p in ["mythrax-core/tests", "evals"]:
        if os.path.exists(p):
            paths_to_check.append(p)

    for f in src_files:
        with open(f, "r") as file_obj:
            lines = file_obj.readlines()

        for i, line in enumerate(lines):
            match = re.search(r"pub\s+(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*\(", line)
            if match:
                fn_name = match.group(1)

                found_in_tests = False
                if paths_to_check:
                    cmd = ["git", "grep", "-q", "-E", fn_name, "--"] + paths_to_check
                    try:
                        subprocess.run(cmd, check=True, stderr=subprocess.DEVNULL)
                        found_in_tests = True
                    except subprocess.CalledProcessError:
                        pass

                if not found_in_tests:
                    file_content = "".join(lines)
                    if "#[test]" in file_content and fn_name in file_content[file_content.find(line)+len(line):]:
                         found_in_tests = True

                if not found_in_tests:
                    findings.append({
                        "title": f"Test Coverage Gap: {fn_name}",
                        "file": f,
                        "line": i + 1,
                        "finding": f"Public function `{fn_name}` has no corresponding test in the eval or test suite.",
                        "current_assumption": "Public functions are thoroughly tested.",
                        "attack_scenario": "Unintended logic changes or edge cases in this function will not be caught by CI.",
                        "blast_radius": "High risk of silent regressions.",
                        "recommended_structural_change": "Implement a mandatory coverage gate for all `pub fn` declarations.",
                        "reproducible_scenario": "N/A - static analysis finding.",
                        "severity": "Medium",
                        "suggested_fix": f"Add integration tests in `mythrax-core/tests` or `evals` covering `{fn_name}`."
                    })
    return findings

def analyze_code(files):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not set. Skipping LLM analysis.")
        return []

    findings = []
    for f in files:
        if not os.path.isfile(f) or f.endswith(".md"):
            continue
        with open(f, "r") as file_obj:
            content = file_obj.read()

        prompt = f"""You are the 'Adversarial CTO'. Analyze this code for logic bugs, edge case vulnerabilities, and correctness issues:
- Off-by-one errors in loop, index, or range operation
- Panic paths: .unwrap() or .expect() triggered by realistic inputs
- Integer overflow/underflow risks in arithmetic (agent scoring, token counting, eval metrics)
- Race conditions or incorrect assumptions about execution order
- Logic branches where code silently succeeds but produces incorrect result

File: {f}
Code:
```
{content}
```

Format output as a JSON array of objects. Each object must have:
- title: Short title of the bug
- file: {f}
- line: line number (integer)
- finding: Description of the bug
- current_assumption: Current Assumption
- attack_scenario: Attack Scenario
- blast_radius: Blast Radius
- recommended_structural_change: Recommended Structural Change
- reproducible_scenario: Minimal reproducible scenario
- severity: Severity (e.g. Critical, High, Medium, Low)
- suggested_fix: Suggested fix

Only return the JSON array, no markdown wrappers, no other text."""

        req = urllib.request.Request(
            "https://api.openai.com/v1/chat/completions",
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {api_key}"
            },
            data=json.dumps({
                "model": "gpt-4o",
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0.0
            }).encode("utf-8")
        )
        try:
            with urllib.request.urlopen(req) as response:
                result = json.loads(response.read().decode("utf-8"))
                content_resp = result["choices"][0]["message"]["content"].strip()
                if content_resp.startswith("```json"):
                    content_resp = content_resp[7:-3]
                elif content_resp.startswith("```"):
                    content_resp = content_resp[3:-3]

                try:
                    file_findings = json.loads(content_resp)
                    if isinstance(file_findings, list):
                        findings.extend(file_findings)
                except json.JSONDecodeError as e:
                    print(f"Failed to decode JSON from LLM for {f}: {e}")
        except Exception as e:
            print(f"Failed to analyze {f}: {e}")

    return findings

def get_existing_issues(repo, token):
    issues = []
    url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100"
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json"
    })
    try:
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode("utf-8"))
            for issue in data:
                issues.append(issue["title"])
    except Exception as e:
        print(f"Failed to fetch existing issues: {e}")
    return set(issues)

def create_github_issue(repo, token, finding):
    url = f"https://api.github.com/repos/{repo}/issues"

    title = f"Adversarial CTO: {finding.get('title', 'Bug Found')}"

    body = f"""## Red Team Architecture Brief

**Finding:** {finding.get('finding')}
**Current Assumption:** {finding.get('current_assumption')}
**Attack Scenario:** {finding.get('attack_scenario')}
**Blast Radius:** {finding.get('blast_radius')}
**Recommended Structural Change:** {finding.get('recommended_structural_change')}

### Details
- **File:** {finding.get('file')}
- **Line:** {finding.get('line')}
- **Severity:** {finding.get('severity')}

**Minimal Reproducible Scenario:**
{finding.get('reproducible_scenario')}

**Suggested Fix:**
{finding.get('suggested_fix')}

> **Note:** Developers must never close this issue without a documented architectural decision record (ADR) response.
"""

    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    req = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github.v3+json",
            "Content-Type": "application/json"
        },
        data=json.dumps(data).encode("utf-8")
    )

    try:
        with urllib.request.urlopen(req) as response:
            print(f"Successfully created issue: {title}")
    except urllib.error.HTTPError as e:
        print(f"Failed to create issue: {e.read().decode('utf-8')}")
    except Exception as e:
        print(f"Failed to create issue: {e}")

def main():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "schedule")
    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")

    if not repo or not token:
        print("GITHUB_REPOSITORY or GITHUB_TOKEN is not set. Running in local fallback mode.")
        is_local = True
    else:
        is_local = False

    files = get_files_to_analyze(event_name)
    print(f"Analyzing {len(files)} files...")

    findings = check_test_coverage(files)
    findings.extend(analyze_code(files))

    if not findings:
        print("No findings to report.")
        return

    if is_local:
        if not os.path.exists("issues"):
            os.makedirs("issues")
        for i, finding in enumerate(findings):
            title = finding.get('title', f'Bug_{i}')
            safe_title = "".join(c if c.isalnum() else "_" for c in title)
            with open(f"issues/{safe_title}.md", "w") as f:
                f.write(f"Title: {title}\n")
                f.write(json.dumps(finding, indent=2))
        print(f"Saved {len(findings)} mock issues to issues/ directory.")
        return

    existing_issues = get_existing_issues(repo, token)

    for finding in findings:
        title = f"Adversarial CTO: {finding.get('title', 'Bug Found')}"
        if title in existing_issues:
            print(f"Issue already exists: {title}")
            continue
        create_github_issue(repo, token, finding)

if __name__ == "__main__":
    main()
