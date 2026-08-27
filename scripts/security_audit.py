#!/usr/bin/env python3
import subprocess
import re
import os
import urllib.request
import json
from datetime import datetime, timezone
import sys

def fetch_crates_io_info(crate_name):
    url = f"https://crates.io/api/v1/crates/{crate_name}"
    req = urllib.request.Request(url, headers={'User-Agent': 'MythraxSecurityAudit/1.0'})
    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read().decode())
    except Exception as e:
        print(f"Error fetching {crate_name}: {e}")
        return None

def file_github_issue(title, body):
    token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print(f"Skipping issue creation (no GH_TOKEN/GITHUB_REPOSITORY): {title}")
        return
    url = f"https://api.github.com/repos/{repo}/issues"
    req = urllib.request.Request(url, method="POST")
    req.add_header("Authorization", f"Bearer {token}")
    req.add_header("Accept", "application/vnd.github+json")
    req.add_header("X-GitHub-Api-Version", "2022-11-28")
    data = json.dumps({"title": title, "body": body}).encode("utf-8")
    try:
        with urllib.request.urlopen(req, data=data) as response:
            res = json.loads(response.read().decode())
            print(f"Created issue: {res.get('html_url')}")
    except Exception as e:
        print(f"Failed to create issue {title}: {e}")

def main():
    report = "# Mythrax Security Advisory Report\n\n"
    issues_to_file = []

    # 1. Hardcoded Secrets
    report += "## 1. Hardcoded Secrets\n\n"
    hardcoded_keys = []
    try:
        output = subprocess.check_output(
            ["grep", "-rnE", "(password|secret|token|api_key|key|cred)[A-Za-z0-9_]*[[:space:]]*=[[:space:]]*[\"'][A-Za-z0-9_\\-]+[\"']", "mythrax-core/src/"],
            text=True
        )
        for line in output.splitlines():
            if line.startswith("mythrax-core/src/"):
                line = line[len("mythrax-core/src/"):]
            hardcoded_keys.append(f"- `mythrax-core/src/{line}`")
    except subprocess.CalledProcessError:
        pass

    try:
         output = subprocess.check_output(
            ["grep", "-rnE", "(X-Mythrax-Token|secret-token)", "mythrax-core/src/"],
            text=True
         )
         for line in output.splitlines():
            if "secret-token" in line:
                if line.startswith("mythrax-core/src/"):
                    line = line[len("mythrax-core/src/"):]
                hardcoded_keys.append(f"- `mythrax-core/src/{line}`")
    except subprocess.CalledProcessError:
        pass

    if hardcoded_keys:
        report += "**Severity**: Critical\n"
        report += "**Findings**:\n" + "\n".join(hardcoded_keys) + "\n\n"
        report += "**Recommendation**: Use environment variables or secure vault storage for secrets instead of hardcoding them in source.\n"
        report += "**Effort**: Medium\n\n"
        body = "**Severity**: Critical\n\n**Findings**:\n" + "\n".join(hardcoded_keys) + "\n\n**Recommendation**: Use environment variables or secure vault storage for secrets instead of hardcoding them in source.\n"
        issues_to_file.append(("Security: Hardcoded Secrets Found", body))

    # 2. Unsafe Rust Blocks
    report += "## 2. Unsafe Rust Blocks\n\n"
    unsafe_blocks = []
    try:
        output = subprocess.check_output(
            ["grep", "-rnE", "unsafe[[:space:]]*{", "mythrax-core/src/"],
            text=True
        )
        for line in output.splitlines():
            unsafe_blocks.append(f"- `{line}`")
    except subprocess.CalledProcessError:
        pass

    if unsafe_blocks:
        report += "**Severity**: High\n"
        report += "**Findings**:\n" + "\n".join(unsafe_blocks) + "\n\n"
        report += "**Risk**: `unsafe` blocks bypass Rust's memory safety guarantees and can lead to memory corruption, undefined behavior, or exploitable vulnerabilities.\n"
        report += "**Recommendation**: Document justifications for all `unsafe` blocks. Ensure proper safety invariants are upheld or rewrite using safe abstractions if possible.\n"
        report += "**Effort**: High\n\n"
        body = "**Severity**: High\n\n**Findings**:\n" + "\n".join(unsafe_blocks) + "\n\n**Risk**: `unsafe` blocks bypass Rust's memory safety guarantees and can lead to memory corruption, undefined behavior, or exploitable vulnerabilities.\n**Recommendation**: Document justifications for all `unsafe` blocks. Ensure proper safety invariants are upheld or rewrite using safe abstractions if possible.\n"
        issues_to_file.append(("Security: Unsafe Rust Blocks Detected", body))

    # 3. Dependency CVEs and Stale Crates
    report += "## 3. Dependency Vulnerabilities and Stale/Yanked Crates\n\n"
    deps_issues = []

    try:
        subprocess.check_output(["cargo", "audit"], cwd="mythrax-core", text=True)
    except subprocess.CalledProcessError as e:
        audit_output = e.output if e.output else ""
        if hasattr(e, 'stdout') and e.stdout:
            audit_output += e.stdout if isinstance(e.stdout, str) else e.stdout.decode('utf-8', errors='replace')
        for match in re.finditer(r"Crate:\s+(.+?)\nVersion:\s+(.+?)\nTitle:\s+(.+?)\nDate:\s+(.+?)\nID:\s+(.+?)\nURL:\s+(.+?)\n(Severity:\s+(.+?)\n)?Solution:\s+(.+?)\n", audit_output):
            crate = match.group(1).strip()
            version = match.group(2).strip()
            title = match.group(3).strip()
            id = match.group(5).strip()
            severity = match.group(8).strip() if match.group(8) else "Unknown"
            deps_issues.append(f"- **{crate} ({version})**: {title} [{id}]. Severity: {severity}")

    with open("mythrax-core/Cargo.toml") as f:
        content = f.read()

    in_deps = False
    for line in content.splitlines():
        if line.strip() == "[dependencies]" or line.strip() == "[dev-dependencies]":
            in_deps = True
            continue
        if in_deps and line.startswith("["):
            in_deps = False

        if in_deps and "=" in line:
            crate = line.split("=")[0].strip()
            crate = crate.replace('"', '')
            if crate in ['default', 'mlx', 'bench', 'path']:
                continue

            info = fetch_crates_io_info(crate)
            if not info:
                continue

            yanked = [v['num'] for v in info.get('versions', []) if v['yanked']]
            if yanked:
                deps_issues.append(f"- **{crate}**: Has yanked versions ({', '.join(str(y) for y in yanked)})")

            latest = info.get('crate', {}).get('updated_at')
            if latest:
                updated_at = datetime.strptime(latest.split('.')[0] + "Z", "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
                if (datetime.now(timezone.utc) - updated_at).days > 365:
                    deps_issues.append(f"- **{crate}**: Is stale. Last updated: {updated_at.strftime('%Y-%m-%d')}")

    if deps_issues:
        report += "**Severity**: High\n"
        report += "**Findings**:\n" + "\n".join(deps_issues) + "\n\n"
        report += "**Recommendation**: Update vulnerable and yanked crates. Migrate away from stale or unmaintained crates.\n"
        report += "**Effort**: High\n\n"
        body = "**Severity**: High\n\n**Findings**:\n" + "\n".join(deps_issues) + "\n\n**Recommendation**: Update vulnerable and yanked crates. Migrate away from stale or unmaintained crates.\n"
        issues_to_file.append(("Security: Vulnerable, Stale, or Yanked Dependencies", body))

    # 4. Unsafe Execution Paths (Command Injection Risk)
    report += "## 4. Unsafe Execution Paths (Command Injection Risk)\n\n"
    cmd_issues = []
    try:
        output = subprocess.check_output(
            ["grep", "-rn", "std::process::Command", "mythrax-core/src/"],
            text=True
        )
        for line in output.splitlines():
            cmd_issues.append(f"- `{line}`")
    except subprocess.CalledProcessError:
        pass

    if cmd_issues:
        report += "**Severity**: Critical\n"
        report += "**Findings**:\n" + "\n".join(cmd_issues) + "\n\n"
        report += "**Risk**: Spawning processes with external input can lead to command injection if inputs are not strictly sanitized.\n"
        report += "**Recommendation**: Review all `Command::new` usages. Ensure inputs are sanitized, boundary-enforced, or use structured API alternatives over shell execution.\n"
        report += "**Effort**: High\n\n"
        body = "**Severity**: Critical\n\n**Findings**:\n" + "\n".join(cmd_issues) + "\n\n**Risk**: Spawning processes with external input can lead to command injection if inputs are not strictly sanitized.\n**Recommendation**: Review all `Command::new` usages. Ensure inputs are sanitized, boundary-enforced, or use structured API alternatives over shell execution.\n"
        issues_to_file.append(("Security: Unsafe Execution Paths Detected", body))

    # 5. Git History Secrets
    report += "## 5. Secrets in Git History\n\n"
    git_secrets = []
    try:
        subprocess.run(["git", "fetch", "--unshallow"], check=False, capture_output=True)
        subprocess.run(["git", "fetch", "--all"], check=False, capture_output=True)

        output = subprocess.check_output(
            ["git", "log", "-p", "-E", "-i", "-G", "(password|token|secret|api_key|key|cred)[ \t]*=[ \t]*[\"'][A-Za-z0-9_\\-]+[\"']"],
            text=True
        )

        current_commit = ""
        for line in output.splitlines():
            if line.startswith("commit "):
                current_commit = line.split()[1]
            elif line.startswith("+") and not line.startswith("+++"):
                if re.search(r"(password|token|secret|api_key|key|cred)[ \t]*=[ \t]*[\"'][A-Za-z0-9_\-]+[\"']", line, re.IGNORECASE):
                    git_secrets.append(f"- Commit `{current_commit}`: {line.strip()}")
    except subprocess.CalledProcessError:
        pass

    if git_secrets:
        report += "**Severity**: Critical\n"
        report += "**Findings**:\n" + "\n".join(git_secrets) + "\n\n"
        report += "**Risk**: Secrets committed to git history are permanently exposed even if removed in later commits.\n"
        report += "**Recommendation**: Rotate compromised secrets immediately. Use tools like `BFG Repo-Cleaner` or `git filter-repo` to scrub history.\n"
        report += "**Effort**: High\n\n"
        body = "**Severity**: Critical\n\n**Findings**:\n" + "\n".join(git_secrets) + "\n\n**Risk**: Secrets committed to git history are permanently exposed even if removed in later commits.\n**Recommendation**: Rotate compromised secrets immediately. Use tools like `BFG Repo-Cleaner` or `git filter-repo` to scrub history.\n"
        issues_to_file.append(("Security: Secrets Found in Git History", body))

    with open("security_advisory_report.md", "w") as f:
        f.write(report)
    print(report)

    # File issues
    for title, body in issues_to_file:
        file_github_issue(title, body)

    if deps_issues or cmd_issues or git_secrets or hardcoded_keys or unsafe_blocks:
        sys.exit(1)

if __name__ == "__main__":
    main()
