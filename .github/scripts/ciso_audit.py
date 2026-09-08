import os
import re
import subprocess
import urllib.request
import urllib.error
import json
import toml
import time

def log_print(msg):
    print(f"[CISO Audit] {msg}")

def file_github_issue(title, body):
    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")

    if not repo or not token:
        log_print(f"Skipping issue creation (missing GITHUB_REPOSITORY or GITHUB_TOKEN). Title: {title}")
        os.makedirs("issues", exist_ok=True)
        safe_title = re.sub(r'[^a-zA-Z0-9_\-]', '_', title)
        filename = f"issues/{safe_title}.md"
        with open(filename, "w", encoding="utf-8") as f:
            f.write(body)
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }

    # Deduplication
    try:
        req = urllib.request.Request(f"{url}?state=open&per_page=100", headers=headers)
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode())
            for issue in issues:
                if issue["title"] == title:
                    log_print(f"Issue already exists: {title}")
                    return
    except Exception as e:
        log_print(f"Error checking existing issues: {e}")

    data = json.dumps({"title": title, "body": body}).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req) as response:
            log_print(f"Created issue: {title}")
    except Exception as e:
        log_print(f"Error creating issue: {e}")

def scan_secrets(findings):
    log_print("Scanning for hardcoded secrets...")
    # 1. Source code and configs
    secret_regex = re.compile(r"(?i)(password|secret|token|api_key|apikey)[\s]*[=:]\s*['\"][a-zA-Z0-9_\-]+['\"]")

    for root, _, files in os.walk("."):
        # Skip .git and build dirs
        if ".git" in root or "target" in root or ".rustup" in root or ".cargo" in root or ".pyenv" in root:
            continue

        for file in files:
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                    for i, line in enumerate(f):
                        if secret_regex.search(line):
                            findings.append({
                                "severity": "Critical",
                                "title": f"Hardcoded Secret in {filepath}:{i+1}",
                                "description": f"Found potential secret on line {i+1}:\n```rust\n{line.strip()}\n```",
                                "remediation": "Move the secret to an environment variable or secure vault.",
                                "effort": "Low"
                            })
            except Exception:
                pass

    # 2. Git history
    # Stream the output to avoid OOM
    log_print("Scanning git history for secrets...")
    try:
        cmd = ["git", "log", "-G", "secret|token|key|password", "-p", "--all"]
        process = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="ignore")

        chunk_size = 10000
        found_in_history = False
        while True:
            line = process.stdout.readline()
            if not line:
                break

            if secret_regex.search(line):
                found_in_history = True
                break

        process.kill()

        if found_in_history:
             findings.append({
                "severity": "Critical",
                "title": "Secrets Found in Git History",
                "description": "Found secrets in `git log -G`. Needs repository cleaning.",
                "remediation": "Rewrite git history to remove secrets (e.g. BFG or git-filter-repo).",
                "effort": "High"
            })
    except Exception as e:
        log_print(f"Error scanning git history: {e}")

def scan_unsafe(findings):
    log_print("Scanning for unsafe blocks...")
    for root, _, files in os.walk("mythrax-core/src"):
        for file in files:
            if not file.endswith(".rs"): continue
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if "unsafe {" in line or "unsafe impl" in line or "unsafe fn" in line:
                            justified = False
                            if i > 0 and ("//" in lines[i-1] or "/*" in lines[i-1]):
                                justified = True

                            if not justified:
                                findings.append({
                                    "severity": "High",
                                    "title": f"Undocumented Unsafe Block in {filepath}:{i+1}",
                                    "description": f"Found `unsafe` block without justification on line {i+1}:\n```rust\n{line.strip()}\n```\nUnsafe Rust bypasses the borrow checker and can lead to memory safety vulnerabilities.",
                                    "remediation": "Add a `// Safety: ...` comment explaining why this block is safe, or refactor to use safe Rust.",
                                    "effort": "Medium"
                                })
            except Exception:
                pass

def check_crates_io(crate_name, crate_version):
    url = f"https://crates.io/api/v1/crates/{crate_name}"
    req = urllib.request.Request(url, headers={"User-Agent": "Mythrax-CISO-Bot"})
    try:
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode("utf-8"))

            # Check yanked status for the specific version
            versions = data.get("versions", [])
            for v in versions:
                if v.get("num") == crate_version:
                    if v.get("yanked"):
                        return {"status": "yanked", "msg": "Crate version is yanked."}

            # Check stale (no updates in a long time)
            updated_at_str = data.get("crate", {}).get("updated_at")
            if updated_at_str:
                # Parsing date simply to see if year is old
                year = int(updated_at_str[:4])
                if year <= 2021:
                    return {"status": "stale", "msg": f"Crate has not been updated since {year}."}

    except Exception as e:
        pass

    return {"status": "ok", "msg": ""}

def scan_dependencies(findings):
    log_print("Scanning dependencies for CVEs and stale/yanked crates...")
    # cargo audit
    os.chdir("mythrax-core")
    try:
        result = subprocess.run(["cargo", "audit", "--json"], capture_output=True, text=True)
        if result.returncode != 0 and result.stdout:
            try:
                audit_data = json.loads(result.stdout)
                for vuln in audit_data.get("vulnerabilities", {}).get("list", []):
                    # Robust check on cvss score logic in case it's string, null, etc.
                    is_critical = False
                    cvss = vuln.get("advisory", {}).get("cvss")
                    if isinstance(cvss, dict):
                        score = cvss.get("score", 0)
                        if score > 7.0:
                            is_critical = True
                    elif cvss: # it could be string like "CVSS:3.1/..."
                        # fallback assumption for strings that we don't bother to parse perfectly here
                        is_critical = True

                    findings.append({
                        "severity": "Critical" if is_critical else "High",
                        "title": f"Vulnerable Dependency: {vuln['package']['name']} ({vuln['advisory']['id']})",
                        "description": f"Dependency `{vuln['package']['name']}` has a known vulnerability: {vuln['advisory']['title']}.\n\n{vuln['advisory']['description']}",
                        "remediation": f"Update `{vuln['package']['name']}` to a patched version.",
                        "effort": "Low"
                    })
            except Exception as e:
                log_print(f"Error parsing cargo audit output: {e}")
    except Exception as e:
        log_print(f"Error running cargo audit: {e}")

    # Check crates.io
    try:
        with open("Cargo.toml", "r", encoding="utf-8") as f:
            cargo_toml = toml.load(f)
            dependencies = cargo_toml.get("dependencies", {})
            for crate, version_info in dependencies.items():
                version = None
                if isinstance(version_info, str):
                    version = version_info
                elif isinstance(version_info, dict) and "version" in version_info:
                    version = version_info["version"]

                if version:
                    clean_version = re.sub(r'[\^\=\~]', '', version)
                    check = check_crates_io(crate, clean_version)
                    if check["status"] == "yanked":
                        findings.append({
                            "severity": "High",
                            "title": f"Yanked Dependency: {crate}",
                            "description": f"Dependency `{crate}` version `{clean_version}` is yanked on crates.io.",
                            "remediation": f"Update `{crate}` to a non-yanked version.",
                            "effort": "Low"
                        })
                    elif check["status"] == "stale":
                        findings.append({
                            "severity": "Medium",
                            "title": f"Stale Dependency: {crate}",
                            "description": f"Dependency `{crate}`: {check['msg']}",
                            "remediation": f"Consider migrating away from `{crate}` or taking over maintenance.",
                            "effort": "High"
                        })

                time.sleep(0.1)
    except Exception as e:
        log_print(f"Error checking crates.io: {e}")

    os.chdir("..")

def scan_nosql_injection(findings):
    log_print("Scanning for NoSQL injection paths...")
    # Look for format! inside query()
    for root, _, files in os.walk("mythrax-core/src"):
        for file in files:
            if not file.endswith(".rs"): continue
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                    content = f.read()
                    if "format!(" in content and ".query(" in content:
                        lines = content.splitlines()
                        for i, line in enumerate(lines):
                            if ".query(" in line and "format!(" in line:
                                findings.append({
                                    "severity": "Critical",
                                    "title": f"Potential NoSQL Injection in {filepath}:{i+1}",
                                    "description": f"Found `format!` used inside or near a database query on line {i+1}:\n```rust\n{line.strip()}\n```\nThis indicates string interpolation is used instead of parameterized queries, leading to NoSQL injection vulnerabilities.",
                                    "remediation": "Use parameterized queries with `$` placeholders and `.bind()`.",
                                    "effort": "Medium"
                                })
            except Exception:
                pass

def run_audit():
    findings = []

    scan_secrets(findings)
    scan_unsafe(findings)
    scan_dependencies(findings)
    scan_nosql_injection(findings)

    log_print(f"Found {len(findings)} issues.")

    report = "# CISO Security Advisory Report\n\n"

    severity_order = {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}
    findings.sort(key=lambda x: severity_order.get(x["severity"], 4))

    for finding in findings:
        report += f"## [{finding['severity']}] {finding['title']}\n"
        report += f"**Description:**\n{finding['description']}\n\n"
        report += f"**Remediation:** {finding['remediation']}\n"
        report += f"**Estimated Effort:** {finding['effort']}\n\n"

        if finding['severity'] in ["Critical", "High"]:
            body = f"**Severity:** {finding['severity']}\n\n"
            body += f"**Description:**\n{finding['description']}\n\n"
            body += f"**Remediation:** {finding['remediation']}\n"
            body += f"**Estimated Effort:** {finding['effort']}\n"
            file_github_issue(finding['title'], body)

    with open("security_advisory_report.md", "w", encoding="utf-8") as f:
        f.write(report)

    log_print("Audit complete. Report generated.")

if __name__ == "__main__":
    run_audit()
