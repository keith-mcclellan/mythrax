#!/usr/bin/env python3
import os
import re
import json
import subprocess
import urllib.request
import urllib.error
from urllib.parse import quote

# Regex patterns
SECRET_PATTERN = re.compile(r'(?i)(secret|token|api[_-]?key|password|bearer|auth|sk-[a-zA-Z0-9]{20,})\s*(=|:)\s*[\'"][^\'"]+[\'"]')
UNSAFE_PATTERN = re.compile(r'unsafe\s*\{')
EXEC_PATTERN = re.compile(r'std::process::Command::new\([^)]*\)\s*\.arg[s]?\([^)]*\)')

def scan_files(root_dir):
    findings = {'secrets': [], 'unsafe': [], 'exec': []}

    # 1. Scan git history for secrets
    try:
        git_log = subprocess.check_output(['git', 'log', '-p', '-E', '-i', '-G', r'(sk-[a-zA-Z0-9]{20,}|password\s*=|secret\s*=|bearer\s*=|token\s*=)'], text=True)
        # Simplified extraction for history
        commits = re.findall(r'^commit ([a-f0-9]{40})', git_log, re.MULTILINE)
        if commits:
            findings['secrets'].append(f"Found in git history: Commits {', '.join(commits[:5])}...")
    except Exception as e:
        print(f"Git history scan error: {e}")

    for root, dirs, files in os.walk(root_dir):
        dirs[:] = [d for d in dirs if d not in ('target', '.git', 'node_modules', '.venv')]
        for file in files:
            if not file.endswith('.rs') and not file.endswith('.toml'):
                continue
            filepath = os.path.join(root, file)
            try:
                with open(filepath, 'r', encoding='utf-8') as f:
                    lines = f.readlines()
            except Exception:
                continue

            for i, line in enumerate(lines):
                if SECRET_PATTERN.search(line):
                    findings['secrets'].append(f"{filepath}:{i+1} - Hardcoded secret pattern found.")
                if filepath.endswith('.rs') and UNSAFE_PATTERN.search(line):
                    has_justification = (i > 0 and 'Safety:' in lines[i-1])
                    risk_explanation = "Memory safety risk: unsafe blocks bypass Rust's compiler guarantees. Incorrect invariants here can lead to memory corruption, use-after-free, or undefined behavior."
                    flag = "[UNJUSTIFIED] " if not has_justification else "[JUSTIFIED] "
                    findings['unsafe'].append(f"{flag}{filepath}:{i+1} - {risk_explanation}")

            content = ''.join(lines)
            for match in EXEC_PATTERN.finditer(content):
                if 'sh -c' in content or '.arg("-c")' in content:
                    findings['exec'].append(f"{filepath} - Unsanitized external input passed to shell execution.")
    return findings

def check_audit():
    try:
        subprocess.run(['cargo', 'audit', '--json'], stdout=open('mythrax-core/audit_temp.json', 'w'), cwd='mythrax-core', check=False)
        with open('mythrax-core/audit_temp.json', 'r') as f:
            data = json.load(f)
        cves = [f"{v['advisory']['id']} ({v['package']['name']})" for v in data.get('vulnerabilities', {}).get('list', [])]
        warnings = [f"{w.get('kind')} - {w.get('package', {}).get('name')}" for warns in data.get('warnings', {}).values() for w in warns if isinstance(w, dict)]
        return cves, warnings
    except Exception as e:
        print(f"Audit error: {e}")
        return [], []
    finally:
        if os.path.exists('mythrax-core/audit_temp.json'):
            os.remove('mythrax-core/audit_temp.json')

def issue_exists(title):
    token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY', 'keith-mcclellan/mythrax')
    if not token or not repo:
        return False

    url = f"https://api.github.com/repos/{repo}/issues?state=open&creator=github-actions[bot]"
    headers = {
        'Authorization': f'token {token}',
        'Accept': 'application/vnd.github.v3+json'
    }
    req = urllib.request.Request(url, headers=headers)
    try:
        response = urllib.request.urlopen(req)
        issues = json.loads(response.read().decode())
        for issue in issues:
            if issue['title'] == title:
                return True
    except Exception:
        pass
    return False

def create_issue(title, body, labels):
    token = os.environ.get('GITHUB_TOKEN')
    if not token:
        print(f"Would create issue: {title}")
        return

    if issue_exists(title):
        print(f"Issue already exists: {title}")
        return

    repo = os.environ.get('GITHUB_REPOSITORY', 'keith-mcclellan/mythrax')
    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        'Authorization': f'token {token}',
        'Accept': 'application/vnd.github.v3+json'
    }
    data = json.dumps({'title': title, 'body': body, 'labels': labels}).encode('utf-8')
    req = urllib.request.Request(url, data=data, headers=headers)
    try:
        urllib.request.urlopen(req)
        print(f"Created issue: {title}")
    except urllib.error.URLError as e:
        print(f"Failed to create issue: {e}")

def main():
    print("Running security audit...")
    findings = scan_files('mythrax-core')
    cves, warnings = check_audit()

    report = "# Security Advisory Report\n\n"

    if findings['secrets']:
        report += "## Hardcoded Secrets (High)\n"
        report += "**Remediation Recommendation:** Remove hardcoded secrets. Use environment variables (e.g., `dotenv`) or a secure secret manager. Scrub git history using BFG or git-filter-repo.\n"
        report += "**Estimated Effort:** Low\n\n"
        report += "\n".join(f"- {f}" for f in findings['secrets']) + "\n\n"
        create_issue("Security Audit: Hardcoded Secrets Found", "\n".join(f"- {f}" for f in findings['secrets']), ["security", "high"])

    if findings['unsafe']:
        report += "## Unjustified Unsafe Blocks (Medium)\n"
        report += "**Remediation Recommendation:** Review each block for memory safety invariants and add explicit `// Safety:` comments documenting them.\n"
        report += "**Estimated Effort:** Medium\n\n"
        report += "\n".join(f"- {f}" for f in findings['unsafe']) + "\n\n"

    if findings['exec']:
        report += "## Command Injection Vectors (Critical)\n"
        report += "**Remediation Recommendation:** Do not use `sh -c`. Execute commands directly with validated arguments. Avoid passing unsanitized external input to `Command::new`.\n"
        report += "**Estimated Effort:** High\n\n"
        report += "\n".join(f"- {f}" for f in set(findings['exec'])) + "\n\n"
        create_issue("Security Audit: Command Injection Vectors Found", "\n".join(f"- {f}" for f in set(findings['exec'])), ["security", "critical"])

    if cves or warnings:
        report += "## Dependency Vulnerabilities & Warnings (High/Medium)\n"
        report += "**Remediation Recommendation:** Run `cargo update` to pull patched versions. Replace unmaintained or yanked crates with secure alternatives.\n"
        report += "**Estimated Effort:** Medium to High\n\n"
        if cves:
            report += "**CVEs (High):**\n" + "\n".join(f"- {c}" for c in cves) + "\n\n"
        if warnings:
            report += "**Warnings (Medium):**\n" + "\n".join(f"- {w}" for w in warnings) + "\n\n"
        create_issue("Security Audit: Vulnerable/Unmaintained Dependencies Found", f"CVEs:\n" + "\n".join(f"- {c}" for c in cves) + f"\n\nWarnings:\n" + "\n".join(f"- {w}" for w in warnings), ["security", "high"])

    print(report)

if __name__ == '__main__':
    main()
