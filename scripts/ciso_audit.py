import json
import os
import re
import subprocess
import urllib.request
import datetime

def fetch_crates_info(crate_name, version):
    try:
        req = urllib.request.Request(f"https://crates.io/api/v1/crates/{crate_name}", headers={'User-Agent': 'mythrax-ciso-audit'})
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            updated_at = data['crate']['updated_at']
            is_yanked = False
            for v in data['versions']:
                if v['num'] == version:
                    is_yanked = v['yanked']
                    break
            return updated_at, is_yanked
    except Exception as e:
        return None, False

def run_cargo_audit():
    try:
        if not os.path.exists("mythrax-core/Cargo.lock"):
            return None
        res = subprocess.run(["cargo", "audit", "--json"], cwd="mythrax-core", capture_output=True, text=True)
        return res.stdout
    except Exception as e:
        return None

def main():
    findings = []

    # 1. Hardcoded Secrets (Source)
    secret_regex = re.compile(r'(?i)(secret|token|api_key|password|credential)\s*[:=]\s*["\'][a-zA-Z0-9_\-]+["\']')
    for root, _, files in os.walk("mythrax-core"):
        for f in files:
            if f.endswith((".rs", ".toml", ".json", ".md", ".yaml", ".yml")):
                filepath = os.path.join(root, f)
                try:
                    with open(filepath, "r", encoding="utf-8") as file:
                        for i, line in enumerate(file):
                            if secret_regex.search(line):
                                findings.append({
                                    "severity": "Critical",
                                    "title": f"Hardcoded Secret in {f}",
                                    "description": f"Found potential hardcoded secret at `{filepath}:{i+1}`:\n```\n{line.strip()}\n```",
                                    "remediation": "Move the secret to an environment variable or secure vault.",
                                    "effort": "Low"
                                })
                except Exception:
                    pass

    # 2. Unsafe Rust
    unsafe_regex = re.compile(r'unsafe\s*\{')
    for root, _, files in os.walk("mythrax-core/src"):
        for f in files:
            if f.endswith(".rs"):
                filepath = os.path.join(root, f)
                try:
                    with open(filepath, "r", encoding="utf-8") as file:
                        lines = file.readlines()
                        for i, line in enumerate(lines):
                            if unsafe_regex.search(line):
                                has_comment = False
                                if i > 0 and ("// Safety" in lines[i-1] or "// SAFETY" in lines[i-1] or "// safety" in lines[i-1]):
                                    has_comment = True
                                if not has_comment:
                                    findings.append({
                                        "severity": "High",
                                        "title": f"Undocumented Unsafe Block in {f}",
                                        "description": f"Unsafe block found at `{filepath}:{i+1}` without a 'Safety:' justification comment. Memory safety risk depends on contents.",
                                        "remediation": "Audit the unsafe code for memory safety violations and add a `// Safety: ...` comment explaining why it is safe.",
                                        "effort": "Medium"
                                    })
                except Exception:
                    pass

    # 3. Cargo Audit & Yanked/Stale crates (using Cargo.lock for exact versions)
    audit_output = run_cargo_audit()
    if audit_output and audit_output.startswith("{"):
        try:
            audit_data = json.loads(audit_output)
            for vuln in audit_data.get("vulnerabilities", {}).get("list", []):
                findings.append({
                    "severity": "Critical" if vuln["advisory"]["cvss"] and vuln["advisory"]["cvss"]["score"] > 8.0 else "High",
                    "title": f"Dependency CVE in {vuln['package']['name']}",
                    "description": f"CVE: {vuln['advisory']['id']}\nDetails: {vuln['advisory']['title']}",
                    "remediation": f"Update `{vuln['package']['name']}` to a patched version.",
                    "effort": "Low"
                })
        except Exception:
            pass

    try:
        import toml
        with open("mythrax-core/Cargo.lock", "r") as f:
            cargo_lock = toml.load(f)
            packages = cargo_lock.get("package", [])
            for pkg in packages:
                crate = pkg.get("name")
                version = pkg.get("version")
                if not crate or not version:
                    continue
                updated_at, is_yanked = fetch_crates_info(crate, version)
                if is_yanked:
                    findings.append({
                        "severity": "High",
                        "title": f"Yanked Dependency: {crate}",
                        "description": f"Version {version} of {crate} is yanked on crates.io.",
                        "remediation": "Update to a non-yanked version.",
                        "effort": "Low"
                    })
                if updated_at:
                    # Note: Using timezone-aware UTC datetime as per deprecation warning
                    dt = datetime.datetime.strptime(updated_at, "%Y-%m-%dT%H:%M:%S.%fZ").replace(tzinfo=datetime.timezone.utc)
                    if (datetime.datetime.now(datetime.timezone.utc) - dt).days > 365 * 2:
                        findings.append({
                            "severity": "Medium",
                            "title": f"Stale Dependency: {crate}",
                            "description": f"Crate {crate} has no updates since {updated_at}.",
                            "remediation": "Consider replacing or taking over maintenance of the crate.",
                            "effort": "High"
                        })
    except ImportError:
        pass

    # 4. Injection Paths (NoSQL)
    format_query_regex = re.compile(r'db\.query\(\s*(?:&?)?format!\(')
    for root, _, files in os.walk("mythrax-core/src"):
        for f in files:
            if f.endswith(".rs"):
                filepath = os.path.join(root, f)
                try:
                    with open(filepath, "r", encoding="utf-8") as file:
                        for i, line in enumerate(file):
                            if format_query_regex.search(line):
                                findings.append({
                                    "severity": "Critical",
                                    "title": f"NoSQL Injection Path in {f}",
                                    "description": f"Untrusted input might be interpolated into DB query at `{filepath}:{i+1}`:\n```rust\n{line.strip()}\n```",
                                    "remediation": "Use parameterized queries with `$` placeholders and `.bind()`.",
                                    "effort": "Medium"
                                })
                except Exception:
                    pass

    # 5. Secrets in git history (fixing ERE syntax for spacing)
    try:
        git_cmd = "git log -i -E -G '(secret|token|api_key|password|credential)[[:space:]]*[:=][[:space:]]*[\"\'][a-zA-Z0-9_\\-]+[\"\']' -p --all"
        res = subprocess.run(git_cmd, shell=True, capture_output=True, text=True)
        if res.stdout:
            findings.append({
                "severity": "Critical",
                "title": "Secret in Git History",
                "description": "Found potential secrets committed in git history.",
                "remediation": "Rewrite git history to remove the secret and rotate the compromised credential.",
                "effort": "High"
            })
    except Exception:
        pass

    # Generate Report
    report_md = "# Mythrax CISO Security Audit Report\n\n"
    for finding in sorted(findings, key=lambda x: {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}[x["severity"]]):
        report_md += f"## [{finding['severity']}] {finding['title']}\n"
        report_md += f"**Description:** {finding['description']}\n\n"
        report_md += f"**Remediation:** {finding['remediation']} (Effort: {finding['effort']})\n\n"

    with open("ciso_audit_report.md", "w") as f:
        f.write(report_md)

    # File GH Issues
    gh_token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if gh_token and repo:
        import urllib.parse
        for finding in findings:
            if finding["severity"] in ["Critical", "High"]:
                issue_title = finding["title"]
                issue_body = f"{finding['description']}\n\n**Remediation:** {finding['remediation']} (Effort: {finding['effort']})"

                search_url = f"https://api.github.com/search/issues?q=repo:{repo}+is:issue+in:title+%22{urllib.parse.quote(issue_title)}%22"
                req = urllib.request.Request(search_url, headers={"Authorization": f"token {gh_token}", "User-Agent": "mythrax-ciso-audit"})
                try:
                    with urllib.request.urlopen(req) as resp:
                        search_data = json.loads(resp.read().decode())
                        if search_data.get("total_count", 0) > 0:
                            continue # Issue exists
                except Exception:
                    pass

                create_url = f"https://api.github.com/repos/{repo}/issues"
                req = urllib.request.Request(
                    create_url,
                    headers={
                        "Authorization": f"token {gh_token}",
                        "Accept": "application/vnd.github.v3+json",
                        "User-Agent": "mythrax-ciso-audit",
                        "Content-Type": "application/json"
                    },
                    data=json.dumps({"title": issue_title, "body": issue_body, "labels": ["security", "audit", finding["severity"].lower()]}).encode("utf-8"),
                    method="POST"
                )
                try:
                    with urllib.request.urlopen(req) as resp:
                        pass
                except Exception as e:
                    print(f"Failed to create issue: {e}")

if __name__ == "__main__":
    main()
