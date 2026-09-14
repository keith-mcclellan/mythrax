import os
import sys
import subprocess
import json
import urllib.request
import urllib.error
import re
from datetime import datetime, timezone, timedelta

def run_cmd(cmd, cwd=None):
    result = subprocess.run(cmd, shell=True, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    return result.stdout, result.stderr, result.returncode

def find_files(directory, ext):
    filepaths = []
    for root, _, files in os.walk(directory):
        for f in files:
            if f.endswith(ext):
                filepaths.append(os.path.join(root, f))
    return filepaths

def scan_secrets_source():
    print("Scanning source for secrets...")
    findings = []
    regex = re.compile(r'(?i)(secret|token|api[_-]?key|password|credential)\s*[:=]\s*["\'][^"\']+["\']')
    for filepath in find_files("mythrax-core", ".rs") + find_files("mythrax-core", ".toml") + find_files("mythrax-core", ".json"):
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                lines = f.readlines()
                for i, line in enumerate(lines):
                    if regex.search(line) and "test" not in line.lower():
                        findings.append({
                            "title": f"Hardcoded Secret in {os.path.basename(filepath)}",
                            "severity": "Critical",
                            "description": f"File: `{filepath}:{i+1}`\n```rust\n{line.strip()}\n```",
                            "remediation": "Store secrets in environment variables or a secure vault. Remove from source code.",
                            "effort": "Low"
                        })
        except Exception as e:
            pass
    return findings

def scan_secrets_history():
    print("Scanning git history for secrets...")
    findings = []
    stdout, _, _ = run_cmd("git log -i -G 'secret|token|api_key|apikey|password|credential' --patch mythrax-core")
    if stdout:
        for i, line in enumerate(stdout.split('\n')):
            if line.startswith('+') and not line.startswith('+++'):
                if re.search(r'(?i)(secret|token|api[_-]?key|password|credential)\s*[:=]\s*["\'][^"\']+["\']', line):
                    findings.append({
                        "title": "Secret in Git History",
                        "severity": "Critical",
                        "description": f"Found secret in git history:\n```diff\n{line.strip()}\n```",
                        "remediation": "Rewrite git history to remove the secret, and rotate the compromised credential.",
                        "effort": "High"
                    })
    return findings

def scan_unsafe_blocks():
    print("Scanning for unsafe blocks...")
    findings = []
    for filepath in find_files("mythrax-core", ".rs"):
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                lines = f.readlines()
                for i, line in enumerate(lines):
                    if "unsafe {" in line or "unsafe fn" in line:
                        justified = False
                        for j in range(max(0, i-3), i):
                            if "SAFETY:" in lines[j] or "justification" in lines[j].lower():
                                justified = True
                                break
                        if not justified:
                            findings.append({
                                "title": f"Undocumented Unsafe Block in {os.path.basename(filepath)}",
                                "severity": "High",
                                "description": f"Unsafe code lacks a safety justification comment at `{filepath}:{i+1}`.\n```rust\n{line.strip()}\n```\nMemory safety risk if invariants are not manually upheld.",
                                "remediation": "Add a `// SAFETY: ...` comment explaining why the unsafe block is sound, or rewrite in safe Rust.",
                                "effort": "Medium"
                            })
                        else:
                            findings.append({
                                "title": f"Justified Unsafe Block in {os.path.basename(filepath)}",
                                "severity": "Low",
                                "description": f"Documented unsafe code at `{filepath}:{i+1}`.\n```rust\n{line.strip()}\n```",
                                "remediation": "Review the safety comment to ensure it is sound and valid.",
                                "effort": "Low"
                            })
        except:
            pass
    return findings

def scan_cargo_audit():
    print("Running cargo audit...")
    findings = []
    stdout, _, _ = run_cmd("cargo audit --json", cwd="mythrax-core")
    try:
        data = json.loads(stdout)
        vulnerabilities = data.get("vulnerabilities", {})
        if "list" in vulnerabilities:
            for vuln in vulnerabilities["list"]:
                advisory = vuln.get("advisory", {})
                package = vuln.get("package", {})
                title = advisory.get("title", "Unknown CVE")
                package_name = package.get("name", "Unknown Package")
                findings.append({
                    "title": f"CVE in {package_name}: {title}",
                    "severity": "Critical",
                    "description": f"Vulnerability ID: {advisory.get('id')}\nDescription: {advisory.get('description')}",
                    "remediation": "Update the dependency to a patched version.",
                    "effort": "Low"
                })
    except:
        pass
    return findings

def scan_crates_io():
    print("Scanning crates.io for yanked/stale crates...")
    findings = []
    try:
        with open("mythrax-core/Cargo.toml", 'r') as f:
            content = f.read()
    except:
        return findings

    deps = set()
    for line in content.split('\n'):
        line = line.strip()
        if not line or line.startswith('#'):
            continue

        match = re.match(r'^([a-zA-Z0-9_-]+)\s*=', line)
        if match:
            deps.add(match.group(1))

        match = re.match(r'^\[(?:dev-|build-)?dependencies\.([a-zA-Z0-9_-]+)\]', line)
        if match:
            deps.add(match.group(1))

    import time
    for crate in deps:
        req = urllib.request.Request(f"https://crates.io/api/v1/crates/{crate}", headers={'User-Agent': 'Mythrax-CISO-Bot/1.0'})
        try:
            with urllib.request.urlopen(req, timeout=5) as response:
                if response.status == 200:
                    data = json.loads(response.read().decode('utf-8'))
                    crate_info = data.get("crate", {})
                    versions = data.get("versions", [])
                    if versions and versions[0].get("yanked", False):
                        findings.append({
                            "title": f"Yanked Dependency: {crate}",
                            "severity": "High",
                            "description": f"The crate `{crate}` has been yanked from crates.io.",
                            "remediation": "Switch to an alternative crate or an un-yanked version.",
                            "effort": "Medium"
                        })
                    updated_at = crate_info.get("updated_at")
                    if updated_at:
                        updated_date = datetime.strptime(updated_at.split('T')[0], "%Y-%m-%d")
                        if (datetime.utcnow() - updated_date).days > 730:
                            findings.append({
                                "title": f"Stale Dependency: {crate}",
                                "severity": "Medium",
                                "description": f"The crate `{crate}` has not been updated in over 2 years.",
                                "remediation": "Audit the dependency for potential unpatched issues or replace it.",
                                "effort": "Medium"
                            })
        except:
            pass
        time.sleep(1)

    return findings

def scan_untrusted_input():
    print("Scanning for untrusted input execution paths...")
    findings = []
    for filepath in find_files("mythrax-core", ".rs"):
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                if re.search(r'query\s*\(\s*format!\s*\(', f.read()):
                    findings.append({
                        "title": f"NoSQL Injection Risk in {os.path.basename(filepath)}",
                        "severity": "Critical",
                        "description": f"Found `format!` used within `.query()` in `{filepath}`. This leads to NoSQL injection.",
                        "remediation": "Use parameterized queries with `$` placeholders and `.bind()` instead of formatting strings.",
                        "effort": "Low"
                    })
        except:
            pass
    return findings

def file_github_issues(findings):
    token = os.environ.get("GITHUB_TOKEN")
    if not token:
        print("No GITHUB_TOKEN set, skipping issue creation.")
        return

    repo = os.environ.get("GITHUB_REPOSITORY", "keith-mcclellan/mythrax")

    existing_titles = []
    page = 1
    while True:
        api_url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100&page={page}"
        req = urllib.request.Request(api_url, headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28"
        })
        try:
            with urllib.request.urlopen(req) as response:
                if response.status == 200:
                    issues = json.loads(response.read().decode('utf-8'))
                    if not issues:
                        break
                    existing_titles.extend([issue.get("title") for issue in issues])
                    page += 1
                else:
                    break
        except Exception as e:
            print(f"Failed to fetch existing issues on page {page}: {e}")
            break

    for finding in findings:
        if finding["severity"] not in ["Critical", "High"]:
            continue

        title = finding["title"]
        if title in existing_titles:
            print(f"Issue already exists for: {title}")
            continue

        body = f"## 🛡️ CISO Security Advisory\n\n**Severity:** {finding['severity']}\n**Description:**\n{finding['description']}\n\n**Remediation:** {finding['remediation']}\n**Estimated Effort:** {finding['effort']}"

        payload = json.dumps({"title": title, "body": body, "labels": ["security", "ciso-audit"]}).encode("utf-8")
        req = urllib.request.Request(f"https://api.github.com/repos/{repo}/issues", data=payload, headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "Content-Type": "application/json"
        }, method="POST")

        try:
            with urllib.request.urlopen(req) as response:
                if response.status == 201:
                    print(f"Successfully created issue: {title}")
        except Exception as e:
            print(f"Error creating issue {title}: {e}")

def main():
    findings = []
    findings.extend(scan_secrets_source())
    findings.extend(scan_secrets_history())
    findings.extend(scan_unsafe_blocks())
    findings.extend(scan_cargo_audit())
    findings.extend(scan_crates_io())
    findings.extend(scan_untrusted_input())

    report_path = "security_advisory_report.md"
    with open(report_path, "w") as f:
        f.write("# 🛡️ CISO Security Advisory Report\n\n")
        f.write(f"Generated at: {datetime.now(timezone.utc).isoformat()}\n\n")
        for finding in sorted(findings, key=lambda x: {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}.get(x["severity"], 4)):
            f.write(f"## [{finding['severity']}] {finding['title']}\n")
            f.write(f"{finding['description']}\n\n")
            f.write(f"**Remediation:** {finding['remediation']}\n")
            f.write(f"**Estimated Effort:** {finding['effort']}\n\n")

    print(f"Generated report at {report_path}")
    file_github_issues(findings)

if __name__ == '__main__':
    main()
