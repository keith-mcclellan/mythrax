#!/usr/bin/env python3
import os
import re
import subprocess
import json
import urllib.request
import shlex
from collections import defaultdict
import toml
from datetime import datetime, timezone, timedelta

print("Starting security audit...")

report = defaultdict(list)
issues = []
issue_counter = 1

def add_issue(title, severity, file_path, line_num, desc, remediation, effort, snippet=""):
    global issue_counter
    report[severity].append({
        "title": title,
        "file": file_path,
        "line": line_num,
        "desc": desc,
        "remediation": remediation,
        "effort": effort,
        "snippet": snippet
    })

    if severity in ["Critical", "High"]:
        issue_content = f"""## Security Finding: {title}
**Severity**: {severity}
**File**: `{file_path}:{line_num}`

### Description
{desc}

### Snippet / Reproducible Scenario
```rust
{snippet}
```

### Remediation
{remediation}

**Effort**: {effort}
"""
        issues.append({
            "title": f"Security Finding: {title}",
            "body": issue_content,
            "labels": ["bug", "security", "agent-found"]
        })
        issue_counter += 1

def scan_secrets():
    print("Scanning for hardcoded secrets in files...")
    secret_regex = re.compile(r'(?i)(password|token|secret|api_key|apikey|credential|bearer)[-_]*\s*[:=]\s*[\'"][a-zA-Z0-9_\-]+[\'"]')
    for root, _, files in os.walk("mythrax-core"):
        for file in files:
            if file.endswith((".rs", ".toml", ".yaml", ".json")):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r') as f:
                        for line_num, line in enumerate(f, 1):
                            if secret_regex.search(line):
                                add_issue(
                                    "Hardcoded Secret", "Critical", filepath, line_num,
                                    "Found potential hardcoded secret/credential.",
                                    "Move secret to environment variables or a secure secret manager.",
                                    "Low", line.strip()
                                )
                except Exception:
                    pass

def scan_git_history():
    print("Scanning for secrets in git history...")
    cmd = ["git", "log", "-p", "-E", "-i", "-G", r'(password|token|secret|key|apikey|credential|bearer)[[:space:]]*=[[:space:]]*[^ ]+']
    try:
        output = subprocess.check_output(cmd, text=True, errors='ignore')
        if output.strip():
            commits = set(re.findall(r'^commit ([a-f0-9]+)', output, re.MULTILINE))
            if commits:
                add_issue(
                    "Secrets in Git History", "Critical", "git history", 0,
                    f"Found potential secrets in git history (Commits: {', '.join(list(commits)[:3])}...).",
                    "Rotate exposed secrets immediately. Rewrite git history (e.g., using BFG Repo-Cleaner or git filter-repo) to remove the secrets.",
                    "High", "git log -p -E -i -G '(password|token|secret|key|apikey|credential|bearer)[[:space:]]*=[[:space:]]*[^ ]+'"
                )
    except Exception:
        pass

def scan_unsafe_rust():
    print("Scanning for unsafe Rust blocks...")
    unsafe_regex = re.compile(r'\bunsafe\s*\{')
    for root, _, files in os.walk("mythrax-core"):
        for file in files:
            if file.endswith(".rs"):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r') as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            if unsafe_regex.search(line):
                                has_comment = False
                                for j in range(max(0, i-3), i):
                                    if "//" in lines[j] or "/*" in lines[j]:
                                        has_comment = True
                                        break
                                snippet = "".join(lines[i:min(len(lines), i+5)]).strip()

                                severity = "Medium" if has_comment else "High"
                                desc = "Unsafe block found. " + ("" if has_comment else "Lacks documentation justifying its use.")
                                remediation = "Audit the unsafe block for memory safety risks (e.g., raw pointer deref, unchecked bounds). Add a comment explaining invariants."
                                effort = "Medium"

                                add_issue(
                                    "Unsafe Rust Block", severity, filepath, i+1,
                                    desc, remediation, effort, snippet
                                )
                except Exception:
                    pass

def scan_dependencies():
    print("Scanning dependencies for CVEs and staleness...")

    # Run cargo audit for CVEs via RustSec
    try:
        subprocess.check_call(["cargo", "audit", "--version"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except Exception:
        print("cargo-audit not found, installing...")
        try:
            subprocess.check_call(["cargo", "install", "cargo-audit", "--locked"])
        except Exception as e:
            print(f"cargo install cargo-audit failed: {e}")

    try:
        # Do NOT generate lockfile to avoid modifying git state incorrectly during execution
        audit_output = subprocess.check_output(["cargo", "audit", "--json"], cwd="mythrax-core", text=True, stderr=subprocess.STDOUT)
        audit_data = json.loads(audit_output)
        for vuln in audit_data.get("vulnerabilities", {}).get("list", []):
            pkg = vuln["package"]["name"]
            advisory = vuln["advisory"]
            add_issue(
                f"Dependency CVE: {pkg}", "High", "mythrax-core/Cargo.lock", 0,
                f"CVE found in {pkg}: {advisory['title']} ({advisory['id']})",
                f"Update {pkg} to a patched version.", "Low", f"{advisory['id']}"
            )
    except subprocess.CalledProcessError as e:
        # cargo audit exits with non-zero status if vulnerabilities are found
        try:
            audit_data = json.loads(e.output)
            for vuln in audit_data.get("vulnerabilities", {}).get("list", []):
                pkg = vuln["package"]["name"]
                advisory = vuln["advisory"]
                add_issue(
                    f"Dependency CVE: {pkg}", "High", "mythrax-core/Cargo.lock", 0,
                    f"CVE found in {pkg}: {advisory['title']} ({advisory['id']})",
                    f"Update {pkg} to a patched version.", "Low", f"{advisory['id']}"
                )
        except Exception as e2:
             print(f"cargo audit execution failed and output could not be parsed: {e2}")
    except Exception as e:
        print(f"cargo audit execution failed: {e}")

    # Check staleness/yanked via crates.io API
    try:
        with open("mythrax-core/Cargo.toml") as f:
            cargo_data = toml.load(f)
        deps = cargo_data.get("dependencies", {})
        for dep in deps:
            try:
                url = f"https://crates.io/api/v1/crates/{dep}"
                req = urllib.request.Request(url, headers={'User-Agent': 'Mythrax-Audit'})
                with urllib.request.urlopen(req, timeout=5) as response:
                    data = json.loads(response.read().decode())
                    crate_info = data.get("crate", {})
                    if crate_info:
                        versions = data.get("versions", [])
                        is_yanked = any(v.get("yanked") for v in versions[:3])

                        # Stale check based on last updated timestamp (if older than 2 years)
                        is_stale = False
                        if versions:
                            # Check the most recent version date
                            last_updated_str = versions[0].get("updated_at")
                            if last_updated_str:
                                try:
                                     # format: 2024-03-21T02:07:34.619183+00:00
                                     last_updated = datetime.fromisoformat(last_updated_str.replace("Z", "+00:00"))
                                     if (datetime.now(timezone.utc) - last_updated).days > 365 * 2:
                                         is_stale = True
                                except Exception:
                                     pass
                        else:
                            is_stale = True

                        if is_yanked:
                            add_issue(
                                f"Yanked Dependency: {dep}", "Medium", "mythrax-core/Cargo.toml", 0,
                                f"Dependency {dep} has recent yanked versions, possibly indicating issues.",
                                f"Check crates.io for {dep} status and consider migrating or updating.", "Medium", ""
                            )
                        elif is_stale:
                            add_issue(
                                f"Stale Dependency: {dep}", "Medium", "mythrax-core/Cargo.toml", 0,
                                f"Dependency {dep} hasn't been updated in over 2 years.",
                                f"Check crates.io for {dep} status and consider migrating to a maintained alternative.", "Medium", ""
                            )
            except Exception:
                pass
    except Exception:
         pass

def scan_untrusted_input():
    print("Scanning for untrusted external input (basic regex)...")
    sql_regex = re.compile(r'format!\s*\(\s*["\'][^"\']*(SELECT|INSERT|UPDATE|DELETE).*?["\'].*?\)')
    for root, _, files in os.walk("mythrax-core/src"):
        for file in files:
            if file.endswith(".rs"):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r') as f:
                        for line_num, line in enumerate(f, 1):
                            if sql_regex.search(line):
                                add_issue(
                                    "Potential SQL Injection", "High", filepath, line_num,
                                    "Found potential string formatting in a query. Untrusted input could lead to injection.",
                                    "Use parameterized queries instead of string formatting.",
                                    "Medium", line.strip()
                                )
                except Exception:
                    pass

scan_secrets()
scan_git_history()
scan_unsafe_rust()
scan_dependencies()
scan_untrusted_input()

print("\nGenerating report...")
with open("security_report.md", "w") as f:
    f.write("# Mythrax Security Advisory Report\n\n")
    for severity in ["Critical", "High", "Medium", "Low"]:
        f.write(f"## {severity} Findings\n")
        if not report[severity]:
            f.write("*None*\n\n")
        for i, item in enumerate(report[severity], 1):
            f.write(f"### {i}. {item['title']} ({item['file']}:{item['line']})\n")
            f.write(f"**Description**: {item['desc']}\n\n")
            f.write(f"**Snippet**:\n```rust\n{item['snippet']}\n```\n\n")
            f.write(f"**Remediation**: {item['remediation']}\n\n")
            f.write(f"**Estimated Effort**: {item['effort']}\n\n")

print("Writing mock issues...")
os.makedirs("issues", exist_ok=True)
for i, issue in enumerate(issues):
    issue_file = f"issues/issue_{i}.md"
    with open(issue_file, "w") as f:
        f.write(f"---\nTitle: {issue['title']}\nLabels: {', '.join(issue['labels'])}\n---\n\n")
        f.write(issue['body'])
    if os.environ.get("GITHUB_ACTIONS") == "true":
        print(f"Creating issue via gh cli: {issue['title']}")
        try:
             subprocess.run(["gh", "issue", "create", "--title", issue["title"], "--body-file", issue_file, "--label", ",".join(issue["labels"])])
        except Exception as e:
             print(f"Failed to create gh issue: {e}")

print("Done.")
