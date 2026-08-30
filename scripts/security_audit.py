import urllib.request
import json
import datetime
import os
import subprocess
import re

def create_issue(title, body):
    token = os.environ.get('GH_TOKEN')
    print(f"DEBUG GH_TOKEN present: {bool(token)}")
    if not token:
        # According to memory: "When tasked with generating mock GitHub issues... generate them as untracked files in the issues/ directory instead"
        os.makedirs("issues", exist_ok=True)
        filename = f"issues/{title.replace(' ', '_').replace(':', '').lower()}.md"
        with open(filename, "w") as f:
            f.write(f"# {title}\n\n{body}")
        print(f"Created mock issue: {filename}")
        return

    url = "https://api.github.com/repos/keith-mcclellan/mythrax/issues"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    data = json.dumps({"title": title, "body": body}).encode('utf-8')
    try:
        req = urllib.request.Request(url, data=data, headers=headers, method='POST')
        response = urllib.request.urlopen(req)
        print(f"Created issue: {title}")
    except Exception as e:
        print(f"Failed to create issue: {e}")

def check_stale_crates(cargo_toml_path="mythrax-core/Cargo.toml"):
    with open(cargo_toml_path, "r") as f:
        content = f.read()

    deps = []
    in_deps = False
    for line in content.splitlines():
        if line.startswith("[dependencies]") or line.startswith("[dev-dependencies]"):
            in_deps = True
            continue
        elif line.startswith("[") and in_deps:
            in_deps = False

        if in_deps and "=" in line:
            parts = line.split("=")
            dep_name = parts[0].strip()
            if dep_name and not dep_name.startswith("#"):
                deps.append(dep_name)

    stale_crates = []
    yanked_crates = []

    for dep in deps:
        try:
            url = f"https://crates.io/api/v1/crates/{dep}"
            req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
            response = urllib.request.urlopen(req)
            data = json.loads(response.read().decode('utf-8'))

            versions = data.get("versions", [])
            if not versions:
                continue

            latest = versions[0]
            if latest.get("yanked", False):
                yanked_crates.append(dep)

            created_at = latest.get("created_at")
            if created_at:
                dt = datetime.datetime.fromisoformat(created_at.replace("Z", "+00:00"))
                now = datetime.datetime.now(datetime.timezone.utc)
                diff = now - dt
                if diff.days > 365:
                    stale_crates.append(dep)
        except Exception as e:
            pass

    return stale_crates, yanked_crates

def check_unsafe():
    findings = []
    for root, _, files in os.walk("mythrax-core/src"):
        for file in files:
            if file.endswith(".rs"):
                path = os.path.join(root, file)
                with open(path, "r") as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if "unsafe {" in line:
                            has_comment = False
                            if i > 0 and (lines[i-1].strip().startswith("//") or lines[i-1].strip().startswith("/*")):
                                has_comment = True

                            findings.append((path, i+1, line.strip(), has_comment))
    return findings

def check_secrets_history():
    findings = []
    try:
        output = subprocess.check_output(
            ["git", "log", "-p", "-E", "-i", "-G", r'(password|token|api_key|secret)[[:space:]]*=[[:space:]]*[^ ]+'],
            stderr=subprocess.STDOUT
        ).decode('utf-8', errors='replace')

        lines = output.splitlines()
        commit = ""
        for line in lines:
            if line.startswith("commit "):
                commit = line.split()[1]
            elif line.startswith("+") and not line.startswith("+++"):
                if re.search(r'(password|token|api_key|secret)\s*=\s*[^ ]+', line, re.IGNORECASE):
                    findings.append((commit, line))
    except Exception as e:
        print(f"Error checking git history: {e}")
    return findings

def check_cargo_audit():
    findings = []
    try:
        subprocess.run(["cargo", "audit", "--json"], cwd="mythrax-core", capture_output=True, check=True)
    except subprocess.CalledProcessError as e:
        if e.returncode == 1 or e.returncode == 101:
            try:
                data = json.loads(e.stdout.decode('utf-8'))
                vulnerabilities = data.get("vulnerabilities", {}).get("list", [])
                for v in vulnerabilities:
                    findings.append(v)
            except:
                findings.append({"error": "Failed to parse cargo audit output."})
    except FileNotFoundError:
        findings.append({"error": "cargo-audit not installed."})
    return findings

def check_hardcoded_secrets():
    findings = []
    for root, _, files in os.walk("mythrax-core/src"):
        for file in files:
            if file.endswith(".rs"):
                path = os.path.join(root, file)
                with open(path, "r") as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if re.search(r'(password|token|api_key|secret)\s*=\s*".+"', line, re.IGNORECASE):
                            findings.append((path, i+1, line.strip()))
    return findings

def check_untrusted_input():
    findings = []
    for root, _, files in os.walk("mythrax-core/src"):
        for file in files:
            if file.endswith(".rs"):
                path = os.path.join(root, file)
                with open(path, "r") as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if "fs::read_to_string(" in line or "Command::new(" in line or "File::open(" in line:
                            findings.append((path, i+1, line.strip()))
    return findings

def generate_report():
    print("Starting security audit...")
    stale_crates, yanked_crates = check_stale_crates()
    unsafe_blocks = check_unsafe()
    secrets_history = check_secrets_history()
    cargo_audit = check_cargo_audit()
    hardcoded_secrets = check_hardcoded_secrets()
    untrusted_inputs = check_untrusted_input()

    report = "# Mythrax Core Security Audit Report\n\n"

    report += "## Critical Findings\n\n"
    has_critical = False

    if hardcoded_secrets:
        report += "### Hardcoded Secrets in Source\n"
        body = "### Hardcoded Secrets in Source\n"
        for path, line_num, line in hardcoded_secrets:
            report += f"- {path}:{line_num}: `{line}`\n"
            body += f"- {path}:{line_num}: `{line}`\n"
        report += "  - **Risk**: Hardcoded credentials can be extracted by attackers.\n"
        report += "  - **Remediation**: Use environment variables or a secrets manager.\n"
        report += "  - **Effort**: Low\n\n"
        has_critical = True
        create_issue("Critical: Hardcoded Secrets Found", body + "\n**Risk**: Hardcoded credentials can be extracted by attackers.\n**Remediation**: Use environment variables or a secrets manager.\n**Effort**: Low")

    if secrets_history:
        report += "### Secrets in Git History\n"
        report += "Found potential secrets in git history.\n"
        body = "Found potential secrets in git history.\n"
        for commit, line in secrets_history[:5]:
            report += f"- Commit {commit}: `{line.strip()}`\n"
            body += f"- Commit {commit}: `{line.strip()}`\n"
        report += "  - **Risk**: Secrets in git history can be extracted even if removed from HEAD.\n"
        report += "  - **Remediation**: Rewrite git history using tools like BFG or git-filter-repo, and rotate the exposed secrets.\n"
        report += "  - **Effort**: High\n\n"
        has_critical = True
        create_issue("Critical: Secrets in Git History", body + "\n**Risk**: Secrets in git history can be extracted even if removed from HEAD.\n**Remediation**: Rewrite git history using tools like BFG or git-filter-repo, and rotate the exposed secrets.\n**Effort**: High")

    if cargo_audit and not isinstance(cargo_audit[0], dict) or cargo_audit and "error" not in cargo_audit[0]:
        report += "### Vulnerable Dependencies (Cargo Audit)\n"
        body = "### Vulnerable Dependencies (Cargo Audit)\n"
        for vuln in cargo_audit:
            if isinstance(vuln, dict) and "advisory" in vuln:
                adv = vuln["advisory"]
                report += f"- {adv.get('package')} ({adv.get('version')}): {adv.get('title')}\n"
                body += f"- {adv.get('package')} ({adv.get('version')}): {adv.get('title')}\n"
        report += "  - **Risk**: Known vulnerabilities in dependencies can be exploited.\n"
        report += "  - **Remediation**: Update dependencies to patched versions.\n"
        report += "  - **Effort**: Low to Medium\n\n"
        has_critical = True
        create_issue("Critical: Vulnerable Dependencies", body + "\n**Risk**: Known vulnerabilities in dependencies can be exploited.\n**Remediation**: Update dependencies to patched versions.\n**Effort**: Low to Medium")

    if not has_critical:
        report += "No critical findings.\n\n"

    report += "## High Findings\n\n"
    has_high = False

    if untrusted_inputs:
        report += "### Potential Untrusted Input to Sensitive APIs\n"
        report += "Found potential usage of file system or command execution APIs that might accept untrusted input.\n"
        body = "Found potential usage of file system or command execution APIs that might accept untrusted input.\n"
        for path, line_num, line in untrusted_inputs[:10]:
            report += f"- {path}:{line_num}: `{line}`\n"
            body += f"- {path}:{line_num}: `{line}`\n"
        report += "  - **Risk**: Path traversal or command injection if input is not sanitized.\n"
        report += "  - **Remediation**: Validate and sanitize all external input before using it in file or command operations.\n"
        report += "  - **Effort**: Medium\n\n"
        has_high = True
        create_issue("High: Potential Untrusted Input in Sensitive APIs", body + "\n**Risk**: Path traversal or command injection if input is not sanitized.\n**Remediation**: Validate and sanitize all external input before using it in file or command operations.\n**Effort**: Medium")

    if yanked_crates:
        report += "### Yanked Crates\n"
        body = "### Yanked Crates\n"
        for crate in yanked_crates:
            report += f"- {crate}\n"
            body += f"- {crate}\n"
        report += "  - **Risk**: Yanked crates may contain critical bugs or vulnerabilities.\n"
        report += "  - **Remediation**: Migrate to a supported version or alternative crate.\n"
        report += "  - **Effort**: Medium\n\n"
        has_high = True
        create_issue("High: Yanked Crates in Dependencies", body + "\n**Risk**: Yanked crates may contain critical bugs or vulnerabilities.\n**Remediation**: Migrate to a supported version or alternative crate.\n**Effort**: Medium")

    if not has_high:
        report += "No high findings.\n\n"

    report += "## Medium Findings\n\n"

    if stale_crates:
        report += "### Stale Crates\n"
        report += f"Found crates with no releases in > 1 year: {', '.join(stale_crates)}\n"
        report += "  - **Risk**: Stale crates may not receive security updates.\n"
        report += "  - **Remediation**: Consider migrating to actively maintained alternatives.\n"
        report += "  - **Effort**: Medium to High\n\n"

    unjustified_unsafe = [u for u in unsafe_blocks if not u[3]]
    if unjustified_unsafe:
        report += "### Unjustified Unsafe Blocks\n"
        for path, line_num, line, has_comment in unjustified_unsafe:
            report += f"- {path}:{line_num}: `{line}`\n"
        report += "  - **Risk**: Unsafe Rust can introduce memory safety vulnerabilities (e.g., use-after-free, buffer overflows).\n"
        report += "  - **Remediation**: Document the safety invariants with a `// SAFETY:` comment or refactor to safe Rust.\n"
        report += "  - **Effort**: Low to Medium\n\n"

    report += "## Low Findings\n\n"

    justified_unsafe = [u for u in unsafe_blocks if u[3]]
    if justified_unsafe:
        report += "### Documented Unsafe Blocks\n"
        for path, line_num, line, has_comment in justified_unsafe:
            report += f"- {path}:{line_num}: `{line}`\n"
        report += "  - **Risk**: Even documented unsafe blocks carry inherent memory safety risks.\n"
        report += "  - **Remediation**: Periodically review safety invariants and consider if safe alternatives are now available.\n"
        report += "  - **Effort**: Low\n\n"

    print(report)

if __name__ == '__main__':
    generate_report()
