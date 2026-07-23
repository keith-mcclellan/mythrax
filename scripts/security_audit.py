import os
import subprocess
from datetime import datetime
import json
import shlex

def run_cmd(cmd, cwd=None):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=cwd)
    return result.stdout, result.stderr, result.returncode

def file_github_issue(title, body, labels=[]):
    # Using gh cli in github action
    # Quote the labels carefully to avoid shell injection
    labels_arg = ""
    for label in labels:
        labels_arg += f" --label {shlex.quote(label)}"

    cmd = f"gh issue create --title {shlex.quote(title)} --body {shlex.quote(body)}{labels_arg}"
    out, err, code = run_cmd(cmd)
    if code != 0:
        print(f"Error creating issue: {err}")
    else:
        print(f"Created issue: {out.strip()}")

def scan_codebase():
    report_sections = []
    issues_to_file = []

    # 1. Hardcoded Secrets
    secrets_found = []
    # Generic secret scanning based on keywords, ensuring we don't just capture test files
    cmd = "grep -rnE '(api_key|auth_token|secret|password|credential)\\s*[:=]\\s*\"[a-zA-Z0-9_\\-]+\"' mythrax-core/src/ || true"
    out, _, _ = run_cmd(cmd)
    for line in out.splitlines():
        if "test" not in line.lower() and "mock" not in line.lower():
            secrets_found.append(line.strip())

    if secrets_found:
        body = "Multiple instances of hardcoded API keys/tokens/secrets found:\n"
        for s in secrets_found:
            body += f"- `{s}`\n"
        report_sections.append(f"## Hardcoded Secrets\n\n**Severity:** Critical\n**Description:** {body}\n**Remediation:** Remove all hardcoded secrets from source code. Load dynamically from env vars or secret managers.\n**Estimated Effort:** Low (1-2 days)\n")
        issues_to_file.append(("CRITICAL: Hardcoded Secrets Found in Source Code", body, ["security", "critical"]))

    # 2. Unsafe Rust blocks
    unsafe_blocks = []
    out, _, _ = run_cmd("grep -rn 'unsafe {' mythrax-core/src/ || true")
    for line in out.splitlines():
        parts = line.split(":", 2)
        if len(parts) >= 2:
            file_path = parts[0]
            line_num = int(parts[1])
            prev_out, _, _ = run_cmd(f"sed -n '{line_num-1}p' {file_path} || true")
            if "SAFETY:" not in prev_out.upper() and "SAFETY" not in prev_out.upper():
                unsafe_blocks.append(line.strip())

    if unsafe_blocks:
        body = "Multiple `unsafe` blocks identified without documented justification:\n"
        for b in unsafe_blocks:
            body += f"- `{b}`\n"
        if any("set_var" in b for b in unsafe_blocks):
            body += "\nNote: Modifying environment variables (`set_var`) in a multi-threaded context is undefined behavior and poses a high risk data race.\n"

        report_sections.append(f"## Unsafe Rust Blocks\n\n**Severity:** High\n**Description:** {body}\n**Remediation:** Add `// SAFETY: ...` comments explaining why each `unsafe` block is sound. Refactor `set_var` to avoid modifying global state.\n**Estimated Effort:** Medium (2-3 days)\n")
        issues_to_file.append(("HIGH: Unjustified Unsafe Blocks and Data Race Risks", body, ["security", "high"]))

    # 3. CVEs in Dependencies
    cve_findings = []
    out, err, code = run_cmd("cargo audit", cwd="mythrax-core")
    if code != 0 and "Vulnerability" in out:
        cve_findings.append(out)

    if cve_findings:
        body = "Vulnerable dependencies found via `cargo audit`:\n"
        body += "```text\n" + cve_findings[0][:1500] + "\n```\n"
        report_sections.append(f"## Vulnerable Dependencies (CVEs)\n\n**Severity:** Critical\n**Description:** {body}\n**Remediation:** Update affected crates in `Cargo.toml`/`Cargo.lock`.\n**Estimated Effort:** Low (1 day)\n")
        issues_to_file.append(("CRITICAL: Vulnerable Dependencies (CVEs) Detected", body, ["security", "critical"]))

    # 4. Unsanitized Inputs
    unsanitized = []
    out, _, _ = run_cmd("grep -rnE 'Command::new\\(\"(sh|bash)\"\\)' mythrax-core/src/ || true")
    for line in out.splitlines():
        unsanitized.append(line.strip())

    # Special check for known prompt injection
    precompact_vuln = "mythrax-core/src/hooks/precompact.rs"
    if os.path.exists(precompact_vuln):
        unsanitized.append(f"{precompact_vuln}: Extracts tool results verbatim without sanitization")

    body = "Execution paths that accept untrusted external input without sanitization or boundary enforcement found:\n"
    if unsanitized:
        for u in unsanitized[:10]:
            body += f"- `{u}`\n"
        report_sections.append(f"## Unsanitized External Input\n\n**Severity:** Critical\n**Description:** {body}\n**Remediation:** Avoid passing unsanitized inputs to shell. Sanitize inputs before logging or storing.\n**Estimated Effort:** High (3-5 days)\n")
        issues_to_file.append(("CRITICAL: Unsanitized Input Vulnerabilities Detected", body, ["security", "critical"]))

    # 5. Secrets in git history
    git_secrets = []
    out, _, _ = run_cmd("git log -p | grep -E '(api_key|auth_token|secret|password)\\s*[:=]\\s*\"[a-zA-Z0-9_-]+\"' || true", cwd="mythrax-core")
    for line in out.splitlines():
        if not line.startswith("+++") and not line.startswith("---") and not "grep" in line:
            git_secrets.append(line.strip())

    if git_secrets:
        body2 = "Secrets committed in git history found.\n"
        for s in git_secrets[:5]:
            body2 += f"- `{s}`\n"
        report_sections.append(f"## Secrets in Git History\n\n**Severity:** Critical\n**Description:** {body2}\n**Remediation:** Rewrite git history to remove exposed secrets, and rotate any potentially compromised credentials.\n**Estimated Effort:** High\n")
        issues_to_file.append(("CRITICAL: Secrets in Git History", body2, ["security", "critical"]))

    if not report_sections:
        print("No critical/high issues found.")
        return

    report = "# Mythrax Security Advisory Report\n\nDate: " + datetime.now().strftime("%Y-%m-%d") + "\n\n"
    report += "\n\n---\n\n".join(report_sections)

    with open("security_advisory_report.md", "w") as f:
        f.write(report)

    for title, b, labels in issues_to_file:
        file_github_issue(title, b, labels)

if __name__ == "__main__":
    scan_codebase()
