#!/usr/bin/env python3
import os
import re
import urllib.request
import json
import subprocess
import datetime
import shlex

def run_cmd(cmd):
    try:
        return subprocess.check_output(cmd, shell=True, text=True, stderr=subprocess.STDOUT)
    except subprocess.CalledProcessError as e:
        print(f"Command failed: {cmd}\nOutput: {e.output}")
        return e.output

is_github_actions = 'GITHUB_ACTIONS' in os.environ

report = "# Mythrax Security Advisory Report\n\n"
issues = []
issue_counter = 0

def check_issue_exists(title):
    if not is_github_actions:
        return False

    try:
        cmd = f"gh issue list --search {shlex.quote(title + ' in:title')} --json title"
        output = subprocess.check_output(cmd, shell=True, text=True, stderr=subprocess.STDOUT)
        issues_list = json.loads(output)
        for issue in issues_list:
            if issue.get("title") == title:
                return True
        return False
    except subprocess.CalledProcessError as e:
        print(f"Error checking if issue exists: {e.output}")
        # If we can't check, assume it exists to prevent spam
        return True
    except json.JSONDecodeError:
        return True

def add_issue(title, body, labels):
    global issue_counter
    issues.append({
        "title": title,
        "body": body,
        "labels": labels
    })

    if is_github_actions:
        if check_issue_exists(title):
            print(f"Issue '{title}' already exists. Skipping.")
        else:
            print(f"Creating issue: {title}")
            cmd = f"gh issue create --title {shlex.quote(title)} --body {shlex.quote(body)} --label {shlex.quote(labels)}"
            run_cmd(cmd)
    else:
        # Local development / dry run
        print(f"Would create issue: {title}")
        issue_file = f"issues/issue_{issue_counter}.md"
        os.makedirs("issues", exist_ok=True)
        with open(issue_file, "w") as f:
            f.write(f"# {title}\n\nLabels: {labels}\n\n{body}")
        issue_counter += 1


# 1. Hardcoded Secrets
print("Scanning for secrets in codebase...")
report += "## Hardcoded Secrets\n\n"
secrets_found = []
secret_patterns = re.compile(r'(?i)(password|token|secret|api_key|apikey|credentials)\s*=\s*["\'][^"\']+["\']')
for root, _, files in os.walk('mythrax-core'):
    for file in files:
        if file.endswith('.rs') or file.endswith('.toml'):
            filepath = os.path.join(root, file)
            with open(filepath, 'r') as f:
                content = f.read()
                matches = secret_patterns.findall(content)
                if matches:
                    secrets_found.append((filepath, matches))

if secrets_found:
    for filepath, matches in secrets_found:
        report += f"- **Critical**: Found secret in `{filepath}`\n  - Remediation: Extract secrets to environment variables or secure vault.\n  - Effort: Low\n"
        add_issue(
            f"Hardcoded Secret in {filepath}",
            f"Found hardcoded secrets in `{filepath}`. This is a critical security risk.\n\n**Remediation:** Extract secrets to environment variables or secure vault.\n**Severity:** Critical\n**Effort:** Low",
            "security,critical,bug,agent-found"
        )
else:
    report += "No hardcoded secrets found in current HEAD.\n\n"

# Secrets in git history
print("Scanning git history for secrets...")
report += "\n## Git History Secrets\n\n"
git_secrets = run_cmd("git log -p -E -i -G '(password|token|secret|api_key|apikey|credentials)[[:space:]]*=[[:space:]]*[^ ]+' --all")
if git_secrets.strip() and not "fatal" in git_secrets.lower():
    report += "- **High**: Potential secrets found in git history.\n  - Remediation: Use BFG or git-filter-repo to scrub history.\n  - Effort: Medium\n\n"
    add_issue(
        "Potential secrets in Git History",
        "Found potential secrets committed in git history.\n\n**Remediation:** Use BFG or git-filter-repo to scrub history.\n**Severity:** High\n**Effort:** Medium",
        "security,high,bug,agent-found"
    )
else:
    report += "No obvious secrets found in git history.\n\n"

# 2. Unsafe Blocks
print("Scanning unsafe blocks...")
report += "## Unsafe Rust Blocks\n\n"
unsafe_blocks = []
unsafe_issue_body = ""
for root, _, files in os.walk('mythrax-core/src'):
    for file in files:
        if file.endswith('.rs'):
            filepath = os.path.join(root, file)
            with open(filepath, 'r') as f:
                lines = f.readlines()
                for i, line in enumerate(lines):
                    if 'unsafe ' in line or 'unsafe{' in line or 'unsafe {' in line:
                        has_comment = False
                        for j in range(max(0, i-5), i):
                            if 'safety' in lines[j].lower() or 'safe:' in lines[j].lower():
                                has_comment = True
                        if not has_comment:
                            unsafe_blocks.append((filepath, i+1, line.strip()))
                            start_context = max(0, i-2)
                            end_context = min(len(lines), i+3)
                            context = ""
                            for j in range(start_context, end_context):
                                context += f"{j+1}: {lines[j]}"

                            unsafe_issue_body += f"- `{filepath}:{i+1}`:\n```rust\n{context}```\n\n"

                            risk = "Memory safety violation, undefined behavior, possible memory leaks or race conditions."
                            if "libc::" in line:
                                risk = "Calling unsafe FFI (libc). Incorrect usage can lead to undefined behavior, crashes, or privilege escalation."
                            elif "set_var" in line:
                                risk = "Setting environment variables is unsafe in multi-threaded contexts in Rust and can cause data races."
                            elif "Send" in line or "Sync" in line:
                                risk = "Implementing Send/Sync manually can lead to data races if the type is not actually thread-safe."
                            elif "zeroed" in line:
                                risk = "Using uninitialized memory via zeroed() can lead to undefined behavior if the type has non-zero invariants."

                            report += f"- **High**: `{filepath}:{i+1}` lacks documented justification.\n  - Code: `{line.strip()}`\n  - Risk: {risk}\n  - Remediation: Add `// Safety:` comments explaining why each block is memory safe, or refactor to safe Rust.\n  - Effort: Medium\n"

if unsafe_blocks:
    add_issue(
        "Undocumented Unsafe Blocks Found",
        f"Found `unsafe` blocks without documentation and their potential risks:\n\n{unsafe_issue_body}\n\n**Remediation:** Add `// Safety:` comments explaining why each block is memory safe, or refactor to safe Rust.\n**Severity:** High\n**Effort:** Medium",
        "security,high,bug,agent-found"
    )
else:
    report += "All `unsafe` blocks appear to have safety comments.\n\n"

# 3. Dependencies
print("Checking dependencies...")
report += "\n## Dependencies\n\n"
def check_crates_io(crate_name):
    url = f"https://crates.io/api/v1/crates/{crate_name}"
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'mythrax-security-audit'})
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            return data
    except Exception as e:
        return None

deps = []
with open('mythrax-core/Cargo.toml', 'r') as f:
    in_deps = False
    for line in f:
        line = line.strip()
        if line.startswith('[dependencies]') or line.startswith('[build-dependencies]'):
            in_deps = True
            continue
        if line.startswith('['):
            in_deps = False
        if in_deps and '=' in line and not line.startswith('#'):
            parts = line.split('=')
            crate_name = parts[0].strip().strip('"').strip("'")
            deps.append(crate_name)

dep_findings = []
import time
for crate in deps:
    time.sleep(1) # Respect crates.io rate limits
    data = check_crates_io(crate)
    if data:
        crate_info = data.get('crate', {})
        if crate_info.get('yanked', False):
            dep_findings.append(f"**Critical**: {crate} is yanked.\n  - Remediation: Update or replace dependency.\n  - Effort: Low")
            add_issue(
                f"Yanked Dependency: {crate}",
                f"Dependency `{crate}` is yanked on crates.io.\n\n**Remediation:** Update or replace dependency.\n**Severity:** Critical\n**Effort:** Low",
                "security,critical,bug,agent-found"
            )
        updated_at = crate_info.get('updated_at')
        if updated_at:
            dt = datetime.datetime.fromisoformat(updated_at.replace('Z', '+00:00'))
            if (datetime.datetime.now(datetime.timezone.utc) - dt).days > 365:
                dep_findings.append(f"**Low**: {crate} has no commits/updates in over a year.\n  - Remediation: Audit crate for necessity or migrate to maintained alternative.\n  - Effort: Medium")

for finding in dep_findings:
    report += f"- {finding}\n"

# Cargo Audit
audit_out = run_cmd("cd mythrax-core && cargo audit -q")
if "error: no such command" in audit_out or "No such file or directory" in audit_out:
    print("Installing cargo-audit...")
    run_cmd("cargo install cargo-audit --locked")
    audit_out = run_cmd("cd mythrax-core && cargo audit -q")

if "Vulnerabilities found" in audit_out or "Crate:" in audit_out:
    report += "\n### Cargo Audit Findings\n"
    report += f"**High**: Cargo Audit found vulnerabilities.\n  - Remediation: Run `cargo update` or upgrade vulnerable packages.\n  - Effort: Medium\n```\n{audit_out}\n```\n"
    add_issue(
        "Cargo Audit Found Vulnerabilities",
        f"Run `cargo audit` to see details. Fix vulnerable dependencies.\n\n**Remediation:** Run `cargo update` or upgrade vulnerable packages.\n**Severity:** High\n**Effort:** Medium",
        "security,high,bug,agent-found"
    )
else:
    report += "\nNo known CVEs found by cargo audit.\n"

# 4. Untrusted Input
print("Scanning for untrusted input...")
report += "\n## Untrusted External Input\n\n"
input_patterns = re.compile(r'(?i)(Json|Query|Path)<')
input_findings = []
for root, _, files in os.walk('mythrax-core/src'):
    for file in files:
        if file.endswith('.rs'):
            filepath = os.path.join(root, file)
            with open(filepath, 'r') as f:
                content = f.read()
                if input_patterns.search(content):
                    input_findings.append(filepath)
                    report += f"- **Medium**: Found potential untrusted input endpoints in `{filepath}`. Requires validation of bounds/sanitization.\n  - Remediation: Ensure all API endpoints implement boundary enforcement and sanitization for JSON/Query/Path extractors.\n  - Effort: High\n"

if input_findings:
    # Only report in markdown, do not file issue for Medium
    pass


with open("security_report.md", "w") as f:
    f.write(report)

print("Report written to security_report.md")
