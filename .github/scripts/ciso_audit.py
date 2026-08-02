import os
import re
import json
import subprocess
from datetime import datetime

# Define regular expressions for hardcoded secrets
SECRET_PATTERNS = [
    re.compile(r'(?i)(secret-token)'),
    re.compile(r'(?i)(my-secret-key)'),
    re.compile(r'(?i)(sk-[a-zA-Z0-9]{20,})'),
    re.compile(r'(?i)(api[_-]?key[\s:=]+[\'"][a-zA-Z0-9_-]+[\'"])')
]

# Patterns for git log
GIT_SECRET_PATTERN = r'(?i)(secret|token|api_key|password|sk-[a-zA-Z0-9]{20,})'

def find_hardcoded_secrets(src_dir):
    findings = []
    for root, dirs, files in os.walk(src_dir):
        if 'target' in dirs:
            dirs.remove('target')
        if '.git' in dirs:
            dirs.remove('.git')
        if '.venv' in dirs:
            dirs.remove('.venv')
        for file in files:
            filepath = os.path.join(root, file)
            with open(filepath, 'r', encoding='utf-8') as f:
                try:
                    content = f.readlines()
                    for idx, line in enumerate(content):
                        for p in SECRET_PATTERNS:
                            if p.search(line):
                                findings.append({
                                    "severity": "Critical",
                                    "title": "Hardcoded Secret Detected",
                                    "description": f"Found potential hardcoded secret in `{filepath}` at line {idx+1}.",
                                    "remediation": "Remove the hardcoded secret and replace it with environment variable or configuration file injection.",
                                    "estimated_effort": "Low",
                                    "file": filepath,
                                    "line": idx + 1
                                })
                except Exception:
                    pass
    return findings

def find_unsafe_blocks(src_dir):
    findings = []
    unsafe_pattern = re.compile(r'unsafe\s*\{')
    for root, dirs, files in os.walk(src_dir):
        if 'target' in dirs:
            dirs.remove('target')
        if '.git' in dirs:
            dirs.remove('.git')
        if '.venv' in dirs:
            dirs.remove('.venv')
        for file in files:
            if not file.endswith('.rs'):
                continue
            filepath = os.path.join(root, file)
            with open(filepath, 'r', encoding='utf-8') as f:
                try:
                    content = f.readlines()
                    for idx, line in enumerate(content):
                        if unsafe_pattern.search(line):
                            # Check for safety comment in previous lines (simple heuristic)
                            safety_comment_found = False
                            start_idx = max(0, idx - 5)
                            for check_idx in range(start_idx, idx):
                                if re.search(r'//\s*SAFETY:', content[check_idx]):
                                    safety_comment_found = True
                                    break

                            if not safety_comment_found:
                                findings.append({
                                    "severity": "High",
                                    "title": "Unjustified Unsafe Block",
                                    "description": f"Found `unsafe` block in `{filepath}` at line {idx+1} without a preceding `SAFETY:` comment.",
                                    "remediation": "Review the unsafe block for memory safety risks and add a `// SAFETY: ...` comment explaining why it is safe, or refactor to safe Rust.",
                                    "estimated_effort": "Medium",
                                    "file": filepath,
                                    "line": idx + 1
                                })
                            else:
                                findings.append({
                                    "severity": "Low",
                                    "title": "Justified Unsafe Block",
                                    "description": f"Found `unsafe` block in `{filepath}` at line {idx+1} with a preceding `SAFETY:` comment.",
                                    "remediation": "Monitor to ensure the safety condition remains valid during refactors.",
                                    "estimated_effort": "Low",
                                    "file": filepath,
                                    "line": idx + 1
                                })
                except Exception:
                    pass
    return findings

def find_unsanitized_commands(src_dir):
    findings = []
    # Heuristic: looking for Command::new and .arg passing variable instead of literal string
    command_new = re.compile(r'Command::new\s*\(')
    arg_var = re.compile(r'\.args?\s*\(\s*[^"\'(]')

    for root, dirs, files in os.walk(src_dir):
        if 'target' in dirs:
            dirs.remove('target')
        if '.git' in dirs:
            dirs.remove('.git')
        if '.venv' in dirs:
            dirs.remove('.venv')
        for file in files:
            if not file.endswith('.rs'):
                continue
            filepath = os.path.join(root, file)
            with open(filepath, 'r', encoding='utf-8') as f:
                try:
                    content = f.read()
                    lines = content.split('\n')
                    for idx, line in enumerate(lines):
                        if command_new.search(line) and arg_var.search(line):
                            findings.append({
                                "severity": "High",
                                "title": "Potential Unsanitized External Input in Command",
                                "description": f"Found `Command::new` with potentially unsanitized variable argument in `{filepath}` at line {idx+1}.",
                                "remediation": "Ensure that the arguments passed to external commands are strictly sanitized and validated against allowed values to prevent command injection.",
                                "estimated_effort": "Medium",
                                "file": filepath,
                                "line": idx + 1
                            })
                except Exception:
                    pass
    return findings

def run_cargo_audit():
    findings = []
    try:
        res = subprocess.run(
            ['cargo', 'audit', '--json'],
            cwd='mythrax-core',
            capture_output=True,
            text=True
        )
        # cargo audit returns non-zero if vulnerabilities are found
        output = res.stdout if res.stdout else res.stderr
        if not output:
            return findings

        data = json.loads(output)

        # Parse vulnerabilities
        vulns = data.get('vulnerabilities') or {}
        vuln_list = vulns.get('list', []) if isinstance(vulns, dict) else []
        for vuln in vuln_list:
            findings.append({
                "severity": "High",
                "title": f"Dependency CVE: {vuln.get('advisory', {}).get('id')}",
                "description": f"Package `{vuln.get('package', {}).get('name')}` version `{vuln.get('package', {}).get('version')}` has a known vulnerability: {vuln.get('advisory', {}).get('title')}",
                "remediation": f"Update the crate `{vuln.get('package', {}).get('name')}` to a patched version.",
                "estimated_effort": "Low"
            })

        # Parse warnings (yanked, unmaintained)
        warnings = data.get('warnings')

        if isinstance(warnings, dict):
            for warn_list in warnings.values():
                if isinstance(warn_list, list):
                    for item in warn_list:
                        kind = item.get('kind', 'warning')
                        pkg_name = item.get('package', {}).get('name', 'unknown') if item.get('package') else 'unknown'
                        adv = item.get('advisory') or {}
                        adv_title = adv.get('title', 'No title')

                        findings.append({
                            "severity": "Medium",
                            "title": f"Dependency Warning: {kind.capitalize()} crate {pkg_name}",
                            "description": f"The crate `{pkg_name}` triggered a warning: {adv_title}",
                            "remediation": f"Consider migrating away from `{pkg_name}` or updating to a maintained alternative.",
                            "estimated_effort": "Medium"
                        })
        elif isinstance(warnings, list):
            for item in warnings:
                kind = item.get('kind', 'warning')
                pkg_name = item.get('package', {}).get('name', 'unknown') if item.get('package') else 'unknown'
                adv = item.get('advisory') or {}
                adv_title = adv.get('title', 'No title')

                findings.append({
                    "severity": "Medium",
                    "title": f"Dependency Warning: {kind.capitalize()} crate {pkg_name}",
                    "description": f"The crate `{pkg_name}` triggered a warning: {adv_title}",
                    "remediation": f"Consider migrating away from `{pkg_name}` or updating to a maintained alternative.",
                    "estimated_effort": "Medium"
                })
    except Exception as e:
        print(f"Error running cargo audit: {e}")
        pass
    return findings

def scan_git_history():
    findings = []
    try:
        res = subprocess.run(
            ['git', 'log', '-G', GIT_SECRET_PATTERN, '-P', '--all', '--oneline', '--name-only'],
            capture_output=True,
            text=True
        )
        if res.stdout.strip():
            # Extract unique files
            lines = res.stdout.strip().split('\n')
            files_with_secrets = set()
            for line in lines:
                # A file path in git log --name-only is preceded by the commit line.
                # A commit line starts with the hash. The file paths are the lines following it.
                if len(line) >= 7 and ' ' in line and re.match(r'^[0-9a-f]{7,}', line):
                    continue
                if line.strip():
                    files_with_secrets.add(line.strip())

            for file in files_with_secrets:
                findings.append({
                    "severity": "Critical",
                    "title": "Secret in Git History",
                    "description": f"Potential secret identified in the git history of `{file}`.",
                    "remediation": "Rotate the compromised secret immediately and consider rewriting the git history (e.g., using BFG Repo-Cleaner) to remove it.",
                    "estimated_effort": "High"
                })
    except Exception as e:
        print(f"Error scanning git history: {e}")
        pass
    return findings

def main():
    print("Starting CISO Security Audit...")
    findings = []

    # Run scans
    print("Scanning for hardcoded secrets...")
    findings.extend(find_hardcoded_secrets("."))

    print("Scanning for unsafe blocks...")
    findings.extend(find_unsafe_blocks("."))

    print("Scanning for unsanitized commands...")
    findings.extend(find_unsanitized_commands("."))

    print("Running cargo audit...")
    findings.extend(run_cargo_audit())

    print("Scanning git history...")
    findings.extend(scan_git_history())

    # Sort findings by severity
    severity_order = {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}
    findings.sort(key=lambda x: severity_order.get(x["severity"], 4))

    # Generate report
    report_lines = [
        "# CISO Security Audit Report",
        f"**Date:** {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}",
        "",
        "## Executive Summary",
        f"Total findings: {len(findings)}",
        ""
    ]

    for idx, f in enumerate(findings):
        report_lines.append(f"### {idx+1}. [{f['severity']}] {f['title']}")
        report_lines.append(f"**Description:** {f['description']}")
        report_lines.append(f"**Remediation:** {f['remediation']}")
        report_lines.append(f"**Estimated Effort:** {f['estimated_effort']}")
        if "file" in f:
            report_lines.append(f"**Location:** `{f['file']}` (Line {f.get('line', 'N/A')})")
        report_lines.append("")

    report_content = "\n".join(report_lines)

    with open("ciso_audit_report.md", "w") as f:
        f.write(report_content)

    print("Report generated: ciso_audit_report.md")

    # File GitHub Issues for Critical/High findings
    gh_token = os.environ.get("GH_TOKEN")
    if gh_token:
        print("Checking existing GitHub issues...")
        existing_issues = []
        try:
            res = subprocess.run(
                ['gh', 'issue', 'list', '--json', 'title'],
                capture_output=True, text=True, check=True
            )
            existing_issues_data = json.loads(res.stdout)
            existing_issues = [issue['title'] for issue in existing_issues_data]
        except Exception:
            pass

        print("Filing GitHub issues for Critical and High findings...")
        for f in findings:
            if f["severity"] in ["Critical", "High"]:
                issue_title = f"🛡️ CISO Audit: [{f['severity']}] {f['title']}"
                if "file" in f:
                    issue_title += f" in {f['file']} (Line {f.get('line', 'N/A')})"

                if issue_title in existing_issues:
                    print(f"Issue already exists for: {issue_title}")
                    continue

                issue_body = f"## Description\n{f['description']}\n\n## Remediation\n{f['remediation']}\n\n**Estimated Effort:** {f['estimated_effort']}"
                if "file" in f:
                     issue_body += f"\n\n**Location:** `{f['file']}` (Line {f.get('line', 'N/A')})"

                try:
                    subprocess.run(
                        ['gh', 'issue', 'create', '--title', issue_title, '--body', issue_body, '--label', 'security'],
                        check=True
                    )
                except Exception as e:
                    print(f"Failed to file issue for '{issue_title}': {e}")
    else:
         print("GH_TOKEN not found, skipping issue creation.")

if __name__ == "__main__":
    main()
