import os
import subprocess
import json
import re

def run_command(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout, result.stderr, result.returncode

def file_issue(title, body):
    # Files a mock issue as requested by the memory guidelines (gh not available)
    issue_id = len([f for f in os.listdir('.') if f.startswith('issue_') and f.endswith('.md')]) + 1
    filename = f"issue_{issue_id}.md"
    with open(filename, 'w') as f:
        f.write(f"# {title}\n\n{body}\n\n**Labels:** bug, agent-found, security")
    print(f"Filed issue: {filename}")

def check_secrets():
    print("Checking for secrets...")
    # Very basic regex for demonstration, should be expanded
    secret_regex = r"(api_key|secret|password|token)[ ]*[:=][ ]*[\"'][A-Za-z0-9\-_]{16,}[\"']"
    issues = []
    for root, _, files in os.walk('mythrax-core/src'):
        for file in files:
            if not file.endswith('.rs'): continue
            path = os.path.join(root, file)
            with open(path, 'r', errors='ignore') as f:
                content = f.read()
                matches = re.finditer(secret_regex, content, re.IGNORECASE)
                for match in matches:
                     issues.append({
                         "file": path,
                         "match": match.group(0),
                         "severity": "High"
                     })
    return issues

def check_unsafe():
    print("Checking for unsafe blocks...")
    issues = []
    unsafe_regex = r"unsafe\s*\{"
    safety_comment_regex = r"//\s*SAFETY:"

    for root, _, files in os.walk('mythrax-core/src'):
        for file in files:
            if not file.endswith('.rs'): continue
            path = os.path.join(root, file)
            with open(path, 'r') as f:
                lines = f.readlines()
                for i, line in enumerate(lines):
                    if re.search(unsafe_regex, line):
                        # Check preceding lines for safety comment
                        has_comment = False
                        for j in range(max(0, i-3), i):
                            if re.search(safety_comment_regex, lines[j]):
                                has_comment = True
                                break
                        if not has_comment:
                            issues.append({
                                "file": path,
                                "line": i + 1,
                                "severity": "Medium",
                                "desc": "Unsafe block without // SAFETY: documentation"
                            })
    return issues

def check_cves():
    print("Checking for CVEs...")
    stdout, stderr, rc = run_command("cd mythrax-core && cargo audit --json")
    if rc == 0:
        return []
    try:
        data = json.loads(stdout)
        issues = []
        for vuln in data.get("vulnerabilities", {}).get("list", []):
             issues.append({
                 "package": vuln["package"]["name"],
                 "version": vuln["package"]["version"],
                 "advisory": vuln["advisory"]["id"],
                 "severity": "High" # Default to high for CVEs
             })
        return issues
    except:
        print("Failed to parse cargo audit output")
        return []

def check_git_history():
    print("Checking git history for secrets...")
    # Check all commits for secrets
    stdout, _, _ = run_command("git log -p | grep -iE '(api_key|secret|password|token)[ ]*[:=][ ]*[\"\\'][A-Za-z0-9\\-_]+[\"\\']' || true")
    issues = []
    if stdout.strip():
        # Just flag that we found something, parsing git log output perfectly is hard
        issues.append({
            "desc": "Found potential secrets in git history. Please review `git log -p`.",
            "severity": "High"
        })
    return issues

def main():
    report = "# Mythrax Security Advisory Report\n\n"

    secret_issues = check_secrets()
    unsafe_issues = check_unsafe()
    cve_issues = check_cves()
    git_issues = check_git_history()

    all_issues = secret_issues + unsafe_issues + cve_issues + git_issues

    for issue in all_issues:
        severity = issue.get("severity", "Medium")
        title = f"[{severity}] Security Finding"
        body = json.dumps(issue, indent=2)
        report += f"## {title}\n```json\n{body}\n```\n\n"

        if severity in ["Critical", "High"]:
            file_issue(title, body)

    with open("security_advisory_report.md", "w") as f:
        f.write(report)
    print("Audit complete.")

if __name__ == "__main__":
    main()
