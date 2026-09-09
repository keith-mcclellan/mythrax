import os
import sys
import subprocess
import json
import urllib.request
import re
from datetime import datetime

GITHUB_API_URL = os.environ.get("GITHUB_API_URL", "https://api.github.com")
GITHUB_REPOSITORY = os.environ.get("GITHUB_REPOSITORY")
GITHUB_TOKEN = os.environ.get("GITHUB_TOKEN")

findings = []

def add_finding(category, severity, title, file_path, line, description, remediation, effort, snippet=None):
    findings.append({
        "category": category,
        "severity": severity,
        "title": title,
        "file_path": file_path,
        "line": line,
        "description": description,
        "remediation": remediation,
        "effort": effort,
        "snippet": snippet
    })

def scan_hardcoded_secrets():
    patterns = [
        r'(secret|token|api_key|password)[[:space:]]*[=:][[:space:]]*["\'][a-zA-Z0-9_\-]{16,}["\']',
        r'bearer[[:space:]]+[a-zA-Z0-9_\-\.]{20,}',
        r'AKIA[0-9A-Z]{16}'
    ]

    cmd = ["grep", "-rnEi", "|".join(patterns), "mythrax-core"]
    result = subprocess.run(cmd, capture_output=True, text=True)

    if result.returncode == 0:
        for line in result.stdout.splitlines():
            parts = line.split(":", 2)
            if len(parts) >= 3:
                filepath, lineno, content = parts[0], parts[1], parts[2]
                add_finding(
                    category="Hardcoded Secret",
                    severity="Critical",
                    title=f"Hardcoded Secret in {filepath}",
                    file_path=filepath,
                    line=lineno,
                    description=f"Potential hardcoded secret found.",
                    remediation="Move secret to environment variables or a secrets manager.",
                    effort="Low",
                    snippet=content.strip()[:100]
                )

def scan_git_history_secrets():
    # Use [[:space:]] instead of \s for git log -G ERE compatibility
    pattern = r'(secret|token|api_key|password)[[:space:]]*[=:][[:space:]]*["\'][a-zA-Z0-9_\-]{16,}["\']'
    cmd = ["git", "log", "-G", pattern, "-i", "--all", "--name-only", "--pretty=format:%h %s"]

    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode == 0 and result.stdout.strip():
        files = set()
        for line in result.stdout.splitlines():
            if not line.startswith(('*', ' ', '\t')) and len(line.split(' ')) == 1:
                if line: files.add(line)

        for file in files:
            add_finding(
                category="Hardcoded Secret (Git History)",
                severity="Critical",
                title=f"Secret found in git history for {file}",
                file_path=file,
                line="N/A",
                description="A commit in history introduced a hardcoded secret.",
                remediation="Revoke the secret immediately. Consider rewriting git history using BFG or git-filter-repo, though revoking is more critical.",
                effort="Medium"
            )

def scan_unsafe_rust():
    cmd = ["grep", "-rnE", r"unsafe[[:space:]]*{", "mythrax-core"]
    result = subprocess.run(cmd, capture_output=True, text=True)

    if result.returncode == 0:
        for line in result.stdout.splitlines():
            parts = line.split(":", 2)
            if len(parts) >= 3:
                filepath, lineno, content = parts[0], parts[1], parts[2]

                with open(filepath, 'r') as f:
                    lines = f.readlines()

                lineno_int = int(lineno)
                justified = False
                if lineno_int > 1:
                    prev_line = lines[lineno_int - 2].strip()
                    if prev_line.startswith("//") and ("SAFETY:" in prev_line or "safety:" in prev_line.lower()):
                        justified = True

                if not justified:
                    add_finding(
                        category="Unsafe Rust",
                        severity="High",
                        title=f"Undocumented unsafe block in {filepath}",
                        file_path=filepath,
                        line=lineno,
                        description="Unsafe block lacks a SAFETY comment. This introduces unverified memory safety risks.",
                        remediation="Add a `// SAFETY: ...` comment explaining why this block is sound.",
                        effort="Low",
                        snippet=content.strip()
                    )
                else:
                    add_finding(
                        category="Unsafe Rust (Justified)",
                        severity="Low",
                        title=f"Documented unsafe block in {filepath}",
                        file_path=filepath,
                        line=lineno,
                        description="Unsafe block is present but has a SAFETY comment.",
                        remediation="Review the justification and ensure it aligns with the memory safety requirements.",
                        effort="Low",
                        snippet=content.strip()
                    )

def check_crates_io(crate_name, version):
    # Only check if we have an exact version number (e.g. 1.0.2), ignore ranges
    if not re.match(r'^\d+\.\d+\.\d+$', version):
        return False, None
    url = f"https://crates.io/api/v1/crates/{crate_name}/{version}"
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "Mythrax CISO Audit"})
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            crate_data = data.get("version", {})
            return crate_data.get("yanked", False), crate_data.get("updated_at")
    except Exception as e:
        print(f"Error checking crates.io for {crate_name} {version}: {e}")
        return False, None

def audit_dependencies():
    os.chdir("mythrax-core")
    try:
        result = subprocess.run(["cargo", "audit", "--json"], capture_output=True, text=True)
        if result.stdout:
            try:
                data = json.loads(result.stdout)

                for vuln in data.get("vulnerabilities", {}).get("list", []):
                    adv = vuln.get("advisory", {})
                    pkg = vuln.get("package", {})
                    add_finding(
                        category="Dependency CVE",
                        severity="Critical" if adv.get("cvss") and adv.get("cvss").startswith("CRITICAL") else "High",
                        title=f"Vulnerable dependency {pkg.get('name')} {pkg.get('version')}: {adv.get('id')}",
                        file_path="mythrax-core/Cargo.lock",
                        line="N/A",
                        description=adv.get("title", "No description"),
                        remediation=f"Update {pkg.get('name')} to a patched version.",
                        effort="Low"
                    )

                # Fix for warning dictionary iteration
                warnings_dict = data.get("warnings", {})
                for warning_type, warning_list in warnings_dict.items():
                    for warning in warning_list:
                        if warning_type == "yanked":
                            pkg = warning.get("package", {})
                            add_finding(
                                category="Dependency Warning",
                                severity="High",
                                title=f"Yanked dependency {pkg.get('name')} {pkg.get('version')}",
                                file_path="mythrax-core/Cargo.lock",
                                line="N/A",
                                description="The dependency version has been yanked by the author.",
                                remediation=f"Update or replace {pkg.get('name')}.",
                                effort="Low"
                            )
                        elif warning_type == "unmaintained":
                            pkg = warning.get("package", {})
                            add_finding(
                                category="Dependency Warning",
                                severity="Medium",
                                title=f"Unmaintained dependency {pkg.get('name')} {pkg.get('version')}",
                                file_path="mythrax-core/Cargo.lock",
                                line="N/A",
                                description="The dependency is flagged as unmaintained.",
                                remediation=f"Find a suitable replacement for {pkg.get('name')}.",
                                effort="Medium"
                            )

            except json.JSONDecodeError:
                print("Failed to parse cargo audit JSON output")

        # Manually check Cargo.toml for unmaintained/yanked crates using exact resolved versions
        cmd = ["cargo", "metadata", "--format-version=1", "--no-deps"]
        meta_result = subprocess.run(cmd, capture_output=True, text=True)
        if meta_result.returncode == 0:
            try:
                meta_data = json.loads(meta_result.stdout)
                packages = meta_data.get("packages", [])
                if packages:
                    dependencies = packages[0].get("dependencies", [])
                    # Need to check Cargo.lock to get exact versions
                    with open("Cargo.lock", "r") as lock_file:
                        lock_content = lock_file.read()

                    for dep in dependencies:
                        name = dep.get("name")

                        # Find exact version in Cargo.lock
                        match = re.search(f'name = "{name}"\nversion = "(.*?)"', lock_content)
                        if match:
                            exact_version = match.group(1)
                            is_yanked, updated_at_str = check_crates_io(name, exact_version)
                            if is_yanked:
                                add_finding(
                                    category="Dependency Warning",
                                    severity="High",
                                    title=f"Yanked dependency {name} {exact_version}",
                                    file_path="mythrax-core/Cargo.toml",
                                    line="N/A",
                                    description="The dependency version has been yanked by the author on crates.io.",
                                    remediation=f"Update or replace {name}.",
                                    effort="Low"
                                )
                            if updated_at_str:
                                updated_at = datetime.fromisoformat(updated_at_str.replace('Z', '+00:00'))
                                if (datetime.now(updated_at.tzinfo) - updated_at).days > 365 * 2:
                                    add_finding(
                                        category="Dependency Warning",
                                        severity="Medium",
                                        title=f"Inactive dependency {name}",
                                        file_path="mythrax-core/Cargo.toml",
                                        line="N/A",
                                        description="The dependency has not been updated in over 2 years.",
                                        remediation=f"Review if {name} is still maintained or find an active fork/alternative.",
                                        effort="Medium"
                                    )
            except json.JSONDecodeError:
                 print("Failed to parse cargo metadata JSON output")

    except FileNotFoundError:
        print("cargo-audit not found. Make sure it's installed.")
    os.chdir("..")

def check_untrusted_input():
    # Use generic SQL/NoSQL injection heuristics or basic system commands
    patterns = [
        r'Command::new\([^)]*\)',
        # Basic check for any query method that might use unparameterized format!
        r'\.query\([^)]*format!\([^)]*\)\)'
    ]
    for pattern in patterns:
        cmd = ["grep", "-rnE", pattern, "mythrax-core"]
        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode == 0:
             for line in result.stdout.splitlines():
                parts = line.split(":", 2)
                if len(parts) >= 3:
                    filepath, lineno, content = parts[0], parts[1], parts[2]
                    add_finding(
                        category="Untrusted Input",
                        severity="High",
                        title=f"Potential injection vector in {filepath}",
                        file_path=filepath,
                        line=lineno,
                        description="Found execution path that may accept untrusted input without proper sanitization.",
                        remediation="Use parameterized queries or strictly validate and sanitize input before execution.",
                        effort="Medium",
                        snippet=content.strip()[:100]
                    )

def file_github_issue(finding):
    if not GITHUB_TOKEN or not GITHUB_REPOSITORY:
        print("GitHub credentials missing, skipping issue creation.")
        os.makedirs("issues", exist_ok=True)
        safe_title = re.sub(r'[^a-zA-Z0-9_\-]', '_', finding['title'])[:50]
        issue_file = f"issues/{safe_title}.md"
        with open(issue_file, "w") as f:
            f.write(f"# {finding['title']}\n")
            f.write(f"**Severity**: {finding['severity']}\n")
            f.write(f"**Category**: {finding['category']}\n")
            f.write(f"**File**: {finding['file_path']}:{finding['line']}\n")
            f.write(f"\n{finding['description']}\n")
            f.write(f"\n**Remediation**: {finding['remediation']}\n")
        return

    url = f"{GITHUB_API_URL}/repos/{GITHUB_REPOSITORY}/issues"

    try:
        req = urllib.request.Request(url, headers={
            "Authorization": f"Bearer {GITHUB_TOKEN}",
            "Accept": "application/vnd.github.v3+json"
        })
        with urllib.request.urlopen(req) as response:
            existing_issues = json.loads(response.read().decode())
            for issue in existing_issues:
                if issue["title"] == finding["title"] and issue["state"] == "open":
                    print(f"Issue already exists: {finding['title']}")
                    return
    except Exception as e:
         print(f"Error checking existing issues: {e}")

    body = f"""**Severity:** {finding['severity']}
**Category:** {finding['category']}
**Location:** `{finding['file_path']}:{finding['line']}`

**Description:**
{finding['description']}

**Snippet:**
```rust
{finding.get('snippet', 'N/A')}
```

**Remediation:** {finding['remediation']}
**Estimated Effort:** {finding['effort']}
"""
    data = json.dumps({
        "title": finding['title'],
        "body": body,
        "labels": ["security", finding['severity'].lower()]
    }).encode("utf-8")

    req = urllib.request.Request(url, data=data, headers={
        "Authorization": f"Bearer {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }, method="POST")

    try:
        with urllib.request.urlopen(req) as response:
             print(f"Filed issue: {finding['title']}")
    except urllib.error.HTTPError as e:
         print(f"Failed to file issue {finding['title']}: {e.code} {e.read().decode()}")
    except Exception as e:
         print(f"Error filing issue: {e}")

def generate_report():
    report_file = "security_advisory_report.md"
    with open(report_file, "w") as f:
        f.write("# Security Advisory Report\n\n")

        for severity in ["Critical", "High", "Medium", "Low"]:
            sev_findings = [f_item for f_item in findings if f_item["severity"] == severity]
            if sev_findings:
                f.write(f"## {severity} Findings\n\n")
                for finding in sev_findings:
                    f.write(f"### {finding['title']}\n")
                    f.write(f"- **Category:** {finding['category']}\n")
                    f.write(f"- **Location:** `{finding['file_path']}:{finding['line']}`\n")
                    f.write(f"- **Description:** {finding['description']}\n")
                    f.write(f"- **Remediation:** {finding['remediation']} (Effort: {finding['effort']})\n\n")

def main():
    print("Starting CISO Audit...")
    scan_hardcoded_secrets()
    scan_git_history_secrets()
    scan_unsafe_rust()
    audit_dependencies()
    check_untrusted_input()

    generate_report()
    print(f"Generated report with {len(findings)} findings.")

    for finding in findings:
        if finding["severity"] in ["Critical", "High"]:
            file_github_issue(finding)

if __name__ == "__main__":
    main()
