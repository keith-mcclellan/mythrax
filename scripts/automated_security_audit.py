#!/usr/bin/env python3
import os
import re
import subprocess
import json
import urllib.request
import datetime
import toml
import shlex

def run_command(cmd, shell=False):
    result = subprocess.run(cmd, shell=shell, capture_output=True, text=True)
    return result.stdout, result.stderr, result.returncode

def check_hardcoded_secrets():
    print("Checking for hardcoded secrets...")
    pattern = r"(?i)(password|token|api_key|secret|credential).{0,5}=\s*['\"][a-zA-Z0-9_-]{10,}['\"]"
    # Basic grep using egrep-like syntax for the regex, filtering out test lines if possible
    # We will use git grep to look in tracked files
    cmd = ["git", "grep", "-E", "-i", "(password|token|api_key|secret|credential).{0,5}=\\s*['\"][a-zA-Z0-9_-]{10,}['\"]"]
    out, err, code = run_command(cmd)

    findings = []
    if code == 0 and out:
        for line in out.strip().split('\n'):
            findings.append({
                "severity": "Critical",
                "title": "Hardcoded Secret Found in HEAD",
                "description": f"Found potential secret:\n```\n{line}\n```",
                "remediation": "Remove the secret, rotate it, and load it from environment variables or a secure vault.",
                "effort": "Medium"
            })
    return findings

def check_unsafe_blocks():
    print("Checking for unsafe Rust blocks...")
    cmd = ["git", "grep", "-n", "unsafe {"]
    out, err, code = run_command(cmd)

    findings = []
    if code == 0 and out:
        lines = out.strip().split('\n')
        for line in lines:
            parts = line.split(":", 2)
            if len(parts) >= 2:
                file_path = parts[0]
                line_num = parts[1]

                # Check surrounding lines for documentation
                # We'll fetch 3 lines before the unsafe block
                blame_cmd = ["sed", "-n", f"{max(1, int(line_num)-3)},{int(line_num)-1}p", file_path]
                blame_out, _, _ = run_command(blame_cmd)

                if "SAFETY" not in blame_out.upper():
                    findings.append({
                        "severity": "Medium",
                        "title": f"Undocumented unsafe block in {file_path}:{line_num}",
                        "description": f"File: `{file_path}`, Line: {line_num}\nAn `unsafe` block was found without a surrounding SAFETY justification comment. Memory safety risks cannot be verified.",
                        "remediation": "Audit the unsafe block for memory safety (e.g. data races, invalid pointers, undefined behavior) and add a `// SAFETY: ...` comment explaining why it is safe.",
                        "effort": "Low"
                    })
    return findings

def check_cargo_audit():
    print("Running cargo audit...")
    cmd = ["cargo", "audit", "--json"]
    # Run in mythrax-core
    result = subprocess.run(cmd, cwd="mythrax-core", capture_output=True, text=True)
    out = result.stdout

    findings = []
    if out:
        try:
            data = json.loads(out)
            vulnerabilities = data.get("vulnerabilities", {}).get("list", [])
            for vuln in vulnerabilities:
                adv = vuln.get("advisory", {})
                severity = "Medium"
                # Some heuristics for severity if CVSS not available
                title = adv.get("title", "Unknown Vulnerability")
                package = vuln.get("package", {}).get("name", "unknown")
                version = vuln.get("package", {}).get("version", "unknown")

                if "critical" in adv.get("title", "").lower() or "high" in adv.get("title", "").lower() or "remote" in adv.get("title", "").lower():
                     severity = "High"

                findings.append({
                    "severity": severity,
                    "title": f"Dependency Vulnerability: {package} {version} ({adv.get('id')})",
                    "description": f"Title: {title}\nURL: {adv.get('url')}\nDescription: {adv.get('description', '')[:500]}...",
                    "remediation": f"Upgrade crate `{package}`.",
                    "effort": "Low"
                })
        except json.JSONDecodeError:
            pass

    return findings

def check_stale_crates():
    print("Checking crates.io for yanked/stale crates...")
    findings = []
    try:
        with open("mythrax-core/Cargo.toml", "r") as f:
            cargo_toml = toml.load(f)
    except Exception as e:
        print(f"Error parsing Cargo.toml: {e}")
        return findings

    deps = list(cargo_toml.get("dependencies", {}).keys()) + list(cargo_toml.get("dev-dependencies", {}).keys())
    deps = list(set(deps))

    now = datetime.datetime.now(datetime.timezone.utc)

    for dep in deps:
        url = f"https://crates.io/api/v1/crates/{dep}"
        req = urllib.request.Request(url, headers={'User-Agent': 'MythraxSecurityAudit/1.0'})
        try:
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read().decode())
                crate = data.get("crate", {})
                versions = data.get("versions", [])

                if not versions:
                    continue

                latest_version = versions[0]
                if latest_version.get("yanked"):
                    findings.append({
                        "severity": "High",
                        "title": f"Yanked crate dependency: {dep}",
                        "description": f"The crate `{dep}` version {latest_version.get('num')} has been yanked from crates.io.",
                        "remediation": "Remove or update this dependency to a non-yanked version.",
                        "effort": "Low"
                    })

                updated_at = crate.get("updated_at")
                if updated_at:
                    dt = datetime.datetime.fromisoformat(updated_at.replace("Z", "+00:00"))
                    diff = now - dt
                    if diff.days > 365 * 2:
                        findings.append({
                            "severity": "Medium",
                            "title": f"Stale crate dependency: {dep}",
                            "description": f"The crate `{dep}` has not been updated in {diff.days} days.",
                            "remediation": "Audit the crate to ensure it is still safe and maintained, or migrate to an active alternative.",
                            "effort": "Medium"
                        })
        except urllib.error.HTTPError as e:
            pass

    return findings

def check_untrusted_input():
    print("Checking untrusted input execution paths...")
    # This is a naive static analysis finding missing sanitizations
    cmd = ["git", "grep", "-n", "-E", "-i", "args\\.get\\(\"[a-zA-Z0-9_]+\"\\)"]
    out, err, code = run_command(cmd)

    findings = []
    if code == 0 and out:
        for line in out.strip().split('\n'):
            parts = line.split(":", 2)
            if len(parts) >= 3:
                file_path = parts[0]
                line_num = parts[1]
                content = parts[2]

                if "mythrax-core/src/mcp_routes" in file_path:
                    findings.append({
                        "severity": "Medium",
                        "title": f"Potential Untrusted Input in {file_path}:{line_num}",
                        "description": f"Line: `{content.strip()}`\nRoute arguments are accessed directly. Ensure this input is sanitized and boundary-enforced before being used in execution or storage.",
                        "remediation": "Implement strict validation and sanitization for external inputs.",
                        "effort": "Medium"
                    })
    return findings

def check_git_history_secrets():
    print("Checking git history for secrets...")
    # Fetch unshallow to ensure history exists
    subprocess.run(["git", "fetch", "--unshallow"], capture_output=True)
    subprocess.run(["git", "fetch", "--all"], capture_output=True)

    cmd = ["git", "log", "-p", "-E", "-i", "-G", "(password|token|api_key|secret|credential)[[:space:]]*=[[:space:]]*['\"][a-zA-Z0-9_-]{10,}['\"]", "mythrax-core"]
    out, err, code = run_command(cmd)

    findings = []
    if code == 0 and out:
         findings.append({
             "severity": "Critical",
             "title": "Secret found in Git History",
             "description": f"Potential secret found in git history.",
             "remediation": "Rotate the secret immediately and rewrite git history using tools like BFG or git-filter-repo to remove it.",
             "effort": "High"
         })
    return findings

def main():
    findings = []
    findings.extend(check_hardcoded_secrets())
    findings.extend(check_unsafe_blocks())
    findings.extend(check_cargo_audit())
    findings.extend(check_stale_crates())
    findings.extend(check_untrusted_input())
    findings.extend(check_git_history_secrets())

    report_md = "# Security Advisory Report\n\n"

    for idx, f in enumerate(findings):
        report_md += f"## {idx+1}. [{f['severity']}] {f['title']}\n"
        report_md += f"**Description:**\n{f['description']}\n\n"
        report_md += f"**Remediation:** {f['remediation']}\n"
        report_md += f"**Estimated Effort:** {f['effort']}\n\n"

        if f['severity'] in ["Critical", "High"]:
            # File a GitHub Issue
            body = f"**Severity**: {f['severity']}\n\n**Description**:\n{f['description']}\n\n**Remediation**:\n{f['remediation']}\n\n**Effort**: {f['effort']}"

            # Use subprocess with list of arguments instead of shell=True for complex strings
            print(f"Filing issue for {f['title']}")
            try:
                subprocess.run(["gh", "issue", "create", "--title", f"Security Alert: {f['title']}", "--body", body, "--label", "security,automated"], check=True)
            except FileNotFoundError:
                print(f"Warning: 'gh' CLI not found. Skipping issue creation for: {f['title']}")
            except subprocess.CalledProcessError as e:
                print(f"Failed to create issue for {f['title']}: {e}")

    # Output the report to stdout so it can be captured by the action or run interactively
    print(report_md)

if __name__ == "__main__":
    main()
