import os
import re
import json
import subprocess
import urllib.request
import urllib.error
from datetime import datetime, timezone

def load_ciso_config():
    try:
        with open('.agents/ciso.json', 'r') as f:
            config = json.load(f)
            return config['config']['customAgent']['systemPromptSections']
    except Exception as e:
        print(f"Failed to load CISO config: {e}")
        return []

def run_cmd(cmd):
    try:
        result = subprocess.run(cmd, shell=True, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        return result.stdout
    except subprocess.CalledProcessError as e:
        return e.stdout

def check_secrets_git_history():
    print("Checking git history for secrets...")
    run_cmd("git fetch --unshallow || git fetch --all")
    cmd = "git log -p -n 100 | grep -iE 'api_key|secret_key|password.*=|token.*=' | grep -v 'test' || true"
    out = run_cmd(cmd)
    findings = []
    for line in out.split('\n'):
        if line.strip() and not line.startswith('+++') and not line.startswith('---'):
            findings.append({"description": line.strip()[:100]})
    return findings

def check_secrets_current():
    print("Checking codebase for hardcoded secrets...")
    cmd = "grep -rnwiE 'api_key|secret_key|password.*=|token.*=' mythrax-core/src/ mythrax-core/Cargo.toml mythrax-core/Cargo.lock | grep -v 'test' || true"
    out = run_cmd(cmd)
    findings = []
    for line in out.split('\n'):
        if line.strip():
            findings.append({"description": line.strip()[:100]})
    return findings

def check_unsafe_blocks():
    print("Checking codebase for unsafe blocks...")
    findings = []
    for root, dirs, files in os.walk('mythrax-core/src'):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                with open(filepath, 'r') as f:
                    lines = f.readlines()
                for i, line in enumerate(lines):
                    if 'unsafe ' in line or 'unsafe{' in line or 'unsafe {' in line:
                        justified = False
                        for j in range(max(0, i-5), i):
                            if 'Safety:' in lines[j] or 'safety:' in lines[j].lower():
                                justified = True
                                break
                        risk = "Memory corruption, undefined behavior, or segmentation faults due to unchecked invariants."
                        if 'libc' in line or (i+1 < len(lines) and 'libc' in lines[i+1]):
                            risk = "FFI boundaries can lead to undefined behavior if C invariants are not upheld."
                        findings.append({
                            "file": filepath,
                            "line": i + 1,
                            "content": line.strip(),
                            "justified": justified,
                            "risk": risk
                        })
    return findings

def check_cargo_audit():
    print("Checking dependencies with cargo audit...")
    cmd = "cd mythrax-core && cargo audit --json"
    out = run_cmd(cmd)
    findings = []
    try:
        data = json.loads(out)
        for vuln in data.get('vulnerabilities', {}).get('list', []):
            findings.append({
                "package": vuln.get('package', {}).get('name'),
                "version": vuln.get('package', {}).get('version'),
                "id": vuln.get('advisory', {}).get('id'),
                "title": vuln.get('advisory', {}).get('title')
            })
    except:
        pass
    return findings

def get_dependencies():
    deps = set()
    try:
        with open('mythrax-core/Cargo.toml', 'r') as f:
            in_deps = False
            for line in f:
                if line.strip().startswith('[dependencies]'):
                    in_deps = True
                elif line.strip().startswith('['):
                    in_deps = False
                elif in_deps and '=' in line and not line.strip().startswith('#'):
                    dep = line.split('=')[0].strip()
                    deps.add(dep.strip('"'))
    except:
        pass
    return list(deps)

def check_crates_io(deps):
    print("Checking dependencies on crates.io...")
    findings = []
    for dep in deps:
        url = f"https://crates.io/api/v1/crates/{dep}"
        try:
            req = urllib.request.Request(url, headers={'User-Agent': 'mythrax-security-audit/1.0'})
            with urllib.request.urlopen(req, timeout=5) as response:
                data = json.loads(response.read().decode())
                versions = data.get('versions', [])
                if not versions:
                    continue
                latest = versions[0]
                if latest.get('yanked'):
                    findings.append({
                        "package": dep,
                        "issue": "Yanked version in use",
                        "details": "The crate has been yanked from crates.io"
                    })
                updated_at_str = latest.get('updated_at')
                if updated_at_str:
                    try:
                        updated_at = datetime.fromisoformat(updated_at_str.replace('Z', '+00:00'))
                        now = datetime.now(timezone.utc)
                        delta = now - updated_at
                        if delta.days > 365 * 2: # 2 years
                            findings.append({
                                "package": dep,
                                "issue": "Stale crate with no recent commits/updates",
                                "details": f"Last updated {delta.days} days ago ({updated_at_str})"
                            })
                    except Exception as e:
                        pass
        except Exception as e:
            pass
    return findings

def check_untrusted_inputs():
    print("Checking for untrusted external inputs...")
    findings = []
    cmd = "grep -rnw 'mythrax-core/src/' -e 'Command::new' || true"
    out = run_cmd(cmd)
    for line in out.split('\n'):
        if line.strip():
            findings.append({"description": line.strip()[:100]})
    return findings

def file_github_issue(title, body):
    token = os.environ.get('GH_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY', 'keith-mcclellan/mythrax')
    if not token:
        print(f"Would file issue: {title}")
        return
    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json"
    }
    data = json.dumps({"title": title, "body": body}).encode('utf-8')
    req = urllib.request.Request(url, data=data, headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Created issue successfully: {response.getcode()}")
    except urllib.error.URLError as e:
        print(f"Failed to create issue: {e}")

def main():
    prompt_sections = load_ciso_config()
    if prompt_sections:
        print("Loaded CISO Profile Config...")

    git_secrets = check_secrets_git_history()
    curr_secrets = check_secrets_current()
    unsafe_blocks = check_unsafe_blocks()
    audit_vulns = check_cargo_audit()
    deps = get_dependencies()
    crate_issues = check_crates_io(deps)
    untrusted_inputs = check_untrusted_inputs()

    report_lines = ["# CISO Security Advisory Report\n"]
    issues_to_file = []
    report_lines.append("## Findings Ranked by Severity\n")

    if curr_secrets or git_secrets or audit_vulns:
        report_lines.append("### Critical Findings\n")
    for s in curr_secrets:
        report_lines.append(f"- **Hardcoded Secret in Source**: `{s['description']}`")
        report_lines.append("  - **Remediation**: Remove the secret and use a secrets manager or environment variable.")
        report_lines.append("  - **Effort**: Low\n")
        issues_to_file.append(("CRITICAL: Hardcoded Secret Discovered", f"Secret found: `{s['description']}`\n\nRemediation: Revoke and use Secrets Manager."))
    for s in git_secrets:
        report_lines.append(f"- **Secret in Git History**: `{s['description']}`")
        report_lines.append("  - **Remediation**: Use BFG Repo-Cleaner or git-filter-repo to rewrite history. Rotate the secret immediately.")
        report_lines.append("  - **Effort**: High\n")
        issues_to_file.append(("CRITICAL: Secret Discovered in Git History", f"Secret found: `{s['description']}`\n\nRemediation: Rewrite git history and revoke."))
    for v in audit_vulns:
        report_lines.append(f"- **Vulnerable Dependency (CVE)**: {v['package']} {v['version']} - {v['title']} (ID: {v['id']})")
        report_lines.append("  - **Remediation**: Update the package to a patched version using `cargo update -p <pkg>`.")
        report_lines.append("  - **Effort**: Low\n")
        issues_to_file.append((f"CRITICAL: Vulnerable Dependency {v['package']}", f"Vulnerability: {v['title']}\nID: {v['id']}\n\nRemediation: Update dependency."))

    unjustified_unsafe = [u for u in unsafe_blocks if not u['justified']]
    if unjustified_unsafe or untrusted_inputs:
        report_lines.append("### High Findings\n")
    for u in unjustified_unsafe:
        report_lines.append(f"- **Unjustified Unsafe Block**: `{u['file']}:{u['line']}`")
        report_lines.append(f"  - **Risk**: {u['risk']}")
        report_lines.append("  - **Remediation**: Add a `// Safety:` comment explaining why this block is safe, or refactor to safe Rust.")
        report_lines.append("  - **Effort**: Medium\n")
        issues_to_file.append(("HIGH: Unjustified Unsafe Block", f"Location: `{u['file']}:{u['line']}`\nCode: `{u['content']}`\nRisk: {u['risk']}\n\nRemediation: Document or Refactor."))
    for ui in untrusted_inputs:
        report_lines.append(f"- **Untrusted External Input Path**: `{ui['description']}`")
        report_lines.append("  - **Remediation**: Sanitize input boundaries before passing to Command::new or external sinks.")
        report_lines.append("  - **Effort**: Medium\n")
        issues_to_file.append(("HIGH: Untrusted External Input Path", f"Location: `{ui['description']}`\n\nRemediation: Sanitize input."))

    if crate_issues:
        report_lines.append("### Medium Findings\n")
    for ci in crate_issues:
        report_lines.append(f"- **Supply Chain Risk**: {ci['package']} - {ci['issue']}")
        report_lines.append(f"  - **Details**: {ci['details']}")
        report_lines.append("  - **Remediation**: Migrate to an actively maintained alternative or audit the source for backdooring.")
        report_lines.append("  - **Effort**: High\n")

    justified_unsafe = [u for u in unsafe_blocks if u['justified']]
    if justified_unsafe:
        report_lines.append("### Low Findings\n")
    for u in justified_unsafe:
        report_lines.append(f"- **Justified Unsafe Block**: `{u['file']}:{u['line']}`")
        report_lines.append(f"  - **Risk**: {u['risk']}")
        report_lines.append("  - **Remediation**: Periodically review for safer alternatives in the ecosystem.")
        report_lines.append("  - **Effort**: Low\n")

    full_report = "\n".join(report_lines)
    print(full_report)

    for title, body in issues_to_file:
        file_github_issue(title, body)

if __name__ == '__main__':
    main()
