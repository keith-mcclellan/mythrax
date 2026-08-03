import os
import re
import json
import subprocess

REPORT_FILE = "security_advisory_report.md"
ISSUES = []

def log_issue(title, severity, description, remediation, effort):
    ISSUES.append({
        "title": title,
        "severity": severity,
        "description": description,
        "remediation": remediation,
        "effort": effort
    })

def scan_hardcoded_secrets():
    print("Scanning for hardcoded secrets...")
    pattern = re.compile(r"(?i)(token|api_key|secret|password|credential)\s*(:|=>|=)\s*['\"]([^'\"]+)['\"]")

    for root, dirs, files in os.walk("."):
        if "target" in root or ".git" in root or ".venv" in root or "node_modules" in root or "issues" in root:
            continue
        for file in files:
            if file.endswith(".md") and file != "Cargo.toml":
                continue
            if file == "ciso_audit.py":
                continue

            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8") as f:
                    for i, line in enumerate(f):
                        if pattern.search(line):
                            log_issue(
                                title=f"Hardcoded secret in {filepath}",
                                severity="Critical",
                                description=f"Found hardcoded credential pattern in `{filepath}` at line {i+1}: `{line.strip()}`",
                                remediation="Remove hardcoded secret, load from environment variables or secure vault.",
                                effort="Medium"
                            )
            except Exception:
                pass

def scan_unsafe_rust():
    print("Scanning for unsafe Rust...")
    for root, dirs, files in os.walk("."):
        if "target" in root or ".git" in root or ".venv" in root:
            continue
        for file in files:
            if not file.endswith(".rs"): continue
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8") as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if "unsafe {" in line or "unsafe impl" in line or "unsafe fn" in line:
                            has_safety = False
                            for j in range(max(0, i-5), i):
                                if "SAFETY:" in lines[j].upper() or "SAFETY " in lines[j].upper():
                                    has_safety = True
                                    break

                            severity = "Low" if has_safety else "High"
                            desc = f"Found unsafe Rust block at `{filepath}:{i+1}`. Memory safety risk: Unsafe code bypasses Rust's compiler guarantees, potentially leading to memory corruption, use-after-free, or data races.\n`{line.strip()}`"
                            if not has_safety:
                                desc += "\n**Flag:** Lacks documented `SAFETY:` justification comment."

                            log_issue(
                                title=f"Unsafe Rust block in {filepath} (Line {i+1})",
                                severity=severity,
                                description=desc,
                                remediation="Add a `// SAFETY: ...` comment explaining why this unsafe code is sound." if not has_safety else "Ensure the safety contract is maintained.",
                                effort="Low" if not has_safety else "Low"
                            )
            except Exception:
                pass

def scan_vulnerable_deps():
    print("Running cargo audit...")
    try:
        if os.path.exists("mythrax-core"):
            cwd = "mythrax-core"
        else:
            cwd = "."

        result = subprocess.run(
            ["cargo", "audit", "--json"],
            cwd=cwd,
            capture_output=True,
            text=True
        )
        if not result.stdout:
            print("Warning: cargo audit returned empty stdout")
            return

        data = json.loads(result.stdout)

        for vuln in data.get("vulnerabilities", {}).get("list", []):
            crate_name = vuln.get("package", {}).get("name", "unknown")
            version = vuln.get("package", {}).get("version", "unknown")
            advisory = vuln.get("advisory", {})
            title = advisory.get("title", "Unknown vulnerability")
            date = advisory.get("date", "Unknown date")

            log_issue(
                title=f"Vulnerable dependency: {crate_name} {version}",
                severity="Critical",
                description=f"{title} (Published: {date})",
                remediation=f"Update `{crate_name}` to a secure version.",
                effort="Low"
            )

        for warning in data.get("warnings", {}).get("list", []):
            kind = warning.get("kind", "unknown")
            crate_name = warning.get("package", {}).get("name", "unknown")
            version = warning.get("package", {}).get("version", "unknown")
            if kind == "unmaintained":
                log_issue(
                    title=f"Unmaintained dependency: {crate_name} {version}",
                    severity="Medium",
                    description=f"Crate `{crate_name}` is unmaintained and has no recent commits.",
                    remediation=f"Migrate away from `{crate_name}` or take ownership.",
                    effort="High"
                )
            elif kind == "yanked":
                log_issue(
                    title=f"Yanked dependency: {crate_name} {version}",
                    severity="Medium",
                    description=f"Crate `{crate_name}` version {version} is yanked.",
                    remediation=f"Update `{crate_name}` to a non-yanked version.",
                    effort="Low"
                )
    except Exception as e:
        print(f"Failed to run cargo audit: {e}")

def scan_external_input():
    print("Scanning for untrusted external inputs...")
    arg_pattern = re.compile(r"\.(arg|args)\s*\(\s*(.*?)\s*\)")
    for root, dirs, files in os.walk("."):
        if "target" in root or ".git" in root or ".venv" in root:
            continue
        for file in files:
            if not file.endswith(".rs"): continue
            filepath = os.path.join(root, file)
            try:
                with open(filepath, "r", encoding="utf-8") as f:
                    content = f.read()
                    lines = content.split('\n')
                    for i, line in enumerate(lines):
                        if "std::process::Command::new" in line or ".arg(" in line or ".args(" in line:
                            match = arg_pattern.search(line)
                            if match:
                                arg_val = match.group(2).strip()
                                if arg_val and not arg_val.startswith('"') and not arg_val.startswith('&"'):
                                    log_issue(
                                        title=f"Potential command injection in {filepath}",
                                        severity="High",
                                        description=f"Found usage of `std::process::Command` passing variable `{arg_val}` as argument at line {i+1}. This external input may be unsanitized.",
                                        remediation="Ensure arguments are safely sanitized string literals or enforce strict boundary checks.",
                                        effort="Medium"
                                    )
            except Exception:
                pass

def scan_git_secrets():
    print("Scanning git history for secrets...")
    pattern = r"(token|api_key|secret|password|credential)\s*(:|=>|=)\s*['\"][^'\"]+['\"]"
    try:
        # Note: the memory mentions to use git log -G with -P for Perl-compatible
        # So we use -E or -P as recommended. Also we need to use a regex supported by it, so removing (?i) which isn't supported in standard ERE
        # Wait, memory says: "When using git log -G to scan git history for complex regular expressions (such as those using PCRE features like (?i) or \s), explicitly include the -P (Perl-compatible) or -E (Extended) flag to ensure the pattern is evaluated correctly."
        # so -P should work with (?i), but maybe we just avoid (?i) for git log -G since the review failed.
        # Let's use `git log -G "(?i)(token|api_key|secret|password|credential)\s*(:|=>|=)\s*['\"][^'\"]+['\"]" -P` but wait, review said "The git log -G command includes a -P flag, which is an unrecognized argument for git log".
        # But wait, git log doesn't have a -P flag, `git log -G` takes a string. Memory might have meant `git log --grep`? No.
        # wait! `git log --grep="pattern" -P` exists. `git log -G "pattern"` takes a pattern.
        # Actually, let's just use git log -S or git log -p and grep, but wait: `git log -S "token=" -p` is an option, but `git grep` is better.
        # But `git log -p -S` is easier to parse, but the review says "The git log -G command includes a -P flag, which is an unrecognized argument for git log and will cause the command to fail outright...".
        # Oh, `git log -G` takes a basic regular expression.
        # Instead, let's use a simpler pattern for `git log -G`: "token=|api_key=|secret=|password="
        # Let's just do `git log -p --all` and pipe to python, or `git grep`? git grep only searches current tree.
        # To search history: `git log -p` and python does the regex matching. That is 100% safe.

        result = subprocess.run(
            ["git", "log", "-p", "--all"],
            capture_output=True,
            text=True,
            errors='ignore'
        )
        commits_found = set()
        current_commit = ""
        re_pattern = re.compile(r"(?i)(token|api_key|secret|password|credential)\s*(:|=>|=)\s*['\"]([^'\"]+)['\"]")
        for line in result.stdout.split('\n'):
            if line.startswith('commit '):
                current_commit = line.split()[1]
            elif line.startswith('+') and not line.startswith('+++'):
                if re_pattern.search(line):
                    commits_found.add(current_commit)

        if commits_found:
            log_issue(
                title="Secrets found in git history",
                severity="Critical",
                description=f"Found potential secrets committed in git history. Commits:\n```\n" + "\n".join(list(commits_found)[:10]) + "\n```",
                remediation="Rewrite git history to remove secrets using BFG Repo-Cleaner or `git filter-repo`. Rotate all exposed credentials immediately.",
                effort="High"
            )
    except Exception as e:
        print(f"Failed to scan git history: {e}")

def create_github_issues():
    token = os.environ.get("GH_TOKEN")
    if token:
        print("Filing real GitHub Issues using gh API or CLI...")
        repo = os.environ.get("GITHUB_REPOSITORY", "keith-mcclellan/mythrax")

        # Check existing issues to avoid spamming
        existing_issues = []
        try:
            res = subprocess.run(["gh", "issue", "list", "--repo", repo, "--state", "all", "--json", "title"], capture_output=True, text=True)
            if res.returncode == 0:
                issues_json = json.loads(res.stdout)
                existing_issues = [issue.get("title") for issue in issues_json]
        except Exception as e:
            print(f"Could not list existing issues: {e}")

        import urllib.request
        for issue in ISSUES:
            if issue["severity"] in ["Critical", "High"]:
                if issue["title"] in existing_issues:
                    print(f"Issue already exists: {issue['title']}, skipping.")
                    continue

                url = f"https://api.github.com/repos/{repo}/issues"
                headers = {
                    "Authorization": f"token {token}",
                    "Accept": "application/vnd.github.v3+json"
                }
                body_content = (
                    f"**Severity:** {issue['severity']}\n\n"
                    f"**Description:**\n{issue['description']}\n\n"
                    f"**Remediation:**\n{issue['remediation']}\n\n"
                    f"**Estimated Effort:** {issue['effort']}"
                )
                data = json.dumps({"title": issue["title"], "body": body_content}).encode("utf-8")
                req = urllib.request.Request(url, data=data, headers=headers)
                try:
                    urllib.request.urlopen(req)
                    print(f"Filed issue: {issue['title']}")
                except Exception as e:
                    try:
                        subprocess.run(["gh", "issue", "create", "--repo", repo, "--title", issue["title"], "--body", body_content], check=True)
                        print(f"Filed issue with gh cli: {issue['title']}")
                    except Exception as e2:
                        print(f"Failed to file issue {issue['title']}: {e} / {e2}")
    else:
        print("No GH_TOKEN found. Generating mock issues in issues/ directory...")
        os.makedirs("issues", exist_ok=True)
        for f in os.listdir("issues"):
            if f.endswith(".md"):
                os.remove(os.path.join("issues", f))

        idx = 1
        for issue in ISSUES:
            if issue["severity"] in ["Critical", "High"]:
                with open(f"issues/issue_{idx}.md", "w", encoding="utf-8") as f:
                    f.write(f"# {issue['title']}\n\n")
                    f.write(f"**Severity:** {issue['severity']}\n\n")
                    f.write(f"**Description:** {issue['description']}\n\n")
                    f.write(f"**Remediation:** {issue['remediation']}\n\n")
                    f.write(f"**Estimated Effort:** {issue['effort']}\n")
                idx += 1

def generate_report():
    print("Generating report...")
    with open(REPORT_FILE, "w", encoding="utf-8") as f:
        f.write("# Security Advisory Report\n\n")
        if not ISSUES:
            f.write("No issues found.\n")
            return

        for severity in ["Critical", "High", "Medium", "Low"]:
            f.write(f"## {severity} Findings\n")
            found = False
            for issue in ISSUES:
                if issue["severity"] == severity:
                    found = True
                    f.write(f"### {issue['title']}\n")
                    f.write(f"- **Description:** {issue['description']}\n")
                    f.write(f"- **Remediation:** {issue['remediation']}\n")
                    f.write(f"- **Estimated Effort:** {issue['effort']}\n\n")
            if not found:
                f.write("None\n\n")

def main():
    scan_hardcoded_secrets()
    scan_unsafe_rust()
    scan_vulnerable_deps()
    scan_external_input()
    scan_git_secrets()

    generate_report()
    create_github_issues()

    print("Audit complete.")

if __name__ == "__main__":
    main()
