import urllib.request
import urllib.error
import json
import os
import re
import subprocess
from datetime import datetime
import time

CRITICAL = "Critical"
HIGH = "High"
MEDIUM = "Medium"
LOW = "Low"

class AuditFindings:
    def __init__(self):
        self.findings = []

    def add(self, severity, title, desc, file_path, line, remediation, effort):
        finding = {
            "severity": severity,
            "title": title,
            "desc": desc,
            "file": file_path,
            "line": line,
            "remediation": remediation,
            "effort": effort
        }
        if finding not in self.findings:
            self.findings.append(finding)

def strip_strings_and_comments(line):
    line = re.sub(r'".*?(?<!\\)"', '""', line)
    line = re.sub(r'//.*', '', line)
    return line

def check_unsafe(findings, directory):
    for root, _, files in os.walk(directory):
        for file in files:
            if not file.endswith('.rs'): continue
            path = os.path.join(root, file)
            with open(path, 'r', encoding='utf-8') as f:
                lines = f.readlines()

            for i, line in enumerate(lines):
                stripped = strip_strings_and_comments(line)
                if 'unsafe ' in stripped or 'unsafe{' in stripped:
                    has_comment = False
                    for j in range(max(0, i-5), i):
                        if 'safety' in lines[j].lower() or 'safety:' in lines[j].lower():
                            has_comment = True
                            break

                    desc = f"Unsafe Rust block/keyword usage: `{line.strip()}`. Memory safety risk: Unsafe Rust allows bypassing the compiler's strict memory safety guarantees, which can lead to undefined behavior, memory leaks, use-after-free, or data races if not handled carefully."

                    if not has_comment:
                        findings.add(
                            severity=HIGH,
                            title="Undocumented unsafe block",
                            desc=desc + " This instance lacks a documented safety justification.",
                            file_path=path,
                            line=i+1,
                            remediation="Add a `// Safety: ...` comment explaining why this block is sound, or refactor to safe Rust.",
                            effort="Low"
                        )
                    else:
                        findings.add(
                            severity=MEDIUM,
                            title="Unsafe block usage",
                            desc=desc + " Safety justification is present.",
                            file_path=path,
                            line=i+1,
                            remediation="Review if this unsafe block is strictly necessary.",
                            effort="Medium"
                        )

def check_nosql_injection(findings, directory):
    for root, _, files in os.walk(directory):
        for file in files:
            if not file.endswith('.rs'): continue
            path = os.path.join(root, file)
            with open(path, 'r', encoding='utf-8') as f:
                lines = f.readlines()
            for i, line in enumerate(lines):
                if '.query(' in line:
                    if 'format!' in line or (i>0 and 'format!' in lines[i-1]) or '+' in line:
                        findings.add(
                            severity=CRITICAL,
                            title="Potential NoSQL Injection",
                            desc=f"Dynamic string formatting used in database query: `{line.strip()}`. Accepting untrusted external input without sanitization or boundary enforcement can allow attackers to execute arbitrary NoSQL commands.",
                            file_path=path,
                            line=i+1,
                            remediation="Use parameterized queries with $ placeholders and .bind().",
                            effort="Low"
                        )

def check_secrets_git_history(findings):
    res = subprocess.run(["git", "log", "-G", "password|secret|api_key|token", "-i", "-p", "--name-status"], capture_output=True, text=True)
    if res.returncode == 0:
        lines = res.stdout.split('\n')
        curr_commit = ""
        for line in lines:
            if line.startswith("commit "):
                curr_commit = line.split()[1]
            elif line.startswith("+") and not line.startswith("+++"):
                if re.search(r'(password|secret|api_key|token)\s*=\s*["\'][a-zA-Z0-9_\-]{8,}["\']', line, re.IGNORECASE):
                    findings.add(
                        severity=CRITICAL,
                        title="Secret in Git History",
                        desc=f"Found potential secret in commit {curr_commit}: `{line.strip()}`",
                        file_path="Git History",
                        line=0,
                        remediation="Rotate the secret immediately and remove from git history using filter-repo.",
                        effort="High"
                    )

def check_secrets_source(findings, directory):
    for root, _, files in os.walk(directory):
        for file in files:
            if not file.endswith(('.rs', '.toml', '.json', '.yaml', '.yml')): continue
            path = os.path.join(root, file)
            with open(path, 'r', encoding='utf-8') as f:
                lines = f.readlines()
            for i, line in enumerate(lines):
                if re.search(r'(password|secret|api_key|token)\s*=\s*["\'][a-zA-Z0-9_\-]{8,}["\']', line, re.IGNORECASE):
                    findings.add(
                        severity=CRITICAL,
                        title="Hardcoded Secret",
                        desc=f"Found potential hardcoded secret: `{line.strip()}`",
                        file_path=path,
                        line=i+1,
                        remediation="Remove the secret and load it from environment variables or a secure vault.",
                        effort="High"
                    )

def check_cves(findings, directory):
    res = subprocess.run(["cargo", "audit", "--json"], cwd=directory, capture_output=True, text=True)
    try:
        data = json.loads(res.stdout)

        for vuln in data.get("vulnerabilities", {}).get("list", []):
            adv = vuln.get("advisory", {})
            pkg = vuln.get("package", {})
            title = adv.get("title", "Unknown")
            cve_id = adv.get("id", "Unknown")
            pkg_name = pkg.get("name", "Unknown")

            score = 0.0
            cvss = adv.get("cvss_base_score")
            if cvss is not None:
                score = float(cvss)

            sev = MEDIUM
            if score >= 9.0: sev = CRITICAL
            elif score >= 7.0: sev = HIGH

            findings.add(
                severity=sev,
                title=f"Dependency Vulnerability: {title} in {pkg_name}",
                desc=f"CVE/RUSTSEC: {cve_id}. Dependency with known vulnerability.",
                file_path="Cargo.toml/Cargo.lock",
                line=0,
                remediation=f"Upgrade {pkg_name} to a safe version.",
                effort="Medium"
            )

        for warning_type, warning_list in data.get("warnings", {}).items():
            for warn in warning_list:
                pkg = warn.get("package", {})
                kind = warn.get("kind", "")
                pkg_name = pkg.get("name", "Unknown")
                pkg_version = pkg.get("version", "Unknown")

                sev = HIGH if kind in ["yanked", "unsound", "unmaintained"] else MEDIUM
                findings.add(
                    severity=sev,
                    title=f"Dependency Warning: {kind} package {pkg_name}",
                    desc=f"Package {pkg_name} version {pkg_version} has warning: {kind}. This may indicate a yanked crate or no recent commits.",
                    file_path="Cargo.toml/Cargo.lock",
                    line=0,
                    remediation=f"Replace or upgrade {pkg_name}.",
                    effort="Medium"
                )
    except Exception as e:
        pass

def check_crates_io_stale(findings, cargo_toml_path):
    if not os.path.exists(cargo_toml_path): return
    deps = []
    with open(cargo_toml_path, 'r', encoding='utf-8') as f:
        lines = f.readlines()
    in_deps = False
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith('#'): continue
        if re.match(r'^\[(?:dev-)?dependencies\]', stripped):
            in_deps = True
            continue
        if stripped.startswith('['):
            in_deps = False
            continue
        if in_deps:
            match = re.match(r'^([a-zA-Z0-9_\-]+)\s*=', stripped)
            if match:
                deps.append(match.group(1))

    for dep in deps:
        req = urllib.request.Request(f"https://crates.io/api/v1/crates/{dep}", headers={"User-Agent": "CISO_Audit_Script"})
        try:
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read().decode())
                crate_data = data.get('crate', {})
                updated_at = crate_data.get('updated_at')
                if updated_at:
                    updated_dt = datetime.strptime(updated_at[:10], "%Y-%m-%d")
                    delta = datetime.now() - updated_dt
                    if delta.days > 730:
                        findings.add(
                            severity=HIGH,
                            title=f"Stale Dependency: {dep}",
                            desc=f"The crate {dep} has not been updated on crates.io in over 2 years (last updated {updated_at[:10]}).",
                            file_path=cargo_toml_path,
                            line=0,
                            remediation="Consider replacing this dependency with an actively maintained alternative.",
                            effort="Medium"
                        )
            time.sleep(0.5)
        except Exception:
            pass

def post_github_issue(findings, repo, token):
    url = f"https://api.github.com/repos/{repo}/issues?per_page=100&state=open"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json"
    }

    existing_issues = []
    page = 1
    while True:
        req = urllib.request.Request(f"{url}&page={page}", headers=headers)
        try:
            with urllib.request.urlopen(req) as response:
                page_issues = json.loads(response.read().decode())
                if not page_issues: break
                existing_issues.extend(page_issues)
                if len(page_issues) < 100: break
                page += 1
        except Exception:
            break

    existing_titles = [i["title"] for i in existing_issues if i.get("state") == "open"]

    post_url = f"https://api.github.com/repos/{repo}/issues"
    post_headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }

    for f in findings.findings:
        if f["severity"] in [CRITICAL, HIGH]:
            issue_title = f"[Security Audit] {f['severity']}: {f['title']} in {f['file']} (L{f['line']})"
            if issue_title in existing_titles: continue

            body = f"**Severity**: {f['severity']}\n**Title**: {f['title']}\n**Description**: {f['desc']}\n**File**: `{f['file']}` (Line {f['line']})\n**Remediation**: {f['remediation']}\n**Estimated Effort**: {f['effort']}"
            data = json.dumps({"title": issue_title, "body": body}).encode('utf-8')
            req = urllib.request.Request(post_url, data=data, headers=post_headers)
            try:
                urllib.request.urlopen(req)
                existing_titles.append(issue_title)
            except Exception as e:
                print(f"Failed to post issue: {e}")

if __name__ == "__main__":
    findings = AuditFindings()
    if os.path.exists("mythrax-core"):
        print("Checking unsafe blocks...")
        check_unsafe(findings, "mythrax-core/src")
        print("Checking NoSQL injection...")
        check_nosql_injection(findings, "mythrax-core/src")
        print("Checking secrets in git history...")
        check_secrets_git_history(findings)
        print("Checking secrets in source...")
        check_secrets_source(findings, "mythrax-core")
        print("Checking CVEs...")
        check_cves(findings, "mythrax-core")
        print("Checking crates.io for stale dependencies...")
        check_crates_io_stale(findings, "mythrax-core/Cargo.toml")

    report_path = "security_advisory_report.md"
    with open(report_path, "w") as f:
        f.write("# Security Advisory Report\n\n")
        f.write(f"Generated at: {datetime.now().isoformat()}\n\n")

        for sev in [CRITICAL, HIGH, MEDIUM, LOW]:
            sev_findings = [fi for fi in findings.findings if fi["severity"] == sev]
            if not sev_findings: continue
            f.write(f"## {sev} Findings\n\n")
            for fi in sev_findings:
                f.write(f"### {fi['title']}\n")
                f.write(f"- **Description**: {fi['desc']}\n")
                f.write(f"- **File**: `{fi['file']}` (Line {fi['line']})\n")
                f.write(f"- **Remediation**: {fi['remediation']}\n")
                f.write(f"- **Effort**: {fi['effort']}\n\n")

    print(f"Report written to {report_path}")

    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if token and repo:
        post_github_issue(findings, repo, token)
