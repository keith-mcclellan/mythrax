import os
import re
import json
import subprocess
import shutil
import sys
import datetime
from urllib.request import urlopen, Request

def get_last_commit_date(pkg_name):
    # This is a basic attempt to find the repo URL from crates.io and check recent commits
    # It might fail or hit rate limits, so we handle exceptions gracefully
    try:
        url = f"https://crates.io/api/v1/crates/{pkg_name}"
        req = Request(url, headers={'User-Agent': 'Mythrax-Security-Audit/1.0'})
        with urlopen(req, timeout=5) as response:
            data = json.loads(response.read().decode())
            repo_url = data.get('crate', {}).get('repository')
            if repo_url and 'github.com' in repo_url:
                parts = repo_url.rstrip('/').split('/')
                if len(parts) >= 2:
                    owner, repo = parts[-2], parts[-1]
                    if repo.endswith('.git'):
                        repo = repo[:-4]
                    gh_url = f"https://api.github.com/repos/{owner}/{repo}/commits?per_page=1"

                    headers = {'User-Agent': 'Mythrax-Security-Audit/1.0'}
                    token = os.environ.get("GITHUB_TOKEN")
                    if token:
                        headers['Authorization'] = f"token {token}"

                    gh_req = Request(gh_url, headers=headers)
                    with urlopen(gh_req, timeout=5) as gh_response:
                        commits = json.loads(gh_response.read().decode())
                        if commits:
                            date_str = commits[0]['commit']['committer']['date']
                            date_obj = datetime.datetime.strptime(date_str, "%Y-%m-%dT%H:%M:%SZ")
                            return date_obj
    except Exception:
        pass
    return None

def audit_codebase():
    report = "# Mythrax Security Advisory Report\n\n"
    issues = []

    # 1. Hardcoded Secrets
    report += "## 1. Hardcoded Secrets\n\n"
    secrets_regex = re.compile(r'(?i)(secret|token|api[_-]?key|password)\s*[:=]\s*[\'"]([^\'"]{8,})[\'"]')
    secrets_found = False
    for root, dirs, files in os.walk('mythrax-core'):
        if 'target' in root.split(os.sep):
            continue
        for file in files:
            if file.endswith(('.rs', '.toml', '.json', '.yaml', '.yml')):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r') as f:
                        for i, line in enumerate(f):
                            match = secrets_regex.search(line)
                            if match and 'test' not in filepath and 'test' not in file:
                                secrets_found = True
                                report += f"### [Critical] Hardcoded Secret in `{filepath}`\n"
                                report += f"- **Location:** `{filepath}:{i+1}`\n"
                                report += f"- **Description:** Found potential hardcoded secret.\n"
                                report += f"- **Remediation:** Remove the hardcoded secret from the source code and use environment variables.\n"
                                report += f"- **Effort:** Low\n\n"
                                issues.append({
                                    "title": f"[Critical] Hardcoded Secret in {filepath}:{i+1}",
                                    "body": f"Found potential hardcoded secret.\n\n**Remediation:** Remove the hardcoded secret from the source code and use environment variables.\n**Effort:** Low"
                                })
                except Exception:
                    pass
    if not secrets_found:
        report += "No hardcoded secrets found.\n\n"

    # 2. Unsafe Rust Blocks
    report += "## 2. Unsafe Rust Blocks\n\n"
    unsafe_regex = re.compile(r'\bunsafe\s*\{')
    unsafe_found = False
    for root, dirs, files in os.walk('mythrax-core/src'):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r') as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            if unsafe_regex.search(line):
                                unsafe_found = True
                                has_comment = False
                                for j in range(max(0, i - 10), i):
                                    if 'SAFETY' in lines[j] or 'Safety' in lines[j] or 'safety' in lines[j]:
                                        has_comment = True
                                        break

                                content = line.strip()
                                if len(content) > 0:
                                     if i + 1 < len(lines):
                                         content += " " + lines[i+1].strip()

                                risk = "Unknown memory safety risk; requires manual review."
                                if "libc" in content or "ffi" in content.lower():
                                    risk = "FFI calls bypass Rust's borrow checker and can lead to segfaults, buffer overflows, or undefined behavior if invariants are violated."
                                elif "env::set_var" in content:
                                     risk = "Modifying environment variables in a multi-threaded program is undefined behavior before Rust 1.80, potentially causing data races."
                                elif "ptr::" in content or "as *" in content:
                                     risk = "Raw pointer dereferencing can lead to use-after-free, null pointer dereferences, or memory corruption."

                                if has_comment:
                                    report += f"### [Low] Documented Unsafe block in `{filepath}`\n"
                                    report += f"- **Location:** `{filepath}:{i+1}`\n"
                                    report += f"- **Description:** Calls `unsafe` block with a SAFETY comment. **Risk:** {risk}\n"
                                    report += f"- **Remediation:** Ensure invariants are upheld.\n"
                                    report += f"- **Effort:** Low\n\n"
                                else:
                                    report += f"### [High] Undocumented Unsafe block in `{filepath}`\n"
                                    report += f"- **Location:** `{filepath}:{i+1}`\n"
                                    report += f"- **Description:** Calls `unsafe` block without a SAFETY comment. **Risk:** {risk}\n"
                                    report += f"- **Remediation:** Review the memory invariants, ensure they are upheld, and add a `// SAFETY: ...` comment explaining why it's safe.\n"
                                    report += f"- **Effort:** Medium\n\n"
                                    issues.append({
                                        "title": f"[High] Undocumented Unsafe block in {filepath}:{i+1}",
                                        "body": f"Calls `unsafe` block without a SAFETY comment. **Risk:** {risk}\n\n**Remediation:** Review invariants and add a `// SAFETY: ...` comment.\n**Effort:** Medium"
                                    })
                except Exception:
                    pass
    if not unsafe_found:
        report += "No unsafe blocks found.\n\n"

    # 3. Cargo Audit & Dependency Health
    report += "## 3. Dependency Vulnerabilities\n\n"
    audit_found = False
    checked_crates = set()
    six_months_ago = datetime.datetime.now() - datetime.timedelta(days=180)

    if shutil.which("cargo-audit"):
        res = subprocess.run(["cargo", "audit", "--json"], cwd="mythrax-core", capture_output=True, text=True)
        if res.stdout:
            try:
                data = json.loads(res.stdout)
                for vuln in data.get("vulnerabilities", {}).get("list", []):
                    audit_found = True
                    pkg = vuln.get('package', {}).get('name', 'unknown')
                    ver = vuln.get('package', {}).get('version', 'unknown')
                    advisory_id = vuln.get('advisory', {}).get('id', 'unknown')
                    title = vuln.get('advisory', {}).get('title', 'unknown')
                    checked_crates.add(pkg)

                    report += f"### [Critical] Vulnerability in `{pkg} {ver}`\n"
                    report += f"- **Vulnerability:** {advisory_id} - {title}\n"
                    report += f"- **Remediation:** Run `cargo update` or bump the specific dependency versions to patched releases.\n"
                    report += f"- **Effort:** Low\n\n"
                    issues.append({
                        "title": f"[Critical] Vulnerability in {pkg} {ver}",
                        "body": f"**Vulnerability:** {advisory_id} - {title}\n\n**Remediation:** Run `cargo update` or bump the specific dependency versions to patched releases.\n**Effort:** Low"
                    })

                for warning_type, warnings_list in data.get("warnings", {}).items():
                    for w in warnings_list:
                        audit_found = True
                        pkg = w.get('package', {}).get('name', 'unknown')
                        ver = w.get('package', {}).get('version', 'unknown')
                        checked_crates.add(pkg)
                        report += f"### [Medium] {warning_type.capitalize()} crate `{pkg} {ver}`\n"
                        report += f"- **Description:** {pkg} is marked as {warning_type}.\n"
                        report += f"- **Remediation:** Migrate away from unmaintained and yanked crates. Update unsound crates to fixed versions.\n"
                        report += f"- **Effort:** Medium to High\n\n"
            except Exception:
                pass

    # Parse Cargo.lock to check for unmaintained/no recent commits
    try:
        with open("mythrax-core/Cargo.lock", "r") as f:
             content = f.read()
             # naive parse
             for block in content.split("[[package]]"):
                 if "name =" in block:
                     name_match = re.search(r'name = "([^"]+)"', block)
                     if name_match:
                         pkg = name_match.group(1)
                         if pkg not in checked_crates and not pkg.startswith("mythrax"):
                             checked_crates.add(pkg)
                             last_commit = get_last_commit_date(pkg)
                             if last_commit and last_commit < six_months_ago:
                                 audit_found = True
                                 report += f"### [Medium] Stale Dependency: `{pkg}`\n"
                                 report += f"- **Description:** No recent commits found in the last 6 months (last commit: {last_commit.strftime('%Y-%m-%d')}). Supply chain risk.\n"
                                 report += f"- **Remediation:** Evaluate if the crate is feature-complete or abandoned. Consider migrating to actively maintained alternatives.\n"
                                 report += f"- **Effort:** Medium\n\n"
    except Exception:
        pass

    if not audit_found:
        report += "No vulnerable dependencies found.\n\n"

    # 4. Untrusted Input
    report += "## 4. Execution Paths (Command Injection Risks)\n\n"
    untrusted_found = False
    for root, dirs, files in os.walk('mythrax-core/src'):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r') as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            if 'Command::new' in line:
                                untrusted_found = True
                                report += f"### [Critical] Potential Command Injection in `{filepath}`\n"
                                report += f"- **Location:** `{filepath}:{i+1}`\n"
                                report += f"- **Description:** Unsanitized Input in Shell Commands (`tokio::process::Command`, `std::process::Command`).\n"
                                report += f"- **Remediation:** Validate and sanitize all external inputs before passing them to external commands. Enforce strict boundary checks.\n"
                                report += f"- **Effort:** Medium\n\n"
                                issues.append({
                                    "title": f"[Critical] Potential Command Injection in {filepath}:{i+1}",
                                    "body": f"Unsanitized Input in Shell Commands (`tokio::process::Command`, `std::process::Command`).\n\n**Remediation:** Validate and sanitize all external inputs before passing them to external commands. Enforce strict boundary checks.\n**Effort:** Medium"
                                })
                except Exception:
                    pass
    if not untrusted_found:
        report += "No potential command injections found.\n\n"

    # 5. Git History Secrets
    report += "## 5. Secrets in Git History\n\n"
    try:
        process = subprocess.Popen(["git", "log", "-p"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        git_history_found = False
        for line in process.stdout:
            if secrets_regex.search(line):
                git_history_found = True
                report += f"### [High] Secrets present in previous commits\n"
                report += f"- **Description:** A historical audit (`git log -p`) reveals that secrets were introduced in earlier commits.\n"
                report += f"- **Remediation:** Use `git filter-repo` to scrub the secrets from the commit history.\n"
                report += f"- **Effort:** High\n\n"
                issues.append({
                    "title": "[High] Secrets present in previous commits",
                    "body": "A historical audit (`git log -p`) reveals that secrets were introduced in earlier commits.\n\n**Remediation:** Use `git filter-repo` to scrub the secrets from the commit history.\n**Effort:** High"
                })
                break
        process.stdout.close()
        process.wait()
    except Exception:
        pass

    if not git_history_found:
        report += "No secrets found in git history.\n\n"

    with open("security_report.md", "w") as f:
        f.write(report)

    print("Security report generated at security_report.md")

    # Create issues
    if os.environ.get("GITHUB_ACTIONS"):
        for issue in issues:
            title = issue["title"]
            body = issue["body"]
            res = subprocess.run(["gh", "issue", "list", "--state", "all", "--search", title], capture_output=True, text=True)
            if title not in res.stdout:
                subprocess.run(["gh", "issue", "create", "--title", title, "--body", body])
                print(f"Created issue: {title}")
    else:
        # Avoid creating mock issues during standard test runs to prevent repo pollution
        print(f"Would have created {len(issues)} issues in GitHub.")

if __name__ == "__main__":
    audit_codebase()
