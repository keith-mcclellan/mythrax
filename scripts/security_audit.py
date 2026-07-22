import os
import re
import json
import subprocess
import sys
import hashlib

def run_command(command, cwd=None):
    try:
        result = subprocess.run(
            command,
            shell=True,
            cwd=cwd,
            text=True,
            capture_output=True,
            check=False # Allow non-zero exit codes to capture output
        )
        return result.stdout
    except subprocess.CalledProcessError as e:
        print(f"Command failed unexpectedly: {command}\nError: {e.stderr}", file=sys.stderr)
        return ""

def issue_exists(title):
    try:
        result = subprocess.run(
            ['gh', 'issue', 'list', '--search', f'in:title "{title}"', '--state', 'open', '--json', 'title'],
            text=True,
            capture_output=True,
            check=True
        )
        issues = json.loads(result.stdout)
        return len(issues) > 0
    except (subprocess.CalledProcessError, json.JSONDecodeError, FileNotFoundError):
        return False

def create_github_issue(title, body):
    if issue_exists(title):
        print(f"Issue already exists: {title}, skipping creation.")
        return

    print(f"Filing GitHub Issue: {title}")
    command = ['gh', 'issue', 'create', '--title', title, '--body', body]
    try:
        subprocess.run(command, text=True, capture_output=True, check=True)
    except subprocess.CalledProcessError as e:
        print(f"Failed to create issue: {e.stderr}")
    except FileNotFoundError:
        print(f"gh CLI not found, skipping issue creation for local test.")

def get_hash(text):
    return hashlib.md5(text.encode('utf-8')).hexdigest()[:8]

def scan_codebase():
    findings = []

    secret_regex = re.compile(r'(?i)(token|api_key|secret|password|cred)["\']?\s*[:=]\s*["\']([a-zA-Z0-9_\-\.]{10,})["\']')

    # Mythrax-core source analysis
    for root, dirs, files in os.walk('mythrax-core'):
        if 'target' in dirs:
            dirs.remove('target')
        for file in files:
            path = os.path.join(root, file)
            try:
                with open(path, 'r', encoding='utf-8') as f:
                    lines = f.readlines()

                    for i, line in enumerate(lines):
                        # 1. Hardcoded Secrets
                        if secret_regex.search(line):
                            finding = {
                                "severity": "Critical",
                                "type": "Hardcoded Secret",
                                "file": path,
                                "line": i+1,
                                "desc": f"A hardcoded secret or token was found in the source or config file. The explicit secret value is redacted for safety.",
                                "remediation": "Remove the hardcoded secret from the repository. Use environment variables or a secure vault manager instead.",
                                "effort": "Low"
                            }
                            findings.append(finding)

                        # 2. Unsafe Rust Blocks
                        if file.endswith('.rs'):
                            if 'unsafe ' in line or 'unsafe{' in line:
                                has_justification = False
                                if i > 0 and lines[i-1].strip().startswith('//'):
                                    has_justification = True

                                desc = f"An `unsafe` block was found."
                                if not has_justification:
                                     desc += " It lacks a preceding justification comment, meaning safety invariants are unverified."
                                else:
                                     desc += " A justification comment is present."
                                desc += "\nMemory safety risk: Rust's memory guarantees (borrow checking, bounds checking) are disabled inside this block, creating the potential for memory corruption (UAF, data races, buffer overflows)."

                                finding = {
                                    "severity": "Medium",
                                    "type": "Unsafe Rust Block",
                                    "file": path,
                                    "line": i+1,
                                    "desc": f"{desc}\nCode: `{line.strip()}`",
                                    "remediation": "Review the block. If a justification is missing, add `// SAFETY: ...`. If possible, replace the `unsafe` block with safe abstractions or safe wrapper crates.",
                                    "effort": "Low"
                                }
                                findings.append(finding)

                            # 3. Untrusted Input / Command Execution
                            if 'Command::new(' in line or 'Command::new (' in line:
                                finding = {
                                    "severity": "High",
                                    "type": "Potential Untrusted Command Execution",
                                    "file": path,
                                    "line": i+1,
                                    "desc": "The application executes external commands. Ensure no untrusted external inputs are passed directly to `sh -c` or command arguments without rigorous sanitization and boundary enforcement.",
                                    "remediation": "Avoid passing input to a shell (`sh -c`). Use safe shell parsing or pass executable and arguments directly to `Command::new()`. Validate all inputs.",
                                    "effort": "Medium"
                                }
                                findings.append(finding)
            except UnicodeDecodeError:
                continue

    # 4. Cargo Audit (CVEs)
    print("Running cargo audit...")
    audit_output = run_command("cargo audit --json", cwd="mythrax-core")
    if audit_output:
        try:
            audit_data = json.loads(audit_output)

            if "vulnerabilities" in audit_data and "list" in audit_data["vulnerabilities"]:
                for vuln in audit_data["vulnerabilities"]["list"]:
                    severity = "Medium"
                    if vuln.get("advisory") and vuln.get("advisory").get("cvss"):
                        score = vuln["advisory"]["cvss"].get("base_score") if type(vuln["advisory"]["cvss"]) is dict else None
                        if score and score >= 7.0:
                            severity = "High"
                    elif vuln.get("advisory") and vuln["advisory"].get("title"):
                         title_lower = vuln["advisory"]["title"].lower()
                         if "execution" in title_lower or "overflow" in title_lower or "memory" in title_lower:
                             severity = "High"

                    finding = {
                        "severity": severity,
                        "type": f"Vulnerable Dependency: {vuln['package']['name']} {vuln['package']['version']}",
                        "file": "Cargo.lock",
                        "line": 0,
                        "desc": f"CVE ID: {vuln['advisory']['id']}. {vuln['advisory']['title']}",
                        "remediation": f"Upgrade crate to a patched version if available.",
                        "effort": "Medium"
                    }
                    findings.append(finding)

            if "warnings" in audit_data:
                 for warning_list in audit_data["warnings"].values():
                      for w in warning_list:
                          finding = {
                            "severity": "Low",
                            "type": f"Dependency Warning ({w['kind']}): {w['package']['name']}",
                            "file": "Cargo.lock",
                            "line": 0,
                            "desc": f"{(w.get('advisory') or {}).get('title', 'Yanked or Unmaintained crate')}",
                            "remediation": "Consider replacing or updating the crate.",
                            "effort": "Medium"
                          }
                          findings.append(finding)
        except json.JSONDecodeError:
            print("Failed to parse cargo audit output.")

    # 5. Git History Secrets
    print("Scanning git history for secrets...")
    # Fetch log and rely on python to filter
    git_output = run_command("git log -p")

    has_leaked_secret = False
    for line in git_output.split('\n'):
         if line.startswith('+') and secret_regex.search(line):
             has_leaked_secret = True
             break

    if has_leaked_secret:
        finding = {
            "severity": "High",
            "type": "Secret in Git History",
            "file": "Git History",
            "line": 0,
            "desc": "Secrets or authentication tokens were found in the commit history.",
            "remediation": "Consider compromised secrets as leaked and rotate them. Use git pre-commit hooks to prevent future leaks.",
            "effort": "High"
        }
        findings.append(finding)

    return findings

def generate_report_and_issues(findings):
    # Sort findings by severity
    severity_order = {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}
    findings.sort(key=lambda x: (severity_order.get(x["severity"], 4), x["type"], x["file"]))

    report_lines = ["# Security Advisory Report\n"]

    for finding in findings:
        report_lines.append(f"## {finding['severity']}: {finding['type']}")
        report_lines.append(f"**File:** {finding['file']} (Line {finding['line']})")
        report_lines.append(f"**Description:** {finding['desc']}")
        report_lines.append(f"**Remediation:** {finding['remediation']}")
        report_lines.append(f"**Estimated Effort:** {finding['effort']}\n")

        if finding['severity'] in ['Critical', 'High']:
            body = f"**File:** {finding['file']} (Line {finding['line']})\n\n**Description:**\n{finding['desc']}\n\n**Remediation:**\n{finding['remediation']}"
            unique_id = get_hash(f"{finding['type']}-{finding['file']}-{finding['line']}")
            title = f"Security: {finding['type']} [{finding['file'].split('/')[-1]}-{finding['line']}] ({unique_id})"
            create_github_issue(title, body)

    with open("security_audit_report.md", "w") as f:
        f.write("\n".join(report_lines))
    print("Report written to security_audit_report.md")

if __name__ == "__main__":
    findings = scan_codebase()
    generate_report_and_issues(findings)
