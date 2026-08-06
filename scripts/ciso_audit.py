#!/usr/bin/env python3
import json
import os
import re
import urllib.request
import urllib.error
import subprocess
import shutil

GITHUB_TOKEN = os.environ.get('GITHUB_TOKEN')
GITHUB_REPOSITORY = os.environ.get('GITHUB_REPOSITORY')

print("# CISO Security Audit Report\n")
report_md = "# CISO Security Audit Report\n\n"

# EXCLUDE DIRS
EXCLUDE_DIRS = ['target', '.git', '.venv', 'node_modules', 'issues']

# HELPER: Get all rust files
rust_files = []
for root, dirs, files in os.walk('.'):
    dirs[:] = [d for d in dirs if d not in EXCLUDE_DIRS]
    for file in files:
        if file.endswith('.rs') or file.endswith('.toml'):
            rust_files.append(os.path.join(root, file))

# 1. Hardcoded Secrets
print("## Hardcoded Secrets\n")
report_md += "## Hardcoded Secrets\n\n"
secret_pattern = re.compile(r"(?i)(api_key|token|secret|password|passwd|pwd|auth_token)\s*[:=]\s*['\"]([a-zA-Z0-9_\-\.]{10,})['\"]")
secrets = []
for filepath in rust_files:
    if not filepath.endswith('.rs') and not filepath.endswith('.toml'):
        continue
    with open(filepath, 'r', encoding='utf-8') as f:
        lines = f.readlines()
        for idx, line in enumerate(lines):
            if secret_pattern.search(line):
                secrets.append(f"- [Critical] **{filepath}:{idx+1}**: Potential hardcoded secret found: `{line.strip()}`. Recommendation: Use environment variables or a secrets manager. Effort: Low.")

if secrets:
    report_md += "\n".join(secrets) + "\n\n"
else:
    report_md += "None found.\n\n"


# 2. Git History Secrets
print("## Git History Secrets\n")
report_md += "## Git History Secrets\n\n"
git_secrets = []
git_secret_pattern = re.compile(r"(?i)(api_key|token|secret|password|passwd|pwd|auth_token)\s*[:=]\s*['\"]([a-zA-Z0-9_\-\.]{10,})['\"]")
try:
    history_cmd = subprocess.run(
        r'git log -E -p -G "(api_key|token|secret|password|passwd|pwd|auth_token)[[:space:]]*[:=][[:space:]]*[\'\"].{10,}[\'\"]" | grep -E "^\+" | grep -E -v "^\+\+\+"',
        shell=True,
        capture_output=True,
        text=True
    ).stdout
    if history_cmd:
        for line in history_cmd.split('\n'):
            if line.strip() and git_secret_pattern.search(line):
                git_secrets.append(f"- [High] Potential secret found in git history: `{line.strip()}`. Recommendation: Revoke the secret and scrub from history using BFG or git-filter-repo. Effort: Medium.")
except Exception as e:
    print(f"Failed to scan git history: {e}")

if git_secrets:
    git_secrets = list(set(git_secrets))
    report_md += "\n".join(git_secrets) + "\n\n"
else:
    report_md += "None found.\n\n"

# 3. Unsafe Rust
print("## Unsafe Rust Blocks\n")
report_md += "## Unsafe Rust Blocks\n\n"
unsafe_blocks = []
for filepath in rust_files:
    if not filepath.endswith('.rs'):
        continue
    with open(filepath, 'r', encoding='utf-8') as f:
        lines = f.readlines()
        for idx, line in enumerate(lines):
            if 'unsafe' in line:
                if line.strip().startswith('//') and not 'safety' in line.lower():
                    continue

                has_comment = idx > 0 and 'safety' in lines[idx-1].lower()
                if 'safety' in line.lower():
                    has_comment = True

                status = "Documented" if has_comment else "UNDOCUMENTED"
                if not has_comment:
                    unsafe_blocks.append(f"- [Medium] **{filepath}:{idx+1}**: `{line.strip()}` ({status}). Memory safety risk. Recommendation: Document justification with a `// SAFETY:` comment or rewrite using safe Rust. Effort: Low.")
                else:
                    unsafe_blocks.append(f"- [Low] **{filepath}:{idx+1}**: `{line.strip()}` ({status}).")

if unsafe_blocks:
    report_md += "\n".join(unsafe_blocks) + "\n\n"
else:
    report_md += "None found.\n\n"

# 4. Cargo.lock dependencies
print("## Dependency Vulnerabilities & Warnings\n")
report_md += "## Dependency Vulnerabilities & Warnings\n\n"

vulnerabilities = []
warnings = {}

# Run cargo audit
try:
    subprocess.run(['cargo', 'audit', '--json'], cwd='mythrax-core', capture_output=True, text=True, check=False)
except Exception as e:
    print(f"cargo audit error: {e}")

try:
    # Check if we can capture it directly
    audit_res = subprocess.run(['cargo', 'audit', '--json'], cwd='mythrax-core', capture_output=True, text=True)
    if audit_res.stdout:
        audit_data = json.loads(audit_res.stdout)
        vulnerabilities = audit_data.get('vulnerabilities', {}).get('list', [])
        warnings = audit_data.get('warnings', {})
except Exception as e:
    print(f"Failed to parse cargo audit JSON: {e}")

if vulnerabilities:
    report_md += "### Vulnerabilities\n"
    for v in vulnerabilities:
        pkg = v['advisory']['package']
        title = v['advisory']['title']
        report_md += f"- [High] **{pkg}**: {title}. Recommendation: Update crate to patched version. Effort: Low.\n"
else:
    report_md += "No vulnerabilities found.\n"

yanked = warnings.get('yanked', [])
if yanked:
    report_md += "\n### Yanked Packages\n"
    for y in yanked:
        pkg = y['package']['name']
        report_md += f"- [Medium] **{pkg}**: Yanked. Recommendation: Switch to an alternative or updated crate. Effort: Low.\n"

unmaintained = warnings.get('unmaintained', [])
if unmaintained:
    report_md += "\n### Unmaintained Packages\n"
    for u in unmaintained:
        pkg = u['package']['name']
        report_md += f"- [Low] **{pkg}**: Unmaintained. Recommendation: Consider migrating to actively maintained alternatives. Effort: High.\n"

unsound = warnings.get('unsound', [])
if unsound:
    report_md += "\n### Unsound Packages\n"
    for u in unsound:
        pkg = u['package']['name']
        title = u['advisory']['title']
        report_md += f"- [Medium] **{pkg}**: {title} (Unsound). Recommendation: Update crate or use alternative. Effort: Medium.\n"
report_md += "\n"

# 5. Untrusted Execution paths
print("## Potential Command Injection (Untrusted Execution Paths)\n")
report_md += "## Potential Command Injection (Untrusted Execution Paths)\n\n"
cmd_paths = []
for filepath in rust_files:
    if not filepath.endswith('.rs'):
        continue
    with open(filepath, 'r', encoding='utf-8') as f:
        lines = f.readlines()
        for idx, line in enumerate(lines):
            if 'Command::new' in line:
                if '.arg(' in line or '.args(' in line or '(&' in line or '(var' in line or 'cmd_clone' in line or 'command_name' in line or '&args[0]' in line:
                    cmd_paths.append(f"- [High] **{filepath}:{idx+1}**: `{line.strip()}`. Recommendation: Ensure arguments are sanitized and bounds checked to prevent command injection. Effort: Medium.")
                elif idx + 1 < len(lines) and ('.arg(' in lines[idx+1] or '.args(' in lines[idx+1]):
                    cmd_paths.append(f"- [High] **{filepath}:{idx+1}**: `{line.strip()}`. Recommendation: Ensure arguments are sanitized and bounds checked to prevent command injection. Effort: Medium.")

if cmd_paths:
    report_md += "\n".join(cmd_paths) + "\n\n"
else:
    report_md += "None found.\n\n"

print(report_md)

with open('ciso_report.md', 'w') as f:
    f.write(report_md)


def create_github_issue(title, body):
    if GITHUB_TOKEN and GITHUB_REPOSITORY:
        url = f"https://api.github.com/repos/{GITHUB_REPOSITORY}/issues"
        headers = {
            "Authorization": f"token {GITHUB_TOKEN}",
            "Accept": "application/vnd.github.v3+json"
        }
        data = {
            "title": title,
            "body": body,
            "labels": ["bug", "security"]
        }
        try:
            req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
            with urllib.request.urlopen(req) as response:
                print(f"Created issue: {title}")
                return True
        except urllib.error.HTTPError as e:
            print(f"Failed to create issue via API: {e.code} {e.reason}")
            return False
        except Exception as e:
            print(f"Failed to create issue via API: {e}")
            return False

    # Fallback to gh cli
    if shutil.which('gh'):
        try:
            subprocess.run(['gh', 'issue', 'create', '--title', title, '--body', body, '--label', 'bug,security'], check=True)
            print(f"Created issue via gh cli: {title}")
            return True
        except Exception as e:
            print(f"Failed to create issue via gh cli: {e}")

    print(f"WARNING: Cannot file issue. Missing GITHUB_TOKEN/GITHUB_REPOSITORY and `gh` cli tool. Title: {title}")
    return False

# File issues
issues_created = 0
for s in secrets:
    create_github_issue("Critical: Hardcoded Secret Found", s)
    issues_created += 1

for g in git_secrets:
    create_github_issue("High: Secret Found in Git History", g)
    issues_created += 1

for v in vulnerabilities:
    pkg = v['advisory']['package']
    title = v['advisory']['title']
    create_github_issue(f"High: Vulnerability in {pkg}", f"{title}. Recommendation: Update crate to patched version.")
    issues_created += 1

for c in cmd_paths:
    if '[High]' in c:
        create_github_issue("High: Potential Command Injection", c)
        issues_created += 1

print(f"Finished CISO Audit. Identified and attempted to file {issues_created} issues.")
