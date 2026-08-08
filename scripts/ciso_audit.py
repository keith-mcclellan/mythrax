import os
import re
import json
import subprocess
import shutil

class Findings:
    def __init__(self):
        self.critical = []
        self.high = []
        self.medium = []
        self.low = []

    def add(self, severity, issue_type, file, line, description, remediation, effort):
        finding = {
            'type': issue_type,
            'file': file,
            'line': line,
            'description': description,
            'remediation': remediation,
            'effort': effort
        }
        if severity == 'Critical':
            self.critical.append(finding)
        elif severity == 'High':
            self.high.append(finding)
        elif severity == 'Medium':
            self.medium.append(finding)
        else:
            self.low.append(finding)

def scan_for_secrets(directory, findings):
    print("Scanning for hardcoded secrets...")
    # More robust secret regexes (can be expanded)
    patterns = [
        (re.compile(r'(?i)(?:api_key|apikey|secret|token|password|credential|bearer)\s*[:=]\s*["\']([^"\']{8,})["\']'), "Generic Secret/Token"),
        (re.compile(r'(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{36}'), "GitHub Token"),
        (re.compile(r'(?:sk|pk)_(?:test|live)_[0-9a-zA-Z]{24}'), "Stripe Key"),
        (re.compile(r'xox[baprs]-[0-9]{12}-[0-9]{12}-[a-zA-Z0-9]{24}'), "Slack Token"),
    ]

    for root, dirs, files in os.walk(directory):
        # Exclude directories as per memory guideline
        dirs[:] = [d for d in dirs if d not in ['target', '.git', '.venv', 'node_modules', 'issues', 'tmp']]

        for file in files:
            filepath = os.path.join(root, file)
            # Skip binary and certain files
            if file.endswith(('.lock', '.png', '.jpg', '.db', '.wal', '.jsonl', '.pack', '.idx')) or 'mock_audit_report' in file:
                continue

            try:
                with open(filepath, 'r', encoding='utf-8') as f:
                    for i, line in enumerate(f):
                        for pattern, name in patterns:
                            if pattern.search(line):
                                # Skip obvious test/mock values or false positives
                                if any(x in line.lower() for x in ['example', 'mock', 'dummy', 'test_', 'your_']):
                                    continue

                                findings.add(
                                    'Critical',
                                    f'Hardcoded {name}',
                                    filepath,
                                    i + 1,
                                    f"Found a potential hardcoded {name}.",
                                    "Move this secret to a secure environment variable or secrets manager. Do not commit secrets to source control.",
                                    "1 hour"
                                )
            except UnicodeDecodeError:
                pass

def scan_git_history(findings):
    print("Scanning git history for secrets...")
    try:
        # Use git log -E -i -G as per memory, since -P is not supported.
        # This scans history for common secret variable names and values
        result = subprocess.run(
            ['git', 'log', '-E', '-i', '-G', 'secret|token|key|password|credential', '-p'],
            capture_output=True, text=True
        )
        # In a real tool this would parse patches. Here we look for signs of additions
        output = result.stdout
        # Basic heuristic: if we see additions (+) of secrets
        added_lines = [line for line in output.split('\n') if line.startswith('+') and not line.startswith('+++')]

        # Simplified regex for demonstration
        pattern = re.compile(r'(?i)(?:api_key|apikey|secret|token|password|credential|bearer)\s*[:=]\s*["\']([^"\']{8,})["\']')
        for line in added_lines:
            if pattern.search(line) and not any(x in line.lower() for x in ['example', 'mock', 'dummy', 'test_', 'your_']):
                findings.add(
                    'Critical',
                    'Secret in Git History',
                    'git-history',
                    'N/A',
                    "Found a potential secret committed in git history.",
                    "Rotate the exposed secret immediately. Use BFG or git-filter-repo to rewrite history and remove the secret.",
                    "4 hours"
                )
                # Just add one finding per scan for history to avoid overwhelming
                break
    except Exception as e:
        print(f"Error scanning git history: {e}")

def scan_for_unsafe(directory, findings):
    print("Scanning for unsafe Rust blocks...")
    unsafe_pattern = re.compile(r'\bunsafe\s*\{')

    for root, dirs, files in os.walk(directory):
        dirs[:] = [d for d in dirs if d not in ['target', '.git', '.venv', 'node_modules', 'issues', 'tmp']]
        for file in files:
            if not file.endswith('.rs'):
                continue

            filepath = os.path.join(root, file)
            try:
                with open(filepath, 'r', encoding='utf-8') as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if unsafe_pattern.search(line):
                            # Check for safety comment in previous 3 lines
                            has_comment = False
                            for j in range(max(0, i-3), i):
                                if 'SAFETY:' in lines[j] or 'Safety:' in lines[j]:
                                    has_comment = True
                                    break

                            if not has_comment:
                                findings.add(
                                    'High',
                                    'Unjustified Unsafe Block',
                                    filepath,
                                    i + 1,
                                    "Found an `unsafe` block without a preceding `SAFETY:` or `Safety:` documented justification.",
                                    "Add a `SAFETY:` comment explaining why the unsafe block is memory safe, or refactor to safe Rust.",
                                    "30 minutes"
                                )
                            else:
                                findings.add(
                                    'Low',
                                    'Unsafe Block',
                                    filepath,
                                    i + 1,
                                    "Found a documented `unsafe` block. Potential memory safety risk if justification is flawed.",
                                    "Review the `SAFETY:` justification during code review.",
                                    "10 minutes"
                                )
            except UnicodeDecodeError:
                pass

def run_cargo_audit(findings):
    print("Running cargo audit...")
    try:
        # Check if cargo-audit is installed
        result = subprocess.run(['cargo', 'audit', '--version'], capture_output=True, text=True)
        if result.returncode != 0:
            print("Installing cargo-audit...")
            subprocess.run(['cargo', 'install', 'cargo-audit', '--locked'], check=True)

        # Run in mythrax-core directory as per memory
        result = subprocess.run(['cargo', 'audit', '--json'], cwd='mythrax-core', capture_output=True, text=True)

        # Cargo audit returns non-zero if vulnerabilities are found, so we check stdout instead of returncode
        if result.stdout:
            try:
                audit_data = json.loads(result.stdout)

                # Parse vulnerabilities
                vulns = audit_data.get('vulnerabilities', {}).get('list', [])
                for vuln in vulns:
                    advisory = vuln.get('advisory', {})
                    pkg_name = advisory.get('package', 'unknown')
                    cve = advisory.get('id', 'unknown')
                    desc = advisory.get('title', 'Vulnerability')
                    findings.add(
                        'Critical',
                        f'Dependency Vulnerability ({cve})',
                        'Cargo.lock',
                        'N/A',
                        f"Package `{pkg_name}` has a known vulnerability: {desc}.",
                        f"Update `{pkg_name}` to a patched version using `cargo update -p {pkg_name}`.",
                        "1 hour"
                    )

                # Parse warnings for yanked/unmaintained from the warnings dictionary
                warnings_dict = audit_data.get('warnings', {})
                for warning_type, warnings_list in warnings_dict.items():
                    for warning in warnings_list:
                        pkg = warning.get('package', {})
                        pkg_name = pkg.get('name', 'unknown')
                        kind = warning.get('kind', 'unknown warning')

                        severity = 'High' if kind == 'yanked' else 'Medium'
                        findings.add(
                            severity,
                            f'Dependency Warning ({kind})',
                            'Cargo.lock',
                            'N/A',
                            f"Package `{pkg_name}` is flagged as {kind}.",
                            f"Consider replacing or removing the `{pkg_name}` dependency.",
                            "2 hours"
                        )
            except json.JSONDecodeError:
                print("Failed to parse cargo audit JSON output")
    except Exception as e:
        print(f"Error running cargo audit: {e}")

def scan_untrusted_input(directory, findings):
    print("Scanning for untrusted input execution paths...")
    # Simplified scan for Command::new and reqwest args
    cmd_pattern = re.compile(r'Command::new\([^)]+\)\s*(?:\n\s*)?\.args?\(([^)]+)\)')

    for root, dirs, files in os.walk(directory):
        dirs[:] = [d for d in dirs if d not in ['target', '.git', '.venv', 'node_modules', 'issues', 'tmp']]
        for file in files:
            if not file.endswith('.rs'):
                continue

            filepath = os.path.join(root, file)
            try:
                with open(filepath, 'r', encoding='utf-8') as f:
                    content = f.read()

                    for match in cmd_pattern.finditer(content):
                        arg_text = match.group(1)
                        # Heuristic: if arg is not a string literal, flag it as potential unsanitized input
                        if not arg_text.strip().startswith('"'):
                            line_no = content[:match.start()].count('\n') + 1
                            findings.add(
                                'High',
                                'Potential Unsanitized Input in Command Execution',
                                filepath,
                                line_no,
                                f"Execution path accepts potentially untrusted external input in `Command` args: `{arg_text}`.",
                                "Ensure input is sanitized, strictly validated, and avoid passing user input directly to shell commands.",
                                "2 hours"
                            )
            except UnicodeDecodeError:
                pass

def get_existing_issues():
    if shutil.which('gh') is None:
        return []

    try:
        # Fetch up to 100 recent open issues created by this workflow
        # In a real app we'd use pagination or labels
        result = subprocess.run(
            ['gh', 'issue', 'list', '--state', 'open', '--json', 'title'],
            capture_output=True, text=True
        )
        if result.stdout:
            issues = json.loads(result.stdout)
            return [issue['title'] for issue in issues]
    except subprocess.CalledProcessError as e:
        print(f"Failed to fetch existing issues: {e}")
    except json.JSONDecodeError:
        print("Failed to parse existing issues JSON")
    return []

def generate_report_and_issues(findings):
    print("Generating report and filing issues...")
    report_lines = ["# CISO Security Audit Report\n"]

    total_findings = len(findings.critical) + len(findings.high) + len(findings.medium) + len(findings.low)
    report_lines.append(f"**Total Findings:** {total_findings}\n")
    report_lines.append(f"- **Critical:** {len(findings.critical)}")
    report_lines.append(f"- **High:** {len(findings.high)}")
    report_lines.append(f"- **Medium:** {len(findings.medium)}")
    report_lines.append(f"- **Low:** {len(findings.low)}\n")

    gh_cli_available = shutil.which('gh') is not None
    existing_titles = get_existing_issues() if gh_cli_available else []

    # Process each severity level
    for severity, issues_list in [('Critical', findings.critical), ('High', findings.high), ('Medium', findings.medium), ('Low', findings.low)]:
        if issues_list:
            report_lines.append(f"## {severity} Findings\n")
            for i, issue in enumerate(issues_list):
                title = f"[{severity}] {issue['type']} in {issue['file']}"
                body = f"**File:** {issue['file']}\n**Line:** {issue['line']}\n\n**Description:** {issue['description']}\n\n**Remediation:** {issue['remediation']}\n**Estimated Effort:** {issue['effort']}"

                report_lines.append(f"### {i+1}. {title}")
                report_lines.append(f"{body}\n")

                # File GitHub issue for Critical and High findings if they don't already exist
                if severity in ['Critical', 'High'] and gh_cli_available:
                    if title not in existing_titles:
                        try:
                            print(f"Filing issue for: {title}")
                            subprocess.run(['gh', 'issue', 'create', '--title', title, '--body', body], check=True)
                        except subprocess.CalledProcessError as e:
                            print(f"Failed to file issue '{title}': {e}")
                    else:
                        print(f"Issue already exists for: {title}")
                elif severity in ['Critical', 'High'] and not gh_cli_available:
                    print(f"GitHub CLI not found. Skipping issue creation for: {title}")

    report_content = '\n'.join(report_lines)
    with open('security_audit_report.md', 'w') as f:
        f.write(report_content)

    # Also print the report so it's visible in the GitHub Actions logs
    print("\n--- BEGIN AUDIT REPORT ---\n")
    print(report_content)
    print("\n--- END AUDIT REPORT ---\n")
    print("Report saved to security_audit_report.md")

if __name__ == '__main__':
    print("Starting CISO Security Audit...")
    findings = Findings()

    # Run all scans
    scan_for_secrets('.', findings)
    scan_git_history(findings)
    scan_for_unsafe('.', findings)
    run_cargo_audit(findings)
    scan_untrusted_input('.', findings)

    # Generate report and issues
    generate_report_and_issues(findings)
    print("Audit complete.")
