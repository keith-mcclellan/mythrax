#!/usr/bin/env python3
import os
import re
import subprocess
import json
import shlex

def get_files():
    files = []
    # Include root (.) which will naturally traverse mythrax-core
    for root, dirs, filenames in os.walk('.'):
        # Modify dirs in-place to prevent os.walk from descending into them
        for d in ['target', '.git', '.venv']:
            if d in dirs:
                dirs.remove(d)

        for f in filenames:
            if f.endswith('.rs') or f == 'Cargo.toml' or f == 'Cargo.lock':
                files.append(os.path.join(root, f))
    return files

def check_hardcoded_secrets(files):
    findings = []
    # Basic patterns for secrets
    patterns = [
        re.compile(r'token\s*=\s*["\'][a-zA-Z0-9_\-]{16,}["\']', re.IGNORECASE),
        re.compile(r'api_key\s*=\s*["\'][a-zA-Z0-9_\-]{16,}["\']', re.IGNORECASE),
        re.compile(r'password\s*=\s*["\'][^"\']+["\']', re.IGNORECASE),
        re.compile(r'["\']ghp_[a-zA-Z0-9]{36}["\']'), # github token
        re.compile(r'["\']sk-[a-zA-Z0-9]{48}["\']'), # openai
    ]
    for file in files:
        if not file.endswith('.rs') and not file.endswith('.toml'): continue
        with open(file, 'r', encoding='utf-8', errors='ignore') as f:
            for i, line in enumerate(f):
                for p in patterns:
                    if p.search(line):
                        findings.append({
                            "type": "hardcoded_secret",
                            "severity": "Critical",
                            "file": file,
                            "line": i+1,
                            "content": line.strip(),
                            "remediation": "Move secrets to environment variables or a secure vault.",
                            "effort": "Low"
                        })
    return findings

def check_unsafe_blocks(files):
    findings = []
    unsafe_pattern = re.compile(r'unsafe\s*\{')
    for file in files:
        if not file.endswith('.rs'): continue
        with open(file, 'r', encoding='utf-8', errors='ignore') as f:
            lines = f.readlines()
            for i, line in enumerate(lines):
                if unsafe_pattern.search(line):
                    # Check if there is a justification comment above
                    has_comment = False
                    for j in range(max(0, i-5), i):
                        if '//' in lines[j] and ('SAFETY:' in lines[j] or 'justify' in lines[j].lower()):
                            has_comment = True
                            break
                    if not has_comment:
                        findings.append({
                            "type": "unsafe_block_unjustified",
                            "severity": "High",
                            "file": file,
                            "line": i+1,
                            "content": line.strip(),
                            "remediation": "Memory safety risk: Unsafe Rust bypasses compiler guarantees. Add a documented SAFETY comment justifying the unsafe block.",
                            "effort": "Medium"
                        })
                    else:
                        findings.append({
                            "type": "unsafe_block_justified",
                            "severity": "Low",
                            "file": file,
                            "line": i+1,
                            "content": line.strip(),
                            "remediation": "Memory safety risk: Unsafe Rust bypasses compiler guarantees. Ensure the justification remains valid.",
                            "effort": "Low"
                        })
    return findings

def check_command_injection(files):
    findings = []
    cmd_pattern = re.compile(r'Command::new\(')
    arg_pattern = re.compile(r'\.arg\(')
    args_pattern = re.compile(r'\.args\(')

    for file in files:
        if not file.endswith('.rs'): continue
        with open(file, 'r', encoding='utf-8', errors='ignore') as f:
            lines = f.readlines()
            for i, line in enumerate(lines):
                if cmd_pattern.search(line):
                    findings.append({
                        "type": "command_invocation",
                        "severity": "Medium",
                        "file": file,
                        "line": i+1,
                        "content": line.strip(),
                        "remediation": "Ensure external process invocations do not use untrusted input. Sanitize inputs.",
                        "effort": "High"
                    })
                if arg_pattern.search(line) or args_pattern.search(line):
                    # Look ahead a bit to see if it's taking a variable
                    if not re.search(r'\.args?\(\s*".*"\s*\)', line) and not re.search(r'\.args?\(\s*&\[.*\]\s*\)', line):
                        findings.append({
                            "type": "command_injection",
                            "severity": "High",
                            "file": file,
                            "line": i+1,
                            "content": line.strip(),
                            "remediation": "Ensure external process arguments are strictly validated and not untrusted input.",
                            "effort": "High"
                        })
    return findings

def check_dependencies():
    findings = []
    try:
        # Run in mythrax-core where Cargo.toml is located
        audit_out = subprocess.run(["cargo", "audit", "--json"], cwd="mythrax-core", capture_output=True, text=True)
        if audit_out.stdout:
            try:
                audit_data = json.loads(audit_out.stdout)
                for vuln in audit_data.get('vulnerabilities', {}).get('list', []):
                    findings.append({
                        "type": "dependency_cve",
                        "severity": "Critical",
                        "file": "Cargo.toml",
                        "line": 0,
                        "content": f"{vuln.get('package', {}).get('name')} {vuln.get('package', {}).get('version')}: {vuln.get('advisory', {}).get('title')} ({vuln.get('advisory', {}).get('id')})",
                        "remediation": "Update to a patched version.",
                        "effort": "Low"
                    })
                for warn_type, warns in audit_data.get('warnings', {}).items():
                    for w in warns:
                        findings.append({
                            "type": "dependency_warning",
                            "severity": "High", # Flag yanked crates
                            "file": "Cargo.toml",
                            "line": 0,
                            "content": f"{w.get('package', {}).get('name')} {w.get('package', {}).get('version')}: {w.get('kind')} - {w.get('advisory', {}).get('title', 'yanked or unmaintained')}",
                            "remediation": "Replace yanked or unmaintained crate.",
                            "effort": "Medium"
                        })
            except json.JSONDecodeError:
                pass
    except Exception as e:
        print(f"Cargo audit failed: {e}")
    return findings

def check_git_history():
    findings = []
    try:
        # Patterns for git log -G
        regexes = [
            r'ghp_[a-zA-Z0-9]{36}',
            r'sk-[a-zA-Z0-9]{48}',
            r'(?i)token\s*=\s*["\'][a-zA-Z0-9_\-]{16,}["\']',
            r'(?i)api_key\s*=\s*["\'][a-zA-Z0-9_\-]{16,}["\']'
        ]

        for regex in regexes:
            # -E is for extended regex, -i is ignored by -G but we handle (?i) manually
            # Using -P (Perl-compatible) is recommended in memories
            cmd = ["git", "log", "-G", regex, "-P", "--name-status", "--oneline"]
            res = subprocess.run(cmd, capture_output=True, text=True)
            if res.stdout:
                # We won't parse every line perfectly here for brevity, just flag the commits
                commits = set([line.split()[0] for line in res.stdout.splitlines() if len(line) > 0 and len(line.split()[0]) >= 7])
                for commit in commits:
                    findings.append({
                        "type": "git_history_secret",
                        "severity": "Critical",
                        "file": "Git History",
                        "line": 0,
                        "content": f"Secret pattern matched in commit {commit}",
                        "remediation": "Rotate compromised credentials and rewrite git history.",
                        "effort": "High"
                    })
    except Exception as e:
        print(f"Git history check failed: {e}")
    return findings

def format_report(findings):
    report_lines = [
        "# Security Advisory Report",
        "",
        "## Summary",
        f"Total findings: {len(findings)}",
        ""
    ]

    # Sort by severity
    severity_order = {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}
    findings.sort(key=lambda x: severity_order.get(x['severity'], 4))

    for f in findings:
        report_lines.append(f"### [{f['severity']}] {f['type']}")
        report_lines.append(f"- **File:** `{f['file']}` (Line: {f['line']})")
        report_lines.append(f"- **Content:** `{f['content']}`")
        report_lines.append(f"- **Remediation:** {f['remediation']}")
        report_lines.append(f"- **Estimated Effort:** {f['effort']}")
        report_lines.append("")

    return "\n".join(report_lines)

def file_issues(findings):
    for f in findings:
        if f['severity'] in ['Critical', 'High']:
            title = f"Security Vulnerability: [{f['severity']}] {f['type']} in {os.path.basename(f['file'])}"
            body = (
                f"**Severity:** {f['severity']}\n"
                f"**File:** `{f['file']}`\n"
                f"**Line:** {f['line']}\n"
                f"**Content:** `{f['content']}`\n"
                f"**Remediation:** {f['remediation']}\n"
                f"**Estimated Effort:** {f['effort']}\n"
            )

            # Using shlex.quote to prevent shell injection (memory directive)
            # We use subprocess.run with arguments array instead of shell=True to avoid injection
            cmd = ["gh", "issue", "create", "--title", title, "--body", body]
            try:
                subprocess.run(cmd, capture_output=True, text=True, check=True)
                print(f"Filed issue for {f['type']}")
            except FileNotFoundError:
                print(f"Mocking issue creation (gh cli not found): {title}")
            except Exception as e:
                print(f"Failed to file issue: {e}")

if __name__ == "__main__":
    files = get_files()

    findings = []
    findings.extend(check_hardcoded_secrets(files))
    findings.extend(check_unsafe_blocks(files))
    findings.extend(check_command_injection(files))
    findings.extend(check_dependencies())
    findings.extend(check_git_history())

    report_content = format_report(findings)
    with open("security_advisory_report.md", "w", encoding='utf-8') as f:
        f.write(report_content)

    print("Generated security_advisory_report.md")
    print(report_content)

    file_issues(findings)
