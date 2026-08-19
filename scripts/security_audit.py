import json
import os
import re
import shlex
import subprocess
import urllib.request
import sys

def run_cmd(cmd, cwd=None):
    try:
        return subprocess.check_output(cmd, shell=True, text=True, cwd=cwd, stderr=subprocess.STDOUT)
    except subprocess.CalledProcessError as e:
        return e.output

def create_issue(title, body):
    if "GITHUB_ACTIONS" in os.environ:
        body_file = "issue_body.md"
        with open(body_file, "w") as f:
            f.write(body)

        # Safely quote the title for the shell
        safe_title = shlex.quote(title)

        # Check if issue already exists
        search_res = run_cmd(f'gh issue list --state all --search {safe_title}')
        if title not in search_res:
            print(f"Creating GitHub issue: {title}")
            run_cmd(f'gh issue create --title {safe_title} --body-file {body_file} --label "bug,agent-found"')
        if os.path.exists(body_file):
            os.remove(body_file)
    else:
        os.makedirs("issues", exist_ok=True)
        filename = f"issues/{title.replace(' ', '_').replace(':', '').replace('/', '_').lower()}.md"
        with open(filename, "w") as f:
            f.write(f"# {title}\n\n{body}")
        print(f"Created mock issue: {filename}")

def analyze_unsafe():
    findings = []
    unsafe_re = re.compile(r'unsafe\s*\{')
    for root, dirs, files in os.walk("mythrax-core/src"):
        for file in files:
            if not file.endswith(".rs"):
                continue
            path = os.path.join(root, file)
            with open(path, "r", encoding="utf-8") as f:
                lines = f.readlines()
                for i, line in enumerate(lines):
                    if unsafe_re.search(line):
                        # Check for safety comment
                        has_comment = False
                        for j in range(max(0, i-5), i):
                            if "// Safety:" in lines[j] or "// SAFETY:" in lines[j] or "/// Safety" in lines[j]:
                                has_comment = True
                                break

                        finding = {
                            "file": path,
                            "line": i + 1,
                            "has_comment": has_comment,
                            "context": line.strip()
                        }
                        findings.append(finding)
    return findings

def get_crates_commits(pkg_name):
    try:
        url = f"https://crates.io/api/v1/crates/{pkg_name}"
        req = urllib.request.Request(url, headers={'User-Agent': 'SecurityAudit/1.0'})
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode('utf-8'))
            repo_url = data.get('crate', {}).get('repository', '')
            if not repo_url:
                return False
            return True
    except Exception as e:
        return True

def install_cargo_audit():
    try:
        subprocess.check_call("cargo audit --version", shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except subprocess.CalledProcessError:
        print("Installing cargo-audit binary...")
        # Get latest cargo-audit release
        try:
            req = urllib.request.Request("https://api.github.com/repos/rustsec/rustsec/releases/latest", headers={'User-Agent': 'SecurityAudit/1.0'})
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read().decode('utf-8'))
                for asset in data.get("assets", []):
                    if "x86_64-unknown-linux-gnu" in asset["name"] and asset["name"].endswith(".tgz"):
                        url = asset["browser_download_url"]
                        print(f"Downloading from {url}")
                        subprocess.check_call(f"curl -LsSf {url} | tar zxf - -C /tmp --strip-components=1 && mv /tmp/cargo-audit ~/.cargo/bin/ || mv /tmp/cargo-audit /usr/local/bin/", shell=True)
                        return
        except Exception as e:
            print(f"Failed to download precompiled cargo-audit: {e}")
        print("Falling back to cargo install...")
        subprocess.check_call("cargo install cargo-audit --locked", shell=True)

def audit_dependencies():
    findings = []
    install_cargo_audit()
    print("Running cargo audit...")
    audit_output = run_cmd("cargo audit --json", cwd="mythrax-core")
    try:
        audit_data = json.loads(audit_output)

        vulnerabilities = audit_data.get("vulnerabilities", {}).get("list", [])
        for vuln in vulnerabilities:
            advisory = vuln.get("advisory", {})
            package = vuln.get("package", {})
            pkg_name = package.get("name", "Unknown")
            pkg_version = package.get("version", "Unknown")
            title = advisory.get("title", "Unknown vulnerability")
            desc = advisory.get("description", "No description")
            cvss = advisory.get("cvss", None)
            cvss_score = "Unknown"
            if isinstance(cvss, dict):
                cvss_score = cvss.get("score", {}).get("base", "Unknown")

            findings.append({
                "type": "Vulnerability",
                "package": pkg_name,
                "version": pkg_version,
                "title": title,
                "cvss": cvss_score,
                "description": desc
            })

        warnings = audit_data.get("warnings", {})
        for warning_type, warning_list in warnings.items():
            if not warning_list:
                continue
            for warning in warning_list:
                package = warning.get("package", {})
                pkg_name = package.get("name", "Unknown")
                pkg_version = package.get("version", "Unknown")
                kind = warning.get("kind", warning_type)

                findings.append({
                    "type": "Warning",
                    "package": pkg_name,
                    "version": pkg_version,
                    "kind": kind
                })
    except Exception as e:
        print(f"Error parsing cargo audit output: {e}")
        print(f"Audit output was: {audit_output[:500]}...")

    # Check for yanked/stale via crates.io script check
    try:
        with open("mythrax-core/Cargo.toml", "r") as f:
            content = f.read()
            deps_section = False
            for line in content.split("\n"):
                line = line.strip()
                if line.startswith("[dependencies]") or line.startswith("[dev-dependencies]"):
                    deps_section = True
                    continue
                if line.startswith("["):
                    deps_section = False

                if deps_section and "=" in line:
                    pkg_name = line.split("=")[0].strip()
                    if pkg_name.startswith("#"):
                        continue
                    if not get_crates_commits(pkg_name):
                        findings.append({
                            "type": "Stale Crate",
                            "package": pkg_name,
                            "reason": "Missing repository URL on crates.io, potentially stale/unmaintained"
                        })
    except Exception as e:
        pass

    return findings

def check_secrets():
    findings = []

    # Check current HEAD
    patterns = [
        r'api_key\s*=\s*["\'][A-Za-z0-9_\-]{16,}["\']',
        r'secret\s*=\s*["\'][A-Za-z0-9_\-]{16,}["\']',
        r'token\s*=\s*["\'][A-Za-z0-9_\-]{16,}["\']',
        r'sk-[a-zA-Z0-9]{20,}',
        r'ghp_[a-zA-Z0-9]{36}'
    ]

    for root, dirs, files in os.walk("mythrax-core"):
        if ".git" in root or "target" in root:
            continue
        for file in files:
            path = os.path.join(root, file)
            try:
                with open(path, "r", encoding="utf-8") as f:
                    content = f.read()
                    for pattern in patterns:
                        matches = re.finditer(pattern, content, re.IGNORECASE)
                        for match in matches:
                            line_num = content[:match.start()].count('\n') + 1
                            findings.append({
                                "file": path,
                                "line": line_num,
                                "match": match.group(0)[:10] + "..." # Don't leak full secret in report
                            })
            except Exception:
                pass

    # Check git history
    print("Checking git history for secrets...")
    for pattern in patterns:
        try:
            # -G looks for differences whose patch text contains added/removed lines that match
            output = run_cmd(f'git log -p -G"{pattern}"')
            if output and "commit " in output:
                # Extract file and commit
                commits = re.findall(r'commit ([a-f0-9]+)', output)
                if commits:
                    findings.append({
                        "file": "Git History",
                        "commit": commits[0],
                        "match": "Found secret matching pattern in git history"
                    })
        except Exception as e:
            pass

    return findings

def check_untrusted_input():
    findings = []
    # Very basic heuristic: looking for query params or env vars used directly in dangerous functions
    dangerous_patterns = [
        r'std::fs::read_to_string\(([^)]+)\)',
        r'std::fs::File::open\(([^)]+)\)',
        r'Command::new\(([^)]+)\)'
    ]

    for root, dirs, files in os.walk("mythrax-core/src"):
        for file in files:
            if not file.endswith(".rs"):
                continue
            path = os.path.join(root, file)
            try:
                with open(path, "r", encoding="utf-8") as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        for pattern in dangerous_patterns:
                            match = re.search(pattern, line)
                            if match:
                                arg = match.group(1)
                                # If it looks like a variable or direct env var
                                if "env::var" in arg or ("req" in arg and "query" in arg) or not arg.startswith('"'):
                                    findings.append({
                                        "file": path,
                                        "line": i + 1,
                                        "context": line.strip(),
                                        "reason": f"Potential untrusted input used in {pattern}"
                                    })
            except Exception:
                pass
    return findings

def generate_report(unsafe_findings, dep_findings, secret_findings, input_findings):
    report = "# Mythrax Security Advisory Report\n\n"

    # Critical - Secrets
    for secret in secret_findings:
        title = f"Critical: Hardcoded Secret found in {secret['file']}"
        body = f"**File:** {secret['file']}\n"
        if "line" in secret:
            body += f"**Line:** {secret['line']}\n"
        if "commit" in secret:
            body += f"**Commit:** {secret['commit']}\n"
        body += f"**Match:** `{secret['match']}`\n\n"
        body += "**Remediation:** Remove secret from source/history and rotate immediately.\n**Effort:** High (history rewrite/rotation)"
        report += f"## {title}\n{body}\n\n"
        create_issue(title, body)

    # Critical/High - Vulnerable Dependencies
    for dep in dep_findings:
        if dep['type'] == "Vulnerability":
            severity = "Critical" if str(dep['cvss']).startswith("9") or str(dep['cvss']).startswith("10") else "High"
            title = f"{severity}: CVE in {dep['package']} {dep['version']}"
            body = f"**Package:** {dep['package']} ({dep['version']})\n**CVE/Title:** {dep['title']}\n**CVSS:** {dep['cvss']}\n\n{dep['description']}\n\n**Remediation:** Update {dep['package']} to patched version.\n**Effort:** Low"
            report += f"## {title}\n{body}\n\n"
            if severity in ["Critical", "High"]:
                create_issue(title, body)
        else:
            title = f"Medium: {dep['type']} in {dep['package']}"
            body = f"**Package:** {dep['package']}\n**Reason:** {dep.get('kind', dep.get('reason', 'Unknown'))}\n\n**Remediation:** Review dependency usage.\n**Effort:** Medium"
            report += f"## {title}\n{body}\n\n"

    # Medium/High - Unsafe blocks
    for unsafe in unsafe_findings:
        severity = "High" if not unsafe['has_comment'] else "Medium"
        title = f"{severity}: Unsafe block in {unsafe['file']}:{unsafe['line']}"
        body = f"**File:** {unsafe['file']}:{unsafe['line']}\n**Context:** `{unsafe['context']}`\n"
        if not unsafe['has_comment']:
            body += "**Warning:** Missing `// Safety:` documented justification.\n"
        body += "\n**Memory Safety Risk:** Usage of `unsafe` bypasses Rust's borrow checker. Improper usage can lead to memory corruption, use-after-free, or data races.\n"
        body += "**Remediation:** Document safety guarantees or rewrite in safe Rust.\n**Effort:** Medium"
        report += f"## {title}\n{body}\n\n"
        if severity == "High":
             create_issue(title, body)

    # Medium - Untrusted Input
    for input_finding in input_findings:
        title = f"Medium: Potential Untrusted Input in {input_finding['file']}:{input_finding['line']}"
        body = f"**File:** {input_finding['file']}:{input_finding['line']}\n**Context:** `{input_finding['context']}`\n**Reason:** {input_finding['reason']}\n\n**Remediation:** Sanitize input and validate boundaries before usage.\n**Effort:** Medium"
        report += f"## {title}\n{body}\n\n"

    with open("report.md", "w") as f:
        f.write(report)
    print("Report generated: report.md")

def main():
    print("Starting security audit...")
    unsafe_findings = analyze_unsafe()
    print(f"Found {len(unsafe_findings)} unsafe blocks")

    dep_findings = audit_dependencies()
    print(f"Found {len(dep_findings)} dependency issues")

    secret_findings = check_secrets()
    print(f"Found {len(secret_findings)} secret issues")

    input_findings = check_untrusted_input()
    print(f"Found {len(input_findings)} untrusted input issues")

    generate_report(unsafe_findings, dep_findings, secret_findings, input_findings)
    print("Audit complete.")

if __name__ == '__main__':
    main()
