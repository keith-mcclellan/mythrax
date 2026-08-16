import os
import subprocess
import json
import re
import sys
from datetime import datetime
import urllib.request
import urllib.error

# Regex patterns for secret scanning
SECRET_REGEXES = {
    "AWS Access Key": r"AKIA[0-9A-Z]{16}",
    "GitHub Token": r"ghp_[0-9a-zA-Z]{36}",
    "Generic Secret": r"(secret|token|password|api_key|apikey)[\s:=]+[\"'][^\s\"']{10,}[\"']"
}

def scan_secrets(directory):
    findings = []
    # Recompile for local scan with case insensitive since we removed (?i)
    local_regexes = {name: re.compile(pattern, re.IGNORECASE) for name, pattern in SECRET_REGEXES.items()}

    for root, _, files in os.walk(directory):
        if ".git" in root or "target" in root:
            continue
        for file in files:
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8") as f:
                    content = f.read()
                    for name, regex in local_regexes.items():
                        for match in regex.finditer(content):
                            findings.append({
                                "type": "Hardcoded Secret",
                                "severity": "Critical",
                                "file": filepath,
                                "description": f"Found potential {name}",
                                "remediation": "Remove secret from source code and use environment variables or a secret manager.",
                                "effort": "Low"
                            })
            except Exception:
                pass
    return findings

def scan_git_history():
    findings = []
    try:
        # Use git log -G to search efficiently
        # Note: POSIX ERE (-E) doesn't support (?i), so we have to use -i with -E
        # and construct appropriate regexes for git log.
        for name, clean_regex in SECRET_REGEXES.items():

            # git log -G searches for the regex in the diffs
            cmd = ["git", "log", "-G", clean_regex, "-E", "-i", "--format=%H"]
            commits_out = subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL)
            commits = [c for c in commits_out.splitlines() if c.strip()]

            for commit in commits:
                findings.append({
                    "type": "Secret in Git History",
                    "severity": "Critical",
                    "file": commit,
                    "description": f"Found potential {name} in commit {commit}",
                    "remediation": "Rotate the compromised secret immediately. Consider rewriting git history using BFG or git-filter-repo, though revocation is safer.",
                    "effort": "Medium"
                })
    except Exception as e:
        print(f"Error scanning git history: {e}")
    return findings

def check_unsafe_blocks(directory):
    findings = []
    unsafe_re = re.compile(r"unsafe\s*\{")
    for root, _, files in os.walk(directory):
        if ".git" in root or "target" in root:
            continue
        for file in files:
            if not file.endswith(".rs"):
                continue
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8") as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if unsafe_re.search(line):
                            # Check previous 5 lines for a comment
                            has_comment = False
                            for j in range(max(0, i-5), i+1):
                                if "// Safety" in lines[j] or "// Safety:" in lines[j] or "// SAFETY" in lines[j] or "// safety:" in lines[j]:
                                    has_comment = True
                                    break

                            severity = "Medium" if has_comment else "High"
                            desc = f"Unsafe block at line {i+1}. "
                            if not has_comment:
                                desc += "Lacks a documented justification comment. "
                            desc += "Risk: Memory safety violations (e.g., use-after-free, buffer overflows, data races)."

                            findings.append({
                                "type": "Unsafe Rust",
                                "severity": severity,
                                "file": filepath,
                                "description": desc,
                                "remediation": "Add a '// SAFETY:' comment explaining why the unsafe block is sound, or refactor to use safe Rust.",
                                "effort": "Medium"
                            })
            except Exception:
                pass
    return findings

def check_dependencies(directory):
    findings = []
    try:
        # Run cargo audit
        result = subprocess.run(["cargo-audit", "audit", "--json"], cwd=directory, capture_output=True, text=True)
        if result.returncode != 0 and result.stdout == "":
             # cargo-audit might not be in PATH if run locally without setup
             audit_bin = "../cargo-audit" if os.path.exists("../cargo-audit") else "cargo-audit"
             result = subprocess.run([audit_bin, "audit", "--json"], cwd=directory, capture_output=True, text=True)

        if result.stdout:
            data = json.loads(result.stdout)

            # Check vulnerabilities
            for vuln in data.get("vulnerabilities", {}).get("list", []):
                advisory = vuln.get("advisory", {})
                cvss_base = None
                cvss = advisory.get("cvss")
                if isinstance(cvss, dict):
                    cvss_base = cvss.get("base")
                elif isinstance(cvss, str):
                    # Sometimes cvss is a raw string vector like CVSS:3.1/AV:N/AC:L...
                    cvss_base = None

                severity = "Medium"
                if cvss_base:
                    try:
                        score = float(cvss_base)
                        if score >= 9.0: severity = "Critical"
                        elif score >= 7.0: severity = "High"
                    except:
                        pass

                findings.append({
                    "type": "Vulnerable Dependency",
                    "severity": severity,
                    "file": "Cargo.lock",
                    "description": f"{vuln.get('package', {}).get('name')} {vuln.get('package', {}).get('version')} - {advisory.get('title')} ({advisory.get('id')})",
                    "remediation": f"Update {vuln.get('package', {}).get('name')} to a patched version.",
                    "effort": "Low"
                })

            # Check warnings (yanked, unmaintained)
            for warning_type, warnings in data.get("warnings", {}).items():
                for warning in warnings:
                    kind = warning.get("kind")
                    pkg = warning.get("package", {})
                    findings.append({
                        "type": "Dependency Warning",
                        "severity": "Low",
                        "file": "Cargo.lock",
                        "description": f"{pkg.get('name')} {pkg.get('version')} - {kind}",
                        "remediation": f"Investigate and potentially replace {pkg.get('name')}.",
                        "effort": "High"
                    })
    except Exception as e:
        print(f"Error checking dependencies: {e}")
    return findings

def check_input_sanitization(directory):
    findings = []
    # Search for potential untrusted input endpoints (e.g., axum routes, warp, etc.)
    # and lack of sanitization. This is a heuristic.
    untrusted_patterns = [
        r"axum::extract::(Query|Json|Path|Form)",
        r"warp::(query|body|path)"
    ]

    for root, _, files in os.walk(directory):
        if ".git" in root or "target" in root:
            continue
        for file in files:
            if not file.endswith(".rs"):
                continue
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8") as f:
                    content = f.read()
                    for pattern in untrusted_patterns:
                        if re.search(pattern, content):
                            # Heuristic: flag if we see extracting input but no obvious validation
                            if "validator" not in content and "sanitize" not in content and "validate" not in content:
                                findings.append({
                                    "type": "Missing Input Validation",
                                    "severity": "High",
                                    "file": filepath,
                                    "description": f"File extracts untrusted input ({pattern}) but lacks obvious validation/sanitization.",
                                    "remediation": "Implement strict input validation and boundary enforcement for all external data.",
                                    "effort": "Medium"
                                })
            except Exception:
                pass
    return findings

def main():
    print("Starting security audit...")
    findings = []

    print("Scanning for hardcoded secrets in current tree...")
    findings.extend(scan_secrets("."))

    print("Scanning for secrets in git history...")
    findings.extend(scan_git_history())

    print("Checking for unsafe blocks...")
    findings.extend(check_unsafe_blocks("mythrax-core"))

    print("Checking dependencies...")
    findings.extend(check_dependencies("mythrax-core"))

    print("Checking input sanitization...")
    findings.extend(check_input_sanitization("mythrax-core"))

    # Sort findings by severity
    severity_order = {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}
    findings.sort(key=lambda x: severity_order.get(x["severity"], 4))

    # Generate report
    report_path = "security_advisory_report.md"
    with open(report_path, "w") as f:
        f.write("# Security Advisory Report\n\n")
        f.write(f"**Date:** {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n\n")

        for severity in ["Critical", "High", "Medium", "Low"]:
            sev_findings = [x for x in findings if x["severity"] == severity]
            if sev_findings:
                f.write(f"## {severity} Findings ({len(sev_findings)})\n\n")
                for item in sev_findings:
                    f.write(f"### {item['type']} in `{item['file']}`\n")
                    f.write(f"- **Description:** {item['description']}\n")
                    f.write(f"- **Remediation:** {item['remediation']}\n")
                    f.write(f"- **Estimated Effort:** {item['effort']}\n\n")

    print(f"Report generated at {report_path}")

    # Check if we should file issues
    if os.environ.get("GITHUB_ACTIONS"):
        for finding in findings:
            if finding["severity"] in ["Critical", "High"]:
                title = f"[Security - {finding['severity']}] {finding['type']} in {os.path.basename(finding['file'])}"
                body = f"**File:** `{finding['file']}`\n**Description:** {finding['description']}\n**Remediation:** {finding['remediation']}\n**Effort:** {finding['effort']}"

                # Check if issue already exists
                check_cmd = ["gh", "issue", "list", "--state", "all", "--search", f'"{title}"', "--json", "number"]
                try:
                    result = subprocess.run(check_cmd, capture_output=True, text=True, check=True)
                    issues = json.loads(result.stdout)
                    if not issues:
                        print(f"Creating issue: {title}")
                        subprocess.run(["gh", "issue", "create", "--title", title, "--body", body, "--label", "security"], check=True)
                    else:
                        print(f"Issue already exists: {title}")
                except Exception as e:
                    print(f"Failed to interact with GitHub CLI: {e}")
                    sys.exit(1)
    else:
        # Local mock issues
        os.makedirs("issues", exist_ok=True)
        for i, finding in enumerate(findings):
            if finding["severity"] in ["Critical", "High"]:
                title = f"[Security - {finding['severity']}] {finding['type']} in {os.path.basename(finding['file'])}"
                body = f"**File:** `{finding['file']}`\n**Description:** {finding['description']}\n**Remediation:** {finding['remediation']}\n**Effort:** {finding['effort']}"
                with open(f"issues/security_{i}.md", "w") as f:
                    f.write(f"# {title}\n\n{body}")
        print("Generated mock issues in issues/ directory.")

if __name__ == "__main__":
    main()
