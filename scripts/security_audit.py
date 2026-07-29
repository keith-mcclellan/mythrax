import os
import re
import json
import subprocess
import shlex

def scan_files():
    findings = []

    secret_pattern = re.compile(r'(?i)(token|api_key|secret|password|credential|bearer)\s*[:=]\s*["\'][a-zA-Z0-9_\-\.]{10,}["\']')
    unsafe_pattern = re.compile(r'unsafe\s*\{')
    safety_pattern = re.compile(r'SAFETY:')

    command_new_pattern = re.compile(r'std::process::Command::new')

    for root, dirs, files in os.walk('.'):
        dirs[:] = [d for d in dirs if d not in ('target', '.git', '.venv')]

        for file in files:
            filepath = os.path.join(root, file)
            try:
                with open(filepath, 'r', encoding='utf-8') as f:
                    content = f.read()
                    lines = content.split('\n')

                    for i, line in enumerate(lines):
                        if secret_pattern.search(line):
                            findings.append({
                                'type': 'Hardcoded Secret',
                                'severity': 'Critical',
                                'file': filepath,
                                'line': i + 1,
                                'details': 'Hardcoded secret found.',
                                'remediation': 'Use environment variables or a secrets manager.',
                                'estimated_effort': 'Low'
                            })

                        if filepath.endswith('.rs'):
                            if unsafe_pattern.search(line):
                                context = '\n'.join(lines[max(0, i-5):i+1])
                                has_safety = safety_pattern.search(context)

                                severity = 'Low' if has_safety else 'High'
                                details = 'Unsafe block missing SAFETY: documentation.' if not has_safety else 'Unsafe block with SAFETY documentation.'
                                remediation = 'Add a SAFETY: comment explaining why the unsafe block is memory-safe.' if not has_safety else 'No action needed.'

                                findings.append({
                                    'type': 'Unsafe Rust Block',
                                    'severity': severity,
                                    'file': filepath,
                                    'line': i + 1,
                                    'details': f"{details} Memory safety risk: Bypasses Rust's borrow checker and type safety guarantees, potentially leading to memory corruption, undefined behavior, or data races.",
                                    'remediation': remediation,
                                    'estimated_effort': 'Low'
                                })

                            if command_new_pattern.search(line):
                                context = '\n'.join(lines[i:min(len(lines), i+10)])
                                if re.search(r'\.args?\(\s*[^"\'&]', context):
                                    findings.append({
                                        'type': 'Unsanitized Input in Command',
                                        'severity': 'High',
                                        'file': filepath,
                                        'line': i + 1,
                                        'details': 'Potential unsanitized input passed to std::process::Command.',
                                        'remediation': 'Sanitize external inputs or use strict allowlists before passing to Command.',
                                        'estimated_effort': 'Medium'
                                    })
            except (UnicodeDecodeError, FileNotFoundError):
                pass

    return findings

def check_cargo_audit():
    findings = []
    try:
        res = subprocess.run(['cargo', 'audit', '--json'], cwd='mythrax-core', capture_output=True, text=True)
        if res.stdout.strip():
            try:
                data = json.loads(res.stdout)
                vulns = data.get('vulnerabilities', {}).get('list', [])
                for vuln in vulns:
                    adv = vuln.get('advisory', {})
                    findings.append({
                        'type': 'Vulnerable Dependency',
                        'severity': 'High',
                        'file': 'mythrax-core/Cargo.lock',
                        'line': 0,
                        'details': f"CVE found in {vuln.get('package', {}).get('name', 'unknown')}: {adv.get('title', '')}",
                        'remediation': 'Update dependency to a safe version.',
                        'estimated_effort': 'Low'
                    })

                warnings = data.get('warnings', {})
                for warning_type, warning_list in warnings.items():
                    for warn in warning_list:
                        pkg = warn.get('package', {}).get('name', 'unknown')
                        kind = warn.get('kind', 'unknown')
                        findings.append({
                            'type': 'Dependency Warning',
                            'severity': 'Medium',
                            'file': 'mythrax-core/Cargo.lock',
                            'line': 0,
                            'details': f"Warning ({kind}) for package {pkg}",
                            'remediation': 'Replace yanked or unmaintained crate.',
                            'estimated_effort': 'Medium'
                        })
            except json.JSONDecodeError:
                pass
    except Exception as e:
        print(f"Error running cargo audit: {e}")

    return findings

def check_git_history():
    findings = []
    try:
        pattern = r"(token|api_key|secret|password|credential|bearer)\s*[:=]\s*[\"'][a-zA-Z0-9_\-\.]{10,}[\"']"
        res = subprocess.run(['git', 'log', '-G', pattern, '-i', '-P', '--format=%H'], capture_output=True, text=True)
        commits = res.stdout.strip().split('\n')
        commits = [c for c in commits if c]
        # Avoid duplicate commits
        commits = list(set(commits))
        for commit in commits:
            findings.append({
                'type': 'Secret in Git History',
                'severity': 'Critical',
                'file': f"Commit: {commit}",
                'line': 0,
                'details': f"Potential secret found in git history at commit {commit}",
                'remediation': 'Rewrite git history to remove the secret or rotate the credential immediately.',
                'estimated_effort': 'High'
            })
    except Exception as e:
        print(f"Error checking git history: {e}")
    return findings

def create_github_issue(title, body):
    if not os.environ.get('GH_TOKEN') and not os.environ.get('GITHUB_TOKEN'):
        print(f"MOCK: Would create issue: {title}")
        return

    try:
        # Avoid shell=True to prevent shell injection vulnerabilities
        # We pass arguments as a list.
        search_args = ["gh", "issue", "list", "--search", f"in:title \"{title}\"", "--json", "title"]
        search_res = subprocess.run(search_args, capture_output=True, text=True, check=True)

        issues = []
        if search_res.stdout.strip():
            issues = json.loads(search_res.stdout)

        if any(issue.get('title') == title for issue in issues):
            print(f"Issue already exists: {title}")
            return

        create_args = ["gh", "issue", "create", "--title", title, "--body", body]
        subprocess.run(create_args, check=True)
        print(f"Created issue: {title}")
    except subprocess.CalledProcessError as e:
        print(f"Error creating/searching issue: {e}")
    except json.JSONDecodeError as e:
        print(f"Error parsing gh issue list output: {e}")

def main():
    findings = []
    findings.extend(scan_files())
    findings.extend(check_cargo_audit())
    findings.extend(check_git_history())

    severity_order = {'Critical': 0, 'High': 1, 'Medium': 2, 'Low': 3}
    findings.sort(key=lambda x: severity_order.get(x['severity'], 4))

    with open('security_advisory_report.md', 'w', encoding='utf-8') as f:
        f.write('# Security Advisory Report\n\n')
        for idx, finding in enumerate(findings):
            f.write(f"## {idx + 1}. [{finding['severity']}] {finding['type']}\n")
            f.write(f"- **File:** {finding['file']}:{finding['line']}\n")
            f.write(f"- **Details:** {finding['details']}\n")
            f.write(f"- **Remediation:** {finding['remediation']}\n")
            f.write(f"- **Estimated Effort:** {finding['estimated_effort']}\n\n")

            if finding['severity'] in ['Critical', 'High']:
                title = f"[SECURITY] {finding['type']} in {finding['file']}"
                body = f"**Severity:** {finding['severity']}\n**File:** {finding['file']}:{finding['line']}\n**Details:** {finding['details']}\n**Remediation:** {finding['remediation']}\n**Estimated Effort:** {finding['estimated_effort']}"
                create_github_issue(title, body)

if __name__ == '__main__':
    main()
