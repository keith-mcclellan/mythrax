import os
import json
import subprocess
import urllib.request
import re
from datetime import datetime

def get_rust_files(root_dir="mythrax-core"):
    files = []
    for root, _, filenames in os.walk(root_dir):
        if 'target' in root or '.git' in root:
            continue
        for filename in filenames:
            if filename.endswith('.rs'):
                files.append(os.path.join(root, filename))
    return files

def check_hardcoded_secrets():
    findings = []
    patterns = [
        r'(api[_-]?key|secret|token|password)\s*[:=]\s*["\'][a-zA-Z0-9_\-]{16,}["\']',
        r'ghp_[a-zA-Z0-9]{36}',
        r'sk-[a-zA-Z0-9]{48}'
    ]
    for root, _, filenames in os.walk("mythrax-core"):
        if 'target' in root or '.git' in root:
            continue
        for filename in filenames:
            filepath = os.path.join(root, filename)
            try:
                with open(filepath, 'r', encoding='utf-8') as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        for pattern in patterns:
                            if re.search(pattern, line, re.IGNORECASE):
                                if "example" in line.lower() or "dummy" in line.lower() or "mock" in line.lower():
                                    continue
                                findings.append({
                                    "severity": "Critical",
                                    "title": f"Hardcoded Secret in {filepath}",
                                    "description": f"Found potential secret in `{filepath}` at line {i+1}:\n```\n{line.strip()}\n```",
                                    "remediation": "Remove the secret and load it from environment variables or a secure vault.",
                                    "effort": "Low"
                                })
            except Exception:
                pass
    return findings

def check_git_history():
    findings = []
    cmd = ["git", "log", "-i", "-G", "api_key|secret|token|password|ghp_|sk-", "-p", "--all"]
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    current_commit = None
    current_file = None
    count = 0
    max_findings = 10

    while True:
        line = p.stdout.readline()
        if not line:
            break
        if line.startswith('commit '):
            current_commit = line.split()[1]
        elif line.startswith('+++ b/'):
            current_file = line[6:].strip()
        elif line.startswith('+') and not line.startswith('+++'):
            lower_line = line.lower()
            if any(x in lower_line for x in ['api_key', 'secret', 'token', 'password', 'ghp_', 'sk-']):
                if re.search(r'(api[_-]?key|secret|token|password)\s*[:=]\s*["\'][a-zA-Z0-9_\-]{16,}["\']|ghp_[a-zA-Z0-9]{36}|sk-[a-zA-Z0-9]{48}', line, re.IGNORECASE):
                    if current_file and 'test' not in current_file.lower() and 'mock' not in current_file.lower():
                        if count < max_findings:
                            findings.append({
                                "severity": "High",
                                "title": f"Secret in Git History ({current_commit[:8]})",
                                "description": f"Found potential secret in git history in commit `{current_commit}` in file `{current_file}`.",
                                "remediation": "Rotate the compromised secret immediately. Consider rewriting git history.",
                                "effort": "Medium"
                            })
                            count += 1
                        else:
                            p.kill()
                            break
    p.stdout.close()
    p.wait()

    unique_findings = []
    seen = set()
    for f in findings:
        key = f['title']
        if key not in seen:
            seen.add(key)
            unique_findings.append(f)

    return unique_findings

def check_unsafe_blocks(rust_files):
    findings = []
    for filepath in rust_files:
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                lines = f.readlines()
                for i, line in enumerate(lines):
                    if 'unsafe {' in line or 'unsafe fn' in line:
                        has_comment = False
                        if i > 0 and lines[i-1].strip().startswith('//'):
                            has_comment = True

                        # Report ALL unsafe blocks per requirements
                        if has_comment:
                            findings.append({
                                "severity": "Low",
                                "title": f"Documented unsafe block in {filepath}",
                                "description": f"Found documented `unsafe` block at `{filepath}:{i+1}`. Memory safety risk: Unsafe blocks bypass compiler checks. It relies on human documentation to guarantee memory safety invariants.\n```rust\n{line.strip()}\n```",
                                "remediation": "Ensure the documentation correctly justifies the sound usage of this block.",
                                "effort": "None"
                            })
                        else:
                            findings.append({
                                "severity": "Medium",
                                "title": f"Undocumented unsafe block in {filepath}",
                                "description": f"Found `unsafe` block without documentation comment at `{filepath}:{i+1}`. Memory safety risk: Unsafe blocks bypass compiler checks. Without documentation, it's impossible to know what invariants the programmer is manually guaranteeing.\n```rust\n{line.strip()}\n```",
                                "remediation": "Add a `// SAFETY: ...` comment explaining why the unsafe block is sound, or rewrite in safe Rust.",
                                "effort": "Low"
                            })
        except Exception:
            pass
    return findings

def check_dependencies():
    findings = []
    audit_path = "mythrax-core/audit_report.json"
    if os.path.exists(audit_path):
        with open(audit_path, 'r') as f:
            try:
                data = json.load(f)
                for vuln in data.get('vulnerabilities', {}).get('list', []):
                    findings.append({
                        "severity": "High" if vuln['advisory']['cvss_v3'] and vuln['advisory']['cvss_v3'].get('base_score', 0) >= 7.0 else "Medium",
                        "title": f"Dependency CVE in {vuln['package']['name']} ({vuln['package']['version']})",
                        "description": f"Vulnerability: {vuln['advisory']['title']}\nURL: {vuln['advisory']['url']}",
                        "remediation": f"Update `{vuln['package']['name']}` to a patched version.",
                        "effort": "Low"
                    })
            except Exception:
                pass

    import toml
    cargo_toml_path = "mythrax-core/Cargo.toml"
    if os.path.exists(cargo_toml_path):
        try:
            with open(cargo_toml_path, 'r') as f:
                cargo_toml = toml.load(f)

            deps = cargo_toml.get('dependencies', {})
            for crate, version_info in deps.items():
                if isinstance(version_info, str):
                    version = version_info
                elif isinstance(version_info, dict) and 'version' in version_info:
                    version = version_info['version']
                else:
                    continue

                try:
                    url = f"https://crates.io/api/v1/crates/{crate}"
                    req = urllib.request.Request(url, headers={'User-Agent': 'mythrax-security-audit'})
                    with urllib.request.urlopen(req, timeout=2) as response:
                        crate_data = json.loads(response.read().decode())

                        # Check yanked status
                        versions = crate_data.get('versions', [])
                        if versions:
                            for v in versions:
                                if v['num'].startswith(version.replace('^', '').replace('~', '')):
                                    if v.get('yanked', False):
                                        findings.append({
                                            "severity": "High",
                                            "title": f"Yanked dependency: {crate} {v['num']}",
                                            "description": f"The dependency `{crate}` version `{v['num']}` has been yanked from crates.io.",
                                            "remediation": f"Update `{crate}` to a non-yanked version.",
                                            "effort": "Low"
                                        })
                                    break

                        # Check stale status (no commits/updates in ~2 years)
                        crate_info = crate_data.get('crate', {})
                        updated_at = crate_info.get('updated_at')
                        if updated_at:
                            # Parse "2023-11-28T09:47:35.340058+00:00"
                            date_str = updated_at.split('T')[0]
                            last_update = datetime.strptime(date_str, "%Y-%m-%d")
                            diff_days = (datetime.now() - last_update).days
                            if diff_days > 730: # 2 years
                                findings.append({
                                    "severity": "Medium",
                                    "title": f"Stale dependency: {crate}",
                                    "description": f"The dependency `{crate}` has not been updated in over 2 years (last updated {date_str}). This may indicate an abandoned project.",
                                    "remediation": f"Evaluate whether to replace `{crate}` with an actively maintained alternative.",
                                    "effort": "High"
                                })

                except Exception:
                    pass
        except Exception:
            pass

    return findings

def check_untrusted_input(rust_files):
    findings = []
    for filepath in rust_files:
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                lines = f.readlines()
                for i, line in enumerate(lines):
                    if '.query(' in line and 'format!(' in line:
                        findings.append({
                            "severity": "Critical",
                            "title": f"NoSQL Injection vulnerability in {filepath}",
                            "description": f"Found potential NoSQL injection using `format!` in query at `{filepath}:{i+1}`.\n```rust\n{line.strip()}\n```",
                            "remediation": "Use parameterized queries with `$` placeholders and `.bind()` instead of `format!` for SurrealDB queries.",
                            "effort": "Medium"
                        })
        except Exception:
            pass
    return findings

def file_github_issue(finding):
    token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    if not token or not repo:
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json",
        "User-Agent": "mythrax-security-audit"
    }

    try:
        req = urllib.request.Request(url + "?state=open", headers=headers)
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode())
            for issue in issues:
                if issue['title'] == finding['title']:
                    return
    except Exception:
        pass

    body = f"## Security Finding: {finding['title']}\n\n"
    body += f"**Severity:** {finding['severity']}\n"
    body += f"**Description:** {finding['description']}\n\n"
    body += f"**Remediation:** {finding['remediation']}\n"
    body += f"**Estimated Effort:** {finding['effort']}\n"

    payload = json.dumps({
        "title": finding['title'],
        "body": body,
        "labels": ["security", finding['severity'].lower()]
    }).encode('utf-8')

    try:
        req = urllib.request.Request(url, data=payload, headers=headers, method='POST')
        urllib.request.urlopen(req)
    except Exception:
        pass

def main():
    rust_files = get_rust_files()
    all_findings = []
    all_findings.extend(check_hardcoded_secrets())
    all_findings.extend(check_git_history())
    all_findings.extend(check_unsafe_blocks(rust_files))
    all_findings.extend(check_dependencies())
    all_findings.extend(check_untrusted_input(rust_files))

    report = "# Security Advisory Report\n\n"
    if not all_findings:
        report += "No vulnerabilities found!\n"
    else:
        severity_order = {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}
        all_findings.sort(key=lambda x: severity_order.get(x['severity'], 4))
        for i, finding in enumerate(all_findings):
            report += f"## {i+1}. [{finding['severity']}] {finding['title']}\n"
            report += f"- **Description:** {finding['description']}\n"
            report += f"- **Remediation:** {finding['remediation']}\n"
            report += f"- **Estimated Effort:** {finding['effort']}\n\n"
            if finding['severity'] in ['Critical', 'High']:
                file_github_issue(finding)

    with open('security_advisory_report.md', 'w') as f:
        f.write(report)

if __name__ == "__main__":
    main()
