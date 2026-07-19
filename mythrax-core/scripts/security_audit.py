#!/usr/bin/env python3
import os
import subprocess
import json
import re
from datetime import datetime

print("Starting Security Audit...")

CRITICAL_FINDINGS = []
HIGH_FINDINGS = []
MEDIUM_FINDINGS = []
LOW_FINDINGS = []

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout, result.stderr, result.returncode

def redact_secret(line):
    # Very basic redaction: mask anything after = or :
    return re.sub(r'([:=]\s*[\'"]).*?([\'"])', r'\1***REDACTED***\2', line)

def issue_exists(title):
    out, _, _ = run_cmd(f'gh issue list --search "{title} in:title" --state open --json number')
    try:
        issues = json.loads(out)
        return len(issues) > 0
    except Exception:
        return False

# 1. Hardcoded Secrets (Source and Config)
print("Scanning for hardcoded secrets in current HEAD...")
SECRET_PATTERN = r"(?i)(secret|password|api[_-]?key|bearer|token)[_-]?(key|id|secret)?\s*[:=]\s*['\"][a-zA-Z0-9_\-]+['\"]"
GREP_SECRET_PATTERN = r"(secret|password|api[_-]?key|bearer|token)[_-]?(key|id|secret)?\s*[:=]\s*['\"][a-zA-Z0-9_\-]+['\"]"

secrets_found = []
for root, _, files in os.walk("."):
    if ".git" in root or "target" in root:
        continue
    for file in files:
        if file.endswith(".rs") or file.endswith(".toml") or file.endswith(".json") or file.endswith(".yaml") or file.endswith(".yml"):
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if re.search(SECRET_PATTERN, line):
                            secrets_found.append(f"{filepath}:{i+1}: {redact_secret(line.strip())}")
            except Exception:
                pass

if secrets_found:
    CRITICAL_FINDINGS.append({
        "title": "Hardcoded Secrets Found in Source",
        "description": "The following files contain potential hardcoded secrets:\n```\n" + "\n".join(secrets_found[:20]) + ("\n..." if len(secrets_found) > 20 else "") + "\n```",
        "risk": "Credential compromise.",
        "remediation": "Remove secrets from source and use secure environment variables or vault.",
        "effort": "Low (1-2 hours)"
    })

# 2. Secrets in Git History
print("Scanning git history for secrets...")
# We use a simplified pattern for grep to avoid quoting issues
git_log_cmd = f'git log -p | grep -E -i "{GREP_SECRET_PATTERN}"'
out, _, _ = run_cmd(git_log_cmd)
if out.strip():
    MEDIUM_FINDINGS.append({
        "title": "Secrets Potentially Committed in Git History",
        "description": "Potential secrets were found in the commit history.",
        "risk": "Historical secrets can be extracted from the repository.",
        "remediation": "Scrub git history using tools like BFG Repo-Cleaner or git-filter-repo and rotate exposed credentials.",
        "effort": "Medium (2-4 hours)"
    })

# 3. Unsafe Rust Blocks
print("Scanning for unsafe blocks...")
unsafe_found = []
for root, _, files in os.walk("."):
    if ".git" in root or "target" in root:
        continue
    for file in files:
        if file.endswith(".rs"):
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if re.search(r'\bunsafe\s*(?:\{|fn|impl|trait)', line):
                            # Check preceding 3 lines for Safety comment
                            has_comment = False
                            start = max(0, i - 3)
                            for j in range(start, i):
                                if re.search(r'//\s*(?i)safety:', lines[j]):
                                    has_comment = True
                                    break
                            if not has_comment:
                                unsafe_found.append(f"{filepath}:{i+1}: {line.strip()}")
            except Exception:
                pass

if unsafe_found:
    HIGH_FINDINGS.append({
        "title": "Unjustified Unsafe Rust Blocks Detected",
        "description": "The following `unsafe` blocks lack a `// SAFETY:` justification comment:\n```\n" + "\n".join(unsafe_found[:20]) + ("\n..." if len(unsafe_found) > 20 else "") + "\n```",
        "risk": "Memory corruption, data races, undefined behavior.",
        "remediation": "Remove `unsafe` where possible or document with a rigorous `// SAFETY:` comment explaining why invariants hold.",
        "effort": "High (1-2 days)"
    })

# 4. Cargo.lock Vulnerabilities
print("Running cargo audit...")
out, err, rc = run_cmd("cargo audit --json")
try:
    audit_data = json.loads(out)
    vulns = audit_data.get("vulnerabilities", {}).get("list", [])
    warnings = audit_data.get("warnings", {})

    if vulns:
        vuln_details = []
        for v in vulns:
            advisory = v.get("advisory", {})
            title = advisory.get("title", "Unknown")
            pkg = v.get("package", {}).get("name", "Unknown")
            vuln_details.append(f"- {pkg}: {title}")
        CRITICAL_FINDINGS.append({
            "title": "Vulnerable Dependencies (cargo audit)",
            "description": "CVEs were found in dependencies:\n" + "\n".join(vuln_details),
            "risk": "Supply chain attacks, DoS, remote code execution.",
            "remediation": "Run `cargo update` or bump versions in Cargo.toml to patch vulnerable crates.",
            "effort": "Medium (2-4 hours)"
        })

    unmaintained = [w for w in warnings.values() if isinstance(w, list)]
    unmaintained_flat = [item for sublist in unmaintained for item in sublist if item.get("kind") == "unmaintained"]
    yanked_flat = [item for sublist in unmaintained for item in sublist if item.get("kind") == "yanked"]

    if unmaintained_flat or yanked_flat:
        warn_details = []
        for u in unmaintained_flat + yanked_flat:
            pkg = u.get("package", {}).get("name", "Unknown")
            kind = u.get("kind", "Unknown")
            warn_details.append(f"- {pkg} ({kind})")

        HIGH_FINDINGS.append({
            "title": "Unmaintained or Yanked Crates",
            "description": "Dependencies flagged as unmaintained or yanked:\n" + "\n".join(warn_details),
            "risk": "Lack of security patches, supply chain risk.",
            "remediation": "Replace unmaintained crates with active alternatives.",
            "effort": "High (1-3 days)"
        })

except json.JSONDecodeError:
    print("Failed to parse cargo audit JSON. Output was:", out)

# 5. Untrusted external input without sanitization
print("Scanning for unsanitized inputs and execution paths...")
unsanitized_found = []
for root, _, files in os.walk("."):
    if ".git" in root or "target" in root:
        continue
    for file in files:
        if file.endswith(".rs"):
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        # Check for SQL injection (format! near SELECT/INSERT/UPDATE/DELETE)
                        if "format!(" in line and re.search(r'(?i)(SELECT|INSERT|UPDATE|DELETE)\s+', line):
                            unsanitized_found.append(f"{filepath}:{i+1} (SQL Injection Risk): {line.strip()}")
                        # Check for env::set_var which is unsafe in multithreaded context
                        elif "env::set_var" in line:
                            unsanitized_found.append(f"{filepath}:{i+1} (Env Var Modification): {line.strip()}")
                        # Check for unvalidated Command args
                        elif "Command::new" in line or ".arg(" in line:
                            # Too noisy for simple regex, but we flag .arg() if it looks like a variable
                            pass
            except Exception:
                pass

if unsanitized_found:
    HIGH_FINDINGS.append({
        "title": "Untrusted Input Paths and SQL Injection Risk",
        "description": "Execution paths potentially accepting untrusted input without sanitization:\n```\n" + "\n".join(unsanitized_found[:20]) + ("\n..." if len(unsanitized_found) > 20 else "") + "\n```",
        "risk": "SQL Injection, Command Injection, Data Races (for env::set_var).",
        "remediation": "Use parameterized queries (e.g., `type::table($param)`), avoid string formatting for queries, and do not use `env::set_var` in application code.",
        "effort": "Medium (4-6 hours)"
    })

# Generate Report and File Issues
print("Generating Report...")
report_content = "# Security Advisory Report\n\n"

def format_finding(f):
    return f"### {f['title']}\n**Description:**\n{f['description']}\n\n**Risk:** {f['risk']}\n**Remediation Recommendation:** {f['remediation']}\n**Estimated Effort:** {f['effort']}\n\n"

if CRITICAL_FINDINGS:
    report_content += "## Critical Findings\n"
    for f in CRITICAL_FINDINGS:
        report_content += format_finding(f)
        issue_title = f"🛡️ Sentinel: [CRITICAL] {f['title']}"
        if not issue_exists(issue_title):
            issue_body = format_finding(f)
            with open("issue_body.txt", "w") as f_out:
                f_out.write(issue_body)
            run_cmd(f'gh issue create --title "{issue_title}" --body-file issue_body.txt')

if HIGH_FINDINGS:
    report_content += "## High Findings\n"
    for f in HIGH_FINDINGS:
        report_content += format_finding(f)
        issue_title = f"🛡️ Sentinel: [HIGH] {f['title']}"
        if not issue_exists(issue_title):
            issue_body = format_finding(f)
            with open("issue_body.txt", "w") as f_out:
                f_out.write(issue_body)
            run_cmd(f'gh issue create --title "{issue_title}" --body-file issue_body.txt')

if MEDIUM_FINDINGS:
    report_content += "## Medium Findings\n"
    for f in MEDIUM_FINDINGS:
        report_content += format_finding(f)

if LOW_FINDINGS:
    report_content += "## Low Findings\n"
    for f in LOW_FINDINGS:
        report_content += format_finding(f)

with open("SECURITY_ADVISORY_REPORT.md", "w") as f:
    f.write(report_content)

if os.path.exists("issue_body.txt"):
    os.remove("issue_body.txt")

print("Security Audit Complete. Report generated at SECURITY_ADVISORY_REPORT.md")
