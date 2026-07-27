import os
import json
import re
import subprocess
import argparse
from typing import List, Dict, Any, Tuple

REPORT_FILE = "security_audit_report.md"
ISSUES_DIR = "issues"

def get_files_to_scan() -> List[str]:
    files = []
    for root, dirs, filenames in os.walk('.'):
        if '.git' in dirs:
            dirs.remove('.git')
        if 'target' in dirs:
            dirs.remove('target')

        for filename in filenames:
            if filename.endswith('.rs') or filename.endswith('.toml') or filename.endswith('.yml') or filename.endswith('.yaml'):
                files.append(os.path.join(root, filename))
    return files

def check_hardcoded_secrets_and_unsafe(files: List[str]) -> List[Dict[str, Any]]:
    findings = []

    # Very basic secret regexes for demo purposes. Real regexes should be more robust.
    secret_patterns = {
        "API Key": re.compile(r'(?i)(?:api_key|apikey|secret|token|password)[\s:=]+["\']([a-zA-Z0-9_\-]{16,})["\']'),
        "Bearer Token": re.compile(r'(?i)bearer\s+[a-zA-Z0-9_\-\.]+'),
        "Hardcoded Auth Token": re.compile(r'X-Mythrax-Token.*?["\']([^"\']+)["\']', re.IGNORECASE)
    }

    for file_path in files:
        if not file_path.endswith('.rs') and not file_path.endswith('.yml') and not file_path.endswith('.yaml'):
            continue

        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                lines = f.readlines()

            for i, line in enumerate(lines):
                line_num = i + 1

                # Check for secrets
                for secret_type, pattern in secret_patterns.items():
                    if pattern.search(line):
                        # Filter out known safe test tokens or examples if necessary, but we flag for review
                        if "fallback-err-token" in line or "dummy" in line.lower() or "test" in line.lower():
                            severity = "Medium"
                        else:
                            severity = "High"

                        findings.append({
                            "severity": severity,
                            "title": f"Potential {secret_type} found",
                            "file": file_path,
                            "line": line_num,
                            "description": f"Hardcoded credential/secret detected matching {secret_type}.",
                            "remediation": "Move secrets to environment variables, a secure vault, or use the configuration file.",
                            "estimated_effort": "Low"
                        })

                # Check for unsafe blocks in Rust files
                if file_path.endswith('.rs') and 'unsafe {' in line:
                    # Check previous lines for SAFETY comment
                    safety_comment_found = False
                    # Look up to 5 lines above
                    start_check = max(0, i - 5)
                    for j in range(start_check, i):
                        if '// SAFETY:' in lines[j] or '// Safety:' in lines[j] or '// safety:' in lines[j]:
                            safety_comment_found = True
                            break

                    if not safety_comment_found:
                        findings.append({
                            "severity": "High",
                            "title": "Unjustified unsafe block",
                            "file": file_path,
                            "line": line_num,
                            "description": "An unsafe block was found without a preceding '// SAFETY:' comment explaining the memory safety justification (e.g. use-after-free, data races).",
                            "remediation": "Add a '// SAFETY:' comment explaining why this unsafe block is sound, or refactor to safe Rust.",
                            "estimated_effort": "Medium"
                        })
        except Exception as e:
            print(f"Error reading {file_path}: {e}")

    return findings

def check_unsanitized_inputs(files: List[str]) -> List[Dict[str, Any]]:
    findings = []

    command_pattern = re.compile(r'std::process::Command::new\s*\(')

    for file_path in files:
        if not file_path.endswith('.rs'):
            continue

        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                lines = f.readlines()

            for i, line in enumerate(lines):
                if command_pattern.search(line):
                    # Flag all external process invocations for review
                    findings.append({
                        "severity": "Medium", # Could be high depending on input, flag for review
                        "title": "External Process Invocation",
                        "file": file_path,
                        "line": i + 1,
                        "description": "Call to std::process::Command::new detected. Ensure any dynamic inputs passed to this command are strictly sanitized and boundaries are enforced.",
                        "remediation": "Review the command arguments. Avoid passing unsanitized user input to the shell or external programs.",
                        "estimated_effort": "Low"
                    })
        except Exception as e:
            print(f"Error reading {file_path}: {e}")

    return findings

def check_cargo_audit() -> List[Dict[str, Any]]:
    findings = []

    # Check for Cargo.toml in current directory or mythrax-core
    cargo_cwd = "."
    cargo_toml_path = "Cargo.toml"
    if not os.path.exists(cargo_toml_path):
        if os.path.exists("mythrax-core/Cargo.toml"):
            cargo_cwd = "mythrax-core"
            cargo_toml_path = "mythrax-core/Cargo.toml"
        else:
            print("Warning: Cargo.toml not found, skipping cargo audit.")
            return findings

    try:
        print(f"Running cargo audit in {cargo_cwd}...")
        result = subprocess.run(
            ["cargo", "audit", "--json"],
            cwd=cargo_cwd,
            capture_output=True,
            text=True
        )

        # cargo audit returns non-zero if vulnerabilities are found
        if not result.stdout:
            print("Warning: No output from cargo audit. Ensure cargo-audit is installed.")
            return findings

        try:
            audit_data = json.loads(result.stdout)
        except json.JSONDecodeError:
            print("Error: Could not parse cargo audit JSON output.")
            print(result.stdout)
            return findings

        # Check for vulnerabilities
        if "vulnerabilities" in audit_data and audit_data["vulnerabilities"]:
            for vuln in audit_data["vulnerabilities"]["list"]:
                adv = vuln.get("advisory", {})
                pkg = vuln.get("package", {})

                # CVSS score or fallback mapping based on RustSec info
                findings.append({
                    "severity": "High", # Conservatively set known CVEs to High
                    "title": f"Vulnerable Dependency: {pkg.get('name')} {pkg.get('version')}",
                    "file": cargo_toml_path,
                    "line": 1,
                    "description": f"CVE: {adv.get('id')}. {adv.get('title')}. {adv.get('description', '')[:200]}...",
                    "remediation": f"Update `{pkg.get('name')}` to a patched version.",
                    "estimated_effort": "Low"
                })

        # Check for yanked crates
        if "warnings" in audit_data and audit_data["warnings"]:
            for warning in audit_data["warnings"].get("yanked", []):
                pkg = warning.get("package", {})
                findings.append({
                    "severity": "Medium",
                    "title": f"Yanked Dependency: {pkg.get('name')} {pkg.get('version')}",
                    "file": cargo_toml_path,
                    "line": 1,
                    "description": f"The crate `{pkg.get('name')}` version `{pkg.get('version')}` has been yanked from crates.io.",
                    "remediation": f"Update or replace `{pkg.get('name')}`.",
                    "estimated_effort": "Low"
                })

            for warning in audit_data["warnings"].get("unmaintained", []):
                pkg = warning.get("package", {})
                findings.append({
                    "severity": "Medium",
                    "title": f"Unmaintained Dependency: {pkg.get('name')} {pkg.get('version')}",
                    "file": cargo_toml_path,
                    "line": 1,
                    "description": f"The crate `{pkg.get('name')}` is flagged as unmaintained.",
                    "remediation": f"Migrate away from `{pkg.get('name')}` to an actively maintained alternative.",
                    "estimated_effort": "High"
                })

    except Exception as e:
        print(f"Error running cargo audit: {e}")

    return findings

def check_git_history() -> List[Dict[str, Any]]:
    findings = []

    # Very basic secret regexes for demo purposes
    secret_patterns = {
        "API Key": re.compile(r'(?i)(?:api_key|apikey|secret|token|password)[\s:=]+["\']([a-zA-Z0-9_\-]{16,})["\']'),
        "Bearer Token": re.compile(r'(?i)bearer\s+[a-zA-Z0-9_\-\.]+'),
    }

    try:
        print("Scanning git history...")
        result = subprocess.run(
            ["git", "log", "-p"],
            capture_output=True,
            text=True
        )

        if result.returncode != 0:
            print("Warning: Could not run git log.")
            return findings

        current_commit = ""
        current_file = ""

        for line in result.stdout.splitlines():
            if line.startswith("commit "):
                current_commit = line.split()[1]
            elif line.startswith("+++ b/"):
                current_file = line[6:]
            elif line.startswith("+"):
                # Check added lines for secrets
                for secret_type, pattern in secret_patterns.items():
                    if pattern.search(line):
                        # Filter out test tokens
                        if "fallback-err-token" not in line and "dummy" not in line.lower() and "test" not in line.lower():
                            findings.append({
                                "severity": "High",
                                "title": f"Secret in git history: {secret_type}",
                                "file": current_file,
                                "line": 0, # Cannot easily determine exact line in old commit without more context
                                "description": f"Potential {secret_type} found in commit {current_commit}.",
                                "remediation": "Rotate the compromised secret immediately. Consider rewriting git history using BFG or filter-repo if the secret is sensitive.",
                                "estimated_effort": "High"
                            })
                            # Only report once per file/commit combination to avoid spam
                            break
    except Exception as e:
        print(f"Error scanning git history: {e}")

    return findings

def create_issue(finding: Dict[str, Any]):
    title = f"[CISO Audit] {finding['severity']}: {finding['title']} in {finding['file']}"
    body = f"""**File:** {finding['file']}:{finding['line']}
**Severity:** {finding['severity']}

**Description:**
{finding['description']}

**Remediation:**
{finding['remediation']}

**Estimated Effort:**
{finding.get('estimated_effort', 'Unknown')}
"""

    # Check if gh is installed
    try:
        result = subprocess.run(["gh", "--version"], capture_output=True, text=True)
        if result.returncode == 0:
            # gh is available, use it
            subprocess.run(["gh", "issue", "create", "--title", title, "--body", body, "--label", "security"], check=True)
            print(f"Created GitHub Issue: {title}")
            return
    except FileNotFoundError:
        pass
    except subprocess.CalledProcessError as e:
         print(f"Failed to create GitHub Issue: {e}")
         return

    # Fallback to mock issues
    if not os.path.exists(ISSUES_DIR):
        os.makedirs(ISSUES_DIR)

    safe_title = re.sub(r'[^a-zA-Z0-9_\-]', '_', finding['title'])
    safe_file = re.sub(r'[^a-zA-Z0-9_\-]', '_', finding['file'])
    issue_filename = f"{ISSUES_DIR}/{safe_title}_{safe_file}.md"

    with open(issue_filename, "w") as f:
        f.write(f"# {title}\n\n{body}")
    print(f"Created mock issue: {issue_filename}")

def main():
    findings = []

    files_to_scan = get_files_to_scan()
    print(f"Scanning {len(files_to_scan)} files...")

    findings.extend(check_hardcoded_secrets_and_unsafe(files_to_scan))
    findings.extend(check_unsanitized_inputs(files_to_scan))
    findings.extend(check_cargo_audit())
    findings.extend(check_git_history())

    # Sort findings by severity
    severity_order = {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}
    findings.sort(key=lambda x: severity_order.get(x["severity"], 4))

    # Generate report
    with open(REPORT_FILE, "w") as f:
        f.write("# Security Audit Report\n\n")
        f.write("## Findings\n\n")
        if not findings:
            f.write("No findings to report.\n")
        else:
            for finding in findings:
                f.write(f"### {finding['severity']}: {finding['title']}\n")
                f.write(f"- **File:** {finding['file']}:{finding['line']}\n")
                f.write(f"- **Description:** {finding['description']}\n")
                f.write(f"- **Remediation:** {finding['remediation']}\n")
                f.write(f"- **Estimated Effort:** {finding.get('estimated_effort', 'Unknown')}\n\n")

                # Create issues for Critical and High findings
                if finding["severity"] in ["Critical", "High"]:
                    create_issue(finding)

    print(f"Audit complete. Report generated at {REPORT_FILE}")

if __name__ == "__main__":
    main()
