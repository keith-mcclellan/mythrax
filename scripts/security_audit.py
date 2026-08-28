#!/usr/bin/env python3
import json
import os
import re
import subprocess
import sys
import urllib.request
import datetime
import time

# Setup
WORKSPACE_ROOT = os.environ.get("GITHUB_WORKSPACE", os.getcwd())
os.chdir(WORKSPACE_ROOT)

findings = []

def add_finding(title, severity, description, remediation, effort):
    findings.append({
        "title": title,
        "severity": severity,
        "description": description,
        "remediation": remediation,
        "effort": effort
    })

print("Starting security audit...")

# 1. Hardcoded secrets, tokens, API keys, or credentials anywhere in source or config files
print("Checking for hardcoded secrets...")
secret_patterns = [
    r'(password|token|secret|api_key|credential)[ \t]*=[ \t]*["\'][^"\']+["\']'
]

for root, _, files in os.walk("mythrax-core"):
    for file in files:
        if file.endswith(".rs") or file.endswith(".toml") or file.endswith(".json") or file.endswith(".yml") or file.endswith(".yaml"):
            file_path = os.path.join(root, file)
            with open(file_path, "r") as f:
                try:
                    content = f.read()
                    lines = content.split('\n')
                    for i, line in enumerate(lines):
                        for pattern in secret_patterns:
                            if re.search(pattern, line, re.IGNORECASE):
                                if "test" in line.lower() or "example" in line.lower() or "mock" in line.lower():
                                    continue
                                add_finding(
                                    f"Potential Hardcoded Secret in {file_path}",
                                    "High",
                                    f"Found potential secret on line {i+1}: `{line.strip()}`",
                                    "Use environment variables or a secure secret manager to inject this value at runtime.",
                                    "Medium"
                                )
                except UnicodeDecodeError:
                    pass

# 2. Unsafe Rust blocks
print("Checking for unsafe Rust blocks...")
for root, _, files in os.walk("mythrax-core/src"):
    for file in files:
        if file.endswith(".rs"):
            file_path = os.path.join(root, file)
            with open(file_path, "r") as f:
                try:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if "unsafe {" in line or "unsafe fn" in line:
                            has_comment = False
                            if i > 0 and (lines[i-1].strip().startswith("//") or lines[i-1].strip().startswith("/*")):
                                has_comment = True

                            if not has_comment:
                                add_finding(
                                    f"Undocumented Unsafe Rust Block in {file_path}",
                                    "Medium",
                                    f"Found `unsafe` block on line {i+1} without a documented justification comment. Memory safety risks include undefined behavior, buffer overflows, and memory leaks.",
                                    "Add a `// SAFETY: <justification>` comment explaining why the block is safe, or refactor to safe Rust.",
                                    "Low"
                                )
                except UnicodeDecodeError:
                    pass

# 3. Cargo.toml and Cargo.lock dependencies with known CVEs, yanked crates, or stale crates
print("Checking dependencies...")
try:
    res = subprocess.run(["cargo", "audit", "--json"], cwd="mythrax-core", capture_output=True, text=True)
    if res.returncode in [0, 1]:
        try:
            audit_data = json.loads(res.stdout)
            for vuln in audit_data.get("vulnerabilities", {}).get("list", []):
                advisory = vuln.get("advisory", {})
                add_finding(
                    f"Dependency Vulnerability: {advisory.get('package', 'unknown')} ({advisory.get('id', 'unknown')})",
                    "Critical" if advisory.get("cvss") and advisory.get("cvss").get("score") and advisory.get("cvss").get("score") >= 9.0 else "High",
                    f"Vulnerability in {advisory.get('package')}: {advisory.get('title')}. Details: {advisory.get('url', 'N/A')}",
                    f"Upgrade {advisory.get('package')} to a patched version.",
                    "Low"
                )
        except json.JSONDecodeError:
            print("Failed to parse cargo audit JSON output.")
except FileNotFoundError:
    print("cargo-audit not found, skipping CVE check.")

def get_crate_info(crate_name, version=None):
    url = f"https://crates.io/api/v1/crates/{crate_name}"
    req = urllib.request.Request(url, headers={'User-Agent': 'Mythrax-Security-Auditor/1.0'})
    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read().decode())
    except Exception as e:
        print(f"Error fetching {crate_name}: {e}")
        return None

try:
    with open("mythrax-core/Cargo.toml", "r") as f:
        cargo_toml = f.read()

    deps = re.findall(r'^([a-zA-Z0-9_-]+)\s*=\s*(?:".*?"|\{.*?\})', cargo_toml, re.MULTILINE)
    deps = [d for d in deps if d not in ["[dependencies]", "[dev-dependencies]", "[build-dependencies]"]]

    # Also parse from Cargo.lock for a more complete list
    try:
        with open("Cargo.lock", "r") as f:
            lock_data = f.read()
        lock_deps = re.findall(r'name = "([^"]+)"', lock_data)
        deps.extend(lock_deps)
        deps = list(set(deps)) # Deduplicate
    except FileNotFoundError:
        pass
    except Exception as e:
        print(f"Error reading Cargo.lock: {e}")

    print(f"Found {len(deps)} direct dependencies to check.")

    for dep in deps:
        info = get_crate_info(dep)
        time.sleep(1) # Respect crates.io rate limit (1 req/sec)
        if info and "versions" in info:
            latest_version = info["versions"][0]
            if latest_version.get("yanked", False):
                add_finding(
                    f"Yanked Dependency: {dep}",
                    "High",
                    f"The latest version of {dep} is yanked from crates.io.",
                    f"Update {dep} to an unyanked version or find an alternative.",
                    "Medium"
                )

            created_at_str = latest_version.get("created_at")
            if created_at_str:
                try:
                    created_at_str = created_at_str.split(".")[0].replace("Z", "")
                    created_at = datetime.datetime.fromisoformat(created_at_str)
                    age_days = (datetime.datetime.now() - created_at).days
                    if age_days > 365 * 2: # 2 years stale
                        add_finding(
                            f"Stale Dependency: {dep}",
                            "Medium",
                            f"The dependency {dep} has not had a release in over 2 years (last release: {created_at_str}).",
                            "Consider replacing the dependency or maintaining a fork.",
                            "High"
                        )
                except ValueError as e:
                    print(f"Failed to parse date {created_at_str} for {dep}: {e}")

except Exception as e:
    print(f"Error parsing Cargo.toml: {e}")

# 4. Untrusted external input
print("Checking for untrusted input handling...")
for root, _, files in os.walk("mythrax-core"): # Check the full codebase
    for file in files:
        if file.endswith(".rs"):
            file_path = os.path.join(root, file)
            with open(file_path, "r") as f:
                try:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if "fs::read_to_string" in line or "File::open" in line or "Command::new" in line:
                            add_finding(
                                f"Potential Untrusted Input Sink in {file_path}",
                                "Medium",
                                f"Found potential file/command execution on line {i+1}: `{line.strip()}`. Ensure input is sanitized.",
                                "Implement boundary enforcement and sanitize input before using it in file operations or commands.",
                                "Medium"
                            )
                except UnicodeDecodeError:
                    pass

# 5. Secrets in Git History
print("Checking git history for secrets...")
try:
    res = subprocess.run(["git", "log", "-p", "-E", "-i", "-G", r'(password|token|secret|api_key)[[:space:]]*=[[:space:]]*[^ ]+'], capture_output=True, text=True)
    commits = re.findall(r'^commit ([0-9a-f]{40})', res.stdout, re.MULTILINE)

    unique_commits = set(commits)
    if unique_commits:
        add_finding(
            "Secrets Found in Git History",
            "Critical",
            f"Found potential secrets in {len(unique_commits)} historical commits.",
            "Rotate the exposed secrets immediately. Rewrite git history (e.g., using BFG Repo-Cleaner) to remove the secrets.",
            "High"
        )
except Exception as e:
    print(f"Error checking git history: {e}")

# Report Generation
print(f"Found {len(findings)} findings.")

report = f"""# Mythrax Security Audit Report
Generated on: {datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")}

## Summary
| Severity | Count |
| -------- | ----- |
| Critical | {sum(1 for f in findings if f["severity"] == "Critical")} |
| High | {sum(1 for f in findings if f["severity"] == "High")} |
| Medium | {sum(1 for f in findings if f["severity"] == "Medium")} |
| Low | {sum(1 for f in findings if f["severity"] == "Low")} |

## Findings

"""

for i, finding in enumerate(findings):
    report += f"### {i+1}. [{finding['severity']}] {finding['title']}\n"
    report += f"**Description:** {finding['description']}\n\n"
    report += f"**Remediation:** {finding['remediation']} (Effort: {finding['effort']})\n\n"

with open("security_report.md", "w") as f:
    f.write(report)

print("Report saved to security_report.md")

# GitHub Issue Filing (for Critical/High)
gh_token = os.environ.get("GH_TOKEN")
if gh_token:
    repo = os.environ.get("GITHUB_REPOSITORY", "keith-mcclellan/mythrax")

    # Get existing issues to deduplicate
    existing_issues = set()
    try:
        req = urllib.request.Request(
            f"https://api.github.com/repos/{repo}/issues?state=open",
            headers={
                "Authorization": f"Bearer {gh_token}",
                "Accept": "application/vnd.github.v3+json"
            }
        )
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode())
            for issue in issues:
                existing_issues.add(issue.get("title"))
    except Exception as e:
        print(f"Error fetching existing issues: {e}")

    for finding in findings:
        if finding["severity"] in ["Critical", "High"]:
            issue_title = f"[Security] {finding['title']}"

            if issue_title in existing_issues:
                print(f"Issue already exists for: {finding['title']}, skipping.")
                continue

            issue_body = f"## Severity: {finding['severity']}\n\n**Description:** {finding['description']}\n\n**Remediation:** {finding['remediation']}\n\n*Automated security audit finding.*"

            req = urllib.request.Request(
                f"https://api.github.com/repos/{repo}/issues",
                method="POST",
                headers={
                    "Authorization": f"Bearer {gh_token}",
                    "Accept": "application/vnd.github.v3+json"
                },
                data=json.dumps({"title": issue_title, "body": issue_body, "labels": ["security", "bug"]}).encode("utf-8")
            )

            try:
                urllib.request.urlopen(req)
                print(f"Filed issue for: {finding['title']}")
            except Exception as e:
                print(f"Failed to file issue for {finding['title']}: {e}")
else:
    print("GH_TOKEN not found, skipping issue creation.")
