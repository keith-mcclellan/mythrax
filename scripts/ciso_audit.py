import os
import re
import json
import subprocess
import shutil

EXCLUDE_DIRS = {'target', '.git', '.venv', 'node_modules', 'issues', 'tests', 'bench_data'}
SECRET_REGEX = re.compile(r"(api_key|secret|token|password|auth)[_a-zA-Z0-9]*\s*[:=]\s*[\"'][^\"']+[\"']", re.IGNORECASE)
UNSAFE_REGEX = re.compile(r"unsafe\s*\{")
SAFETY_COMMENT_REGEX = re.compile(r"//\s*SAFETY:", re.IGNORECASE)
PROCESS_CMD_REGEX = re.compile(r"Command::new\s*\(\s*[^)]+\s*\)\s*(?:\.[a-zA-Z_0-9]+\([^)]*\)\s*)*\.(?:arg|args)\s*\([^\"']+\)")

def run_cmd(cmd, cwd=None, shell=False):
    try:
        result = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, shell=shell)
        return result.stdout, result.stderr, result.returncode
    except Exception as e:
        return "", str(e), 1

def scan_files():
    findings = []

    # Files to scan
    files_to_scan = []
    for root, dirs, files in os.walk('.'):
        dirs[:] = [d for d in dirs if d not in EXCLUDE_DIRS]
        for file in files:
            if file.endswith(('.rs', '.toml', '.md', '.py', '.yml', '.yaml', '.json')) or file in ('Cargo.toml', 'Cargo.lock'):
                files_to_scan.append(os.path.join(root, file))

    for file_path in files_to_scan:
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                lines = f.readlines()
        except:
            continue

        for i, line in enumerate(lines):
            line_num = i + 1

            # Hardcoded Secrets
            if SECRET_REGEX.search(line):
                findings.append({
                    "severity": "Critical",
                    "type": "Hardcoded Secret",
                    "file": file_path,
                    "line": line_num,
                    "desc": "Potential hardcoded secret or credential found.",
                    "remediation": "Move the secret to an environment variable or secure vault.",
                    "estimated_effort": "Medium"
                })

            # Unsafe Rust
            if file_path.endswith('.rs'):
                if UNSAFE_REGEX.search(line):
                    has_safety_comment = False
                    # Check preceding 5 lines for SAFETY comment
                    for j in range(max(0, i - 5), i):
                        if SAFETY_COMMENT_REGEX.search(lines[j]):
                            has_safety_comment = True
                            break

                    if not has_safety_comment:
                        findings.append({
                            "severity": "High",
                            "type": "Unjustified Unsafe Block",
                            "file": file_path,
                            "line": line_num,
                            "desc": "Unsafe Rust block lacks a documented `// SAFETY:` justification. This poses a memory safety risk as invariants are not explicitly stated.",
                            "remediation": "Add a `// SAFETY:` comment explaining why the unsafe block is sound, or rewrite the code using safe abstractions.",
                            "estimated_effort": "Medium"
                        })
                    else:
                        findings.append({
                            "severity": "Medium",
                            "type": "Unsafe Block Usage",
                            "file": file_path,
                            "line": line_num,
                            "desc": "Unsafe Rust block found with justification. This still requires careful manual review.",
                            "remediation": "Review the unsafe block and its safety justification for correctness.",
                            "estimated_effort": "Low"
                        })

    # Read the whole content for multi-line regexes
    for file_path in files_to_scan:
        if file_path.endswith('.rs'):
            try:
                with open(file_path, 'r', encoding='utf-8') as f:
                    content = f.read()
            except:
                continue

            # Untrusted External Input
            if "Command::new" in content:
                for match in re.finditer(r"Command::new\s*\([^)]*\)(?:\s*\.\s*[a-zA-Z_]+\s*\([^)]*\))*", content):
                    matched_str = match.group(0)
                    if ".arg(" in matched_str or ".args(" in matched_str:
                         # Very basic heuristic: check if arg is passed a string literal
                         if not re.search(r"\.(?:arg|args)\s*\(\s*\"[^\"]*\"\s*\)", matched_str):
                             findings.append({
                                "severity": "High",
                                "type": "External Input Sanitization",
                                "file": file_path,
                                "line": 0, # Hard to get exact line with this simple match
                                "desc": f"Potential unsanitized external input passed to `Command`. Matched pattern: {matched_str[:50]}...",
                                "remediation": "Ensure arguments passed to `std::process::Command` are strictly sanitized or avoid using shell execution if possible.",
                                "estimated_effort": "High"
                            })

    return findings

def scan_cargo_audit():
    findings = []
    if os.path.isdir('mythrax-core'):
        # Check if cargo-audit is installed
        stdout, stderr, rc = run_cmd(["cargo", "audit", "--version"])
        if rc != 0:
             print("cargo-audit not installed. Skipping cargo audit check.")
             return findings

        stdout, stderr, rc = run_cmd(["cargo", "audit", "--json"], cwd="mythrax-core")

        try:
            audit_data = json.loads(stdout)

            for vuln in audit_data.get('vulnerabilities', {}).get('list', []):
                findings.append({
                    "severity": "High",
                    "type": "Vulnerable Dependency (CVE)",
                    "file": "Cargo.lock",
                    "line": 0,
                    "desc": f"Crate {vuln.get('package', {}).get('name')} {vuln.get('package', {}).get('version')} has known vulnerability: {vuln.get('advisory', {}).get('title')} ({vuln.get('advisory', {}).get('id')})",
                    "remediation": f"Update {vuln.get('package', {}).get('name')} to a patched version or remove the dependency.",
                    "estimated_effort": "Low"
                })

            for warning in audit_data.get('warnings', {}).values():
                 for warn in warning:
                    if warn.get('kind') in ('yanked', 'unmaintained'):
                        advisory = warn.get('advisory')
                        title = advisory.get('title', 'No details') if advisory else 'No details'
                        findings.append({
                            "severity": "Medium",
                            "type": "Dependency Warning",
                            "file": "Cargo.lock",
                            "line": 0,
                            "desc": f"Crate {warn.get('package', {}).get('name')} {warn.get('package', {}).get('version')}: {warn.get('kind')} - {title}",
                            "remediation": "Review the dependency and consider migrating to an alternative crate.",
                            "estimated_effort": "Medium"
                        })
        except json.JSONDecodeError:
            print("Failed to parse cargo audit JSON output.")

    return findings

def scan_git_history():
    findings = []
    # Use POSIX Extended Regular Expressions for git log -G
    regex_pattern = r"(api_key|secret|token|password|auth)[_a-zA-Z0-9]*[[:space:]]*[:=][[:space:]]*[\"'][^\"']+[\"']"

    cmd = ["git", "log", "-E", "-G", regex_pattern, "--oneline"]
    stdout, stderr, rc = run_cmd(cmd)

    if stdout:
        commits = stdout.strip().split('\n')
        for commit in commits:
            if commit:
                findings.append({
                    "severity": "High",
                    "type": "Secret in Git History",
                    "file": "Git History",
                    "line": 0,
                    "desc": f"Potential secret found in git commit history: {commit}",
                    "remediation": "Use tools like BFG Repo-Cleaner or git filter-repo to remove the secret from history, and rotate the exposed secret immediately.",
                    "estimated_effort": "High"
                })
    return findings

def generate_report(findings):
    report_path = "security_advisory_report.md"
    with open(report_path, "w") as f:
        f.write("# Security Advisory Report\n\n")

        severity_order = {"Critical": 1, "High": 2, "Medium": 3, "Low": 4}
        findings.sort(key=lambda x: severity_order.get(x["severity"], 99))

        for finding in findings:
            f.write(f"## [{finding['severity']}] {finding['type']}\n")
            f.write(f"- **File:** {finding['file']}:{finding['line']}\n")
            f.write(f"- **Description:** {finding['desc']}\n")
            f.write(f"- **Remediation:** {finding['remediation']}\n")
            f.write(f"- **Estimated Effort:** {finding['estimated_effort']}\n\n")

    print(f"Report generated at {report_path}")

def file_github_issues(findings):
    if not shutil.which('gh'):
        print("GitHub CLI (gh) not found. Skipping issue creation.")
        return

    for finding in findings:
        if finding["severity"] in ["Critical", "High"]:
            title = f"Security Vulnerability: [{finding['severity']}] {finding['type']} in {finding['file']}"
            body = f"""**Severity:** {finding['severity']}
**Type:** {finding['type']}
**File:** {finding['file']}:{finding['line']}

**Description:**
{finding['desc']}

**Remediation:**
{finding['remediation']}

**Estimated Effort:** {finding['estimated_effort']}
"""

            # Check for existing issues to avoid duplication
            search_cmd = ["gh", "issue", "list", "--search", f'"{title}" in:title', "--state", "open", "--json", "title"]
            try:
                stdout, stderr, rc = run_cmd(search_cmd)
                if rc == 0:
                    existing_issues = json.loads(stdout)
                    if any(issue.get("title") == title for issue in existing_issues):
                        print(f"Issue already exists, skipping: {title}")
                        continue
            except Exception as e:
                print(f"Failed to search for existing issues: {e}")

            try:
                cmd = ["gh", "issue", "create", "--title", title, "--body", body, "--label", "security", "--label", finding["severity"].lower()]
                subprocess.run(cmd, check=True)
                print(f"Created GitHub Issue: {title}")
            except subprocess.CalledProcessError as e:
                print(f"Failed to create GitHub Issue: {e}")

if __name__ == "__main__":
    findings = []
    findings.extend(scan_files())
    findings.extend(scan_cargo_audit())
    findings.extend(scan_git_history())

    generate_report(findings)
    file_github_issues(findings)
