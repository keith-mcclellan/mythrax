#!/usr/bin/env python3
import os
import re
import subprocess
import json
import sys
import shlex

def get_files_to_scan(root_dirs):
    files_to_scan = []
    for root_dir in root_dirs:
        for dirpath, dirnames, filenames in os.walk(root_dir):
            if 'target' in dirnames:
                dirnames.remove('target')
            if '.git' in dirnames:
                dirnames.remove('.git')
            if '.venv' in dirnames:
                dirnames.remove('.venv')
            for filename in filenames:
                if filename.endswith(('.rs', '.toml', '.json', '.md', '.env', '.yaml', '.yml')) or filename == '.env':
                    files_to_scan.append(os.path.join(dirpath, filename))
    return files_to_scan

def audit_hardcoded_secrets(files):
    findings = []
    # Basic patterns for hardcoded secrets, tokens, api keys
    patterns = {
        'AWS Access Key': r'AKIA[0-9A-Z]{16}',
        'Generic API Key': r'(?i)(api_key|apikey|secret|token|password)\s*[=:]\s*["\'][a-zA-Z0-9_\-\.]{10,}["\']',
        'Bearer Token': r'Bearer\s+[a-zA-Z0-9_\-\.]+',
        'Absolute Path to Vault': r'/Users/[a-zA-Z0-9_]+/mythrax-vault'
    }

    for filepath in files:
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            for i, line in enumerate(f):
                for name, pattern in patterns.items():
                    if re.search(pattern, line):
                        # Filter out some false positives for Generic API Key (like X-Mythrax-Token in comments, but let's be strict for CISO)
                        if "example" in line.lower() or "dummy" in line.lower():
                            continue
                        findings.append({
                            'severity': 'Critical',
                            'title': f'Hardcoded Secret ({name})',
                            'file': filepath,
                            'line': i + 1,
                            'description': f'Found potential hardcoded secret matching {name} pattern.',
                            'remediation': 'Remove the hardcoded secret and use an environment variable or secure secret manager.',
                            'estimated_effort': 'Medium'
                        })
    return findings

def audit_unsafe_rust(files):
    findings = []
    for filepath in files:
        if not filepath.endswith('.rs'):
            continue
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            lines = f.readlines()
            for i, line in enumerate(lines):
                if re.search(r'\bunsafe\s*\{', line) or re.search(r'\bunsafe\s+fn\b', line):
                    # Look back up to 3 lines for a SAFETY comment
                    has_safety_comment = False
                    start_lookback = max(0, i - 3)
                    for j in range(start_lookback, i):
                        if 'SAFETY:' in lines[j]:
                            has_safety_comment = True
                            break
                    if not has_safety_comment:
                        findings.append({
                            'severity': 'High',
                            'title': 'Unjustified Unsafe Rust Block',
                            'file': filepath,
                            'line': i + 1,
                            'description': 'Found an `unsafe` block or function without a preceding `SAFETY:` comment justifying its memory safety.',
                            'remediation': 'Add a `// SAFETY:` comment explaining why this unsafe block is sound, or refactor to safe Rust.',
                            'estimated_effort': 'High'
                        })
    return findings

def audit_process_command(files):
    findings = []
    for filepath in files:
        if not filepath.endswith('.rs'):
            continue
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            lines = f.readlines()
            for i, line in enumerate(lines):
                if 'std::process::Command::new' in line:
                    # Look ahead a few lines to check for .arg() or .args() with variables
                    lookahead = min(len(lines), i + 10)
                    uses_args = False
                    for j in range(i, lookahead):
                        # Very simple heuristic: if we see .arg(var) or .args(var) where var is not just a string literal
                        if re.search(r'\.args?\([^")]+\)', lines[j]):
                            uses_args = True
                            break

                    description = 'Execution path accepting external process via `std::process::Command::new`.'
                    if uses_args:
                        description += ' Variable arguments detected. Ensure strict sanitization and boundary enforcement.'
                    else:
                        description += ' Verify no untrusted input can manipulate this path.'

                    findings.append({
                        'severity': 'High',
                        'title': 'External Process Invocation',
                        'file': filepath,
                        'line': i + 1,
                        'description': description,
                        'remediation': 'Ensure all inputs passed as arguments are strictly validated, sanitized, and not vulnerable to command injection. Consider native library alternatives.',
                        'estimated_effort': 'Medium'
                    })
    return findings

def audit_dependencies():
    findings = []
    try:
        result = subprocess.run(
            ['cargo', 'audit', '--json'],
            cwd='mythrax-core',
            capture_output=True,
            text=True
        )
        if result.stdout:
            audit_data = json.loads(result.stdout)
            for vuln in audit_data.get('vulnerabilities', {}).get('list', []):
                advisory = vuln.get('advisory', {})
                findings.append({
                    'severity': 'Critical',
                    'title': f"Vulnerable Dependency: {vuln.get('package', {}).get('name')}",
                    'file': 'mythrax-core/Cargo.toml',
                    'line': 0,
                    'description': f"CVE: {advisory.get('id')} - {advisory.get('title')}",
                    'remediation': f"Update crate to a patched version: {', '.join(advisory.get('patched_versions', []))}",
                    'estimated_effort': 'Low'
                })

            # Parse warnings for yanked or unmaintained crates
            for warning in audit_data.get('warnings', {}).values():
                for item in warning:
                    kind = item.get('kind', '')
                    package = item.get('package', {}).get('name', 'Unknown')
                    advisory = item.get('advisory')
                    desc = kind
                    if advisory:
                        desc = f"{kind}: {advisory.get('title', '')}"
                    findings.append({
                        'severity': 'High',
                        'title': f"Unmaintained/Yanked Dependency: {package}",
                        'file': 'mythrax-core/Cargo.toml',
                        'line': 0,
                        'description': desc,
                        'remediation': 'Replace the unmaintained or yanked crate with an active fork or alternative.',
                        'estimated_effort': 'Medium'
                    })
    except Exception as e:
        print(f"Error running cargo audit: {e}", file=sys.stderr)

    return findings

def audit_git_history():
    findings = []
    patterns = {
        'AWS Access Key': r'AKIA[0-9A-Z]{16}',
        'Generic API Key': r'(?i)(api_key|apikey|secret|token|password)\s*[=:]\s*["\'][a-zA-Z0-9_\-\.]{10,}["\']',
        'Bearer Token': r'Bearer\s+[a-zA-Z0-9_\-\.]+',
    }
    try:
        # Comprehensive scan over all git history
        result = subprocess.run(
            ['git', 'log', '-p', '--all'],
            capture_output=True,
            text=True
        )

        found_types = set()
        for line in result.stdout.splitlines():
            if not line.startswith('+'):
                continue
            for name, pattern in patterns.items():
                if name in found_types:
                    continue # Flag each type only once to avoid massive spam
                if re.search(pattern, line):
                    if "example" in line.lower() or "dummy" in line.lower():
                        continue
                    findings.append({
                        'severity': 'High',
                        'title': f'Secret in Git History ({name})',
                        'file': 'Git History',
                        'line': 0,
                        'description': f'Potential secret ({name}) found in git commit history.',
                        'remediation': 'Rotate the secret immediately and rewrite git history using BFG or git-filter-repo to remove it.',
                        'estimated_effort': 'High'
                    })
                    found_types.add(name)
    except Exception as e:
        print(f"Error checking git history: {e}", file=sys.stderr)
    return findings

def generate_report(findings, report_path):
    severity_order = {'Critical': 0, 'High': 1, 'Medium': 2, 'Low': 3}
    findings.sort(key=lambda x: severity_order.get(x['severity'], 4))

    with open(report_path, 'w') as f:
        f.write("# 🛡️ Security Advisory Report\n\n")
        f.write("Generated by Mythrax Security Audit.\n\n")

        if not findings:
            f.write("✅ No security issues found.\n")
            return

        for finding in findings:
            f.write(f"## [{finding['severity']}] {finding['title']}\n")
            f.write(f"- **File:** `{finding['file']}` (Line: {finding['line']})\n")
            f.write(f"- **Description:** {finding['description']}\n")
            f.write(f"- **Remediation:** {finding['remediation']}\n")
            f.write(f"- **Estimated Effort:** {finding['estimated_effort']}\n\n")

def file_github_issues(findings):
    if not os.environ.get('GITHUB_TOKEN'):
        print("GITHUB_TOKEN not found, skipping issue creation.")
        # But we are in a local environment where `gh` might not be available. We file mock issues if gh is missing.
        if subprocess.run(['which', 'gh'], capture_output=True).returncode != 0:
            print("gh CLI not found. Generating mock issues.")
            os.makedirs('issues', exist_ok=True)
            for i, finding in enumerate(findings):
                if finding['severity'] in ['Critical', 'High']:
                    issue_title = f"🛡️ Sentinel: [{finding['severity']}] Fix {finding['title']} in {os.path.basename(finding['file'])}"
                    issue_body = (
                        f"🚨 Severity: {finding['severity']}\n"
                        f"💡 Vulnerability: {finding['description']} at {finding['file']}:{finding['line']}\n"
                        f"🎯 Impact: Potential security compromise.\n"
                        f"🔧 Fix: {finding['remediation']}\n"
                        f"✅ Verification: Run security audit script.\n"
                        f"Estimated Effort: {finding['estimated_effort']}\n"
                    )
                    with open(f"issues/mock_issue_{i}.md", 'w') as f:
                        f.write(f"Title: {issue_title}\n\n{issue_body}")
            return

    for finding in findings:
        if finding['severity'] in ['Critical', 'High']:
            title = f"🛡️ Sentinel: [{finding['severity']}] Fix {finding['title']} in {os.path.basename(finding['file'])}"
            body = (
                f"🚨 Severity: {finding['severity']}\n"
                f"💡 Vulnerability: {finding['description']} at {finding['file']}:{finding['line']}\n"
                f"🎯 Impact: Potential security compromise.\n"
                f"🔧 Fix: {finding['remediation']}\n"
                f"✅ Verification: Run security audit script.\n"
                f"Estimated Effort: {finding['estimated_effort']}\n"
            )
            try:
                cmd = ['gh', 'issue', 'create', '--title', title, '--body', body, '--label', 'security']
                subprocess.run(cmd, check=True)
                print(f"Filed issue: {title}")
            except subprocess.CalledProcessError as e:
                print(f"Failed to file issue: {e}", file=sys.stderr)

def main():
    # Scan the top level directory to catch configs like Cargo.toml, .env, etc. (which includes mythrax-core)
    root_dirs = ['.']
    files_to_scan = get_files_to_scan(root_dirs)

    findings = []
    print("Auditing for hardcoded secrets...")
    findings.extend(audit_hardcoded_secrets(files_to_scan))

    print("Auditing for unsafe Rust...")
    findings.extend(audit_unsafe_rust(files_to_scan))

    print("Auditing for external process invocations...")
    findings.extend(audit_process_command(files_to_scan))

    print("Auditing for vulnerable dependencies...")
    findings.extend(audit_dependencies())

    print("Auditing git history for secrets...")
    findings.extend(audit_git_history())

    report_path = 'security_advisory_report.md'
    print(f"Generating report to {report_path}...")
    generate_report(findings, report_path)

    print("Filing GitHub Issues for Critical/High findings...")
    file_github_issues(findings)

    print("Audit complete.")

if __name__ == '__main__':
    main()
