#!/usr/bin/env python3
import json
import os
import re
import subprocess
import datetime
import urllib.request
from pathlib import Path
from datetime import datetime, timezone

# Deduplication cache
def issue_exists(title):
    gh_token = os.environ.get("GH_TOKEN")
    if not gh_token:
        # Check local issues directory
        issue_path = f"issues/{title.replace(' ', '_').replace(':', '')}.md"
        return os.path.exists(issue_path)

    url = "https://api.github.com/repos/keith-mcclellan/mythrax/issues?state=open"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {gh_token}",
        "X-GitHub-Api-Version": "2022-11-28"
    }

    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode('utf-8'))
            for issue in issues:
                if issue.get("title") == title:
                    return True
    except Exception as e:
        print(f"Failed to check if issue exists: {e}")
    return False

def file_github_issue(title, body, severity):
    if severity not in ["Critical", "High"]:
        return

    if issue_exists(title):
        print(f"Issue already exists: {title}")
        return

    gh_token = os.environ.get("GH_TOKEN")
    if not gh_token:
        os.makedirs("issues", exist_ok=True)
        issue_path = f"issues/{title.replace(' ', '_').replace(':', '')}.md"
        with open(issue_path, 'w') as f:
            f.write(f"# {title}\n\n{body}")
        print(f"Filed issue locally at {issue_path}")
        return

    url = "https://api.github.com/repos/keith-mcclellan/mythrax/issues"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {gh_token}",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    data = {
        "title": title,
        "body": body,
        "labels": ["security", "audit-finding"]
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Successfully filed issue: {title}")
    except Exception as e:
        print(f"Failed to file issue {title}: {e}")
        os.makedirs("issues", exist_ok=True)
        issue_path = f"issues/{title.replace(' ', '_').replace(':', '')}.md"
        with open(issue_path, 'w') as f:
            f.write(f"# {title}\n\n{body}")

def find_hardcoded_secrets():
    report_text = "## Secrets Scan Results\n"
    res = subprocess.run(["git", "log", "-p", "-E", "-i", "-G", "(password|secret|key|token)[[:space:]]*=[[:space:]]*[^ ]+"], capture_output=True, text=True)
    if res.stdout:
        body = "### Hardcoded Secrets Found\n\n```\n" + res.stdout[:5000] + "\n```\n\n**Recommendation:** Revoke these secrets immediately and remove them from git history using a tool like BFG Repo-Cleaner or git-filter-repo.\n**Severity:** Critical\n**Effort:** Medium"
        report_text += body + "\n"
        file_github_issue("Security Audit: Hardcoded Secrets Found", body, "Critical")
    else:
        report_text += "No hardcoded secrets found in git history.\n"
    return report_text

def find_unsafe_blocks():
    report_text = "## Unsafe Rust Blocks\n"
    res = subprocess.run(["grep", "-Rn", "-B2", "-A2", "unsafe {", "mythrax-core/"], capture_output=True, text=True)
    body = "### Unsafe Rust Blocks Found\n\n"
    found = False

    blocks = res.stdout.split('--')
    for block in blocks:
        if "unsafe {" in block and "test" not in block and "scratch" not in block and "bench" not in block:
            lines = block.split('\n')
            has_comment = any("SAFETY:" in l or "safety:" in l.lower() for l in lines)
            if not has_comment:
                found = True
                body += f"```rust\n{block}\n```\n\n"

    if found:
        body += "**Recommendation:** Add documented safety justifications (e.g. `// SAFETY: ...`) to all `unsafe {}` blocks to explain the memory safety context, or refactor to safe Rust.\n**Severity:** High\n**Effort:** Medium"
        report_text += body + "\n"
        file_github_issue("Security Audit: Unjustified Unsafe Rust Blocks", body, "High")
    else:
        report_text += "No unjustified unsafe Rust blocks found.\n"
    return report_text

def check_crates_io(crate_name):
    url = f"https://crates.io/api/v1/crates/{crate_name}"
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mythrax-Security-Auditor/1.0'})
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode('utf-8'))
            crate_info = data.get('crate', {})
            versions = data.get('versions', [])

            # Check for yanked
            yanked = any(v.get('yanked', False) for v in versions if v.get('num') == crate_info.get('max_version'))

            # Check staleness
            stale = False
            if versions:
                latest = versions[0]
                updated_at_str = latest.get('updated_at', '')
                if updated_at_str:
                    try:
                        updated_at = datetime.fromisoformat(updated_at_str.replace('Z', '+00:00'))
                        now = datetime.now(timezone.utc)
                        if (now - updated_at).days > 365:
                            stale = True
                    except Exception:
                        pass

            return yanked, stale
    except Exception as e:
        print(f"Error querying crates.io for {crate_name}: {e}")
        return False, False

def audit_dependencies():
    report_text = "## Dependency Audit\n"
    try:
        res = subprocess.run(["cargo", "audit"], cwd="mythrax-core", capture_output=True, text=True)
        if res.returncode != 0 and res.stdout.strip():
            body = "### Known CVEs Found\n\n```\n" + res.stdout + "\n```\n\n**Recommendation:** Update the vulnerable dependencies by running `cargo update` or modifying `Cargo.toml`.\n**Severity:** Critical\n**Effort:** Low"
            report_text += body + "\n"
            file_github_issue("Security Audit: Dependency CVEs Found", body, "Critical")
        else:
             report_text += "No known CVEs found.\n"
    except FileNotFoundError:
        report_text += "cargo-audit not installed. Please install.\n"

    # Use metadata to fetch all dependencies including transitive
    res = subprocess.run(["cargo", "metadata", "--format-version", "1"], cwd="mythrax-core", capture_output=True, text=True)
    if res.returncode == 0:
        metadata = json.loads(res.stdout)
        packages = metadata.get("packages", [])

        stale_crates = []
        yanked_crates = []
        for pkg in packages:
            # We don't want to check our own workspace packages on crates.io
            if "mythrax" in pkg.get("name", ""):
                continue
            name = pkg.get("name")
            yanked, stale = check_crates_io(name)
            if yanked:
                yanked_crates.append(name)
            if stale:
                stale_crates.append(name)

        if yanked_crates or stale_crates:
            body = "### Stale or Yanked Dependencies Found\n\n"
            if yanked_crates:
                body += f"**Yanked Crates:** {', '.join(yanked_crates)}\n"
            if stale_crates:
                body += f"**Stale Crates (>1 year without commits/releases):** {', '.join(stale_crates)}\n"

            body += "\n**Recommendation:** Replace or update these crates, as they may no longer receive security updates.\n**Severity:** Medium\n**Effort:** Medium"
            report_text += body + "\n"
            file_github_issue("Security Audit: Stale or Yanked Dependencies", body, "Medium")
        else:
             report_text += "No stale or yanked dependencies found.\n"
    return report_text

def find_untrusted_inputs():
    report_text = "## Untrusted Input Execution Paths\n"
    # Looking for routes accepting untrusted input
    res = subprocess.run(["grep", "-Rn", "-E", "(Command::new|fs::read_to_string|fs::File::open|reqwest::get|axum::extract::Query|axum::extract::Json)", "mythrax-core/src/"], capture_output=True, text=True)
    if res.stdout:
         body = "### Potential Untrusted Input Execution Paths\n\n"
         body += "The following locations accept external inputs (HTTP queries, body payloads, file reads, or shell commands). Please manually verify these execution paths enforce boundary checks and input sanitization to prevent SSRF, Path Traversal, or Command Injection.\n\n"
         body += "```\n" + res.stdout[:5000] + "\n```\n\n"
         body += "**Recommendation:** Ensure input validation (e.g., using `validator` crate) and boundary enforcement before processing these inputs.\n**Severity:** High\n**Effort:** High"
         report_text += body + "\n"
         file_github_issue("Security Audit: Untrusted Input Execution Paths", body, "High")
    else:
        report_text += "No untrusted input paths detected.\n"
    return report_text

if __name__ == "__main__":
    full_report = "# Security Advisory Report\n\n"
    full_report += find_hardcoded_secrets() + "\n"
    full_report += find_unsafe_blocks() + "\n"
    full_report += audit_dependencies() + "\n"
    full_report += find_untrusted_inputs() + "\n"

    print(full_report)
