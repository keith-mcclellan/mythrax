import os
import re
import json
import subprocess
import urllib.request
import urllib.error
from datetime import datetime, timezone
import toml

FINDINGS = []

def add_finding(title, severity, description, remediation, effort, file_path=None, line=None):
    FINDINGS.append({
        "title": title,
        "severity": severity,
        "description": description,
        "remediation": remediation,
        "effort": effort,
        "file_path": file_path,
        "line": line
    })

def check_hardcoded_secrets_source():
    secret_pattern = re.compile(r"(?i)(?:api_key|secret|token|password|credential)\s*[:=]\s*[\"'][a-zA-Z0-9_\-]{16,}[\"']")
    for root, _, files in os.walk('mythrax-core'):
        for file in files:
            if file.endswith(('.rs', '.toml', '.json', '.yml', '.yaml')):
                path = os.path.join(root, file)
                try:
                    with open(path, 'r', encoding='utf-8') as f:
                        for i, line in enumerate(f):
                            if secret_pattern.search(line):
                                add_finding(
                                    title=f"Hardcoded secret in {file}",
                                    severity="Critical",
                                    description="A hardcoded secret, token, or API key was found in source code.",
                                    remediation="Remove the hardcoded secret and use environment variables or a secure secret manager.",
                                    effort="Low",
                                    file_path=path,
                                    line=i+1
                                )
                except Exception:
                    pass

def check_git_history():
    cmd = ['git', 'log', '-G', 'api_key|secret|token|password|credential', '-i', '-p', '--all']
    try:
        process = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        current_commit = None
        current_file = None

        secret_pattern = re.compile(r"(?i)(?:api_key|secret|token|password|credential)\s*[:=]\s*[\"'][a-zA-Z0-9_\-]{16,}[\"']")

        for line in process.stdout:
            line = line.strip()
            if line.startswith('commit '):
                current_commit = line.split()[1]
            elif line.startswith('+++ b/'):
                current_file = line[6:]
            elif line.startswith('+') and not line.startswith('+++'):
                if secret_pattern.search(line):
                    add_finding(
                        title=f"Secret in git history for {os.path.basename(current_file) if current_file else 'unknown'}",
                        severity="Critical",
                        description=f"Potential secret found in git history at commit {current_commit[:8] if current_commit else 'unknown'}.",
                        remediation="Rotate the secret immediately and rewrite git history using tools like BFG or git-filter-repo.",
                        effort="High",
                        file_path=current_file
                    )
    except Exception as e:
        print(f"Failed to check git history: {e}")

def check_unsafe_rust():
    for root, _, files in os.walk('mythrax-core'):
        for file in files:
            if file.endswith('.rs'):
                path = os.path.join(root, file)
                try:
                    with open(path, 'r', encoding='utf-8') as f:
                        lines = f.readlines()

                    for i, line in enumerate(lines):
                        if re.search(r'\bunsafe\s*({|fn|impl|trait)', line):
                            documented = False
                            for j in range(max(0, i-5), i+1):
                                if 'safety' in lines[j].lower() or 'safety:' in lines[j].lower():
                                    documented = True
                                    break

                            severity = "Low" if documented else "High"
                            desc = "Unsafe block found. Memory safety risk: Unsafe Rust bypasses compiler checks for memory safety, potentially leading to undefined behavior, use-after-free, or memory corruption."
                            if not documented:
                                desc += " Lacks documented safety justification."

                            add_finding(
                                title=f"Unsafe Rust block in {file}",
                                severity=severity,
                                description=desc,
                                remediation="Document the safety invariants with `// SAFETY:` or refactor to safe Rust if possible.",
                                effort="Medium",
                                file_path=path,
                                line=i+1
                            )
                except Exception:
                    pass

def check_dependencies():
    # Run cargo audit
    try:
        res = subprocess.run(['cargo', 'audit', '--json'], cwd='mythrax-core', capture_output=True, text=True)
        if res.stdout:
            try:
                audit_data = json.loads(res.stdout)
                vulnerabilities = audit_data.get('vulnerabilities', {}).get('list', [])
                for vuln in vulnerabilities:
                    adv = vuln.get('advisory', {})
                    add_finding(
                        title=f"CVE in dependency: {adv.get('package')} ({adv.get('id')})",
                        severity="Critical" if adv.get('cvss') and "CRITICAL" in str(adv.get('cvss')).upper() else "High",
                        description=f"Vulnerability {adv.get('id')} in {adv.get('package')}: {adv.get('title')}",
                        remediation=f"Update {adv.get('package')} to a patched version: {adv.get('patched_versions')}",
                        effort="Low"
                    )
            except json.JSONDecodeError:
                pass
    except Exception as e:
        print(f"Failed to run cargo audit: {e}")

    # Parse Cargo.toml and check crates.io
    import time
    try:
        with open('mythrax-core/Cargo.toml', 'r') as f:
            cargo_toml = toml.load(f)

        deps = cargo_toml.get('dependencies', {})
        for dep, version in deps.items():
            if isinstance(version, dict) and 'version' not in version:
                continue

            time.sleep(0.1)
            try:
                req = urllib.request.Request(f"https://crates.io/api/v1/crates/{dep}", headers={'User-Agent': 'mythrax-ciso-audit'})
                with urllib.request.urlopen(req) as response:
                    data = json.loads(response.read().decode())
                    crate = data.get('crate', {})
                    versions = data.get('versions', [])

                    latest_version = versions[0] if versions else None
                    if latest_version and latest_version.get('yanked'):
                        add_finding(
                            title=f"Yanked dependency: {dep}",
                            severity="High",
                            description=f"The dependency {dep} is using a yanked version.",
                            remediation=f"Remove or replace {dep}.",
                            effort="Medium"
                        )

                    updated_at = crate.get('updated_at')
                    if updated_at:
                        dt = datetime.fromisoformat(updated_at.replace('Z', '+00:00'))
                        now = datetime.now(timezone.utc)
                        if (now - dt).days > 730:
                            add_finding(
                                title=f"Stale dependency: {dep}",
                                severity="Medium",
                                description=f"The dependency {dep} has not been updated in over 2 years.",
                                remediation=f"Consider replacing {dep} with an actively maintained alternative.",
                                effort="High"
                            )
            except Exception:
                pass
    except Exception as e:
        print(f"Failed to parse Cargo.toml: {e}")

def check_execution_paths():
    for root, _, files in os.walk('mythrax-core'):
        for file in files:
            if file.endswith('.rs'):
                path = os.path.join(root, file)
                try:
                    with open(path, 'r', encoding='utf-8') as f:
                        for i, line in enumerate(f):
                            if re.search(r'\bquery\s*\(\s*format!\s*\(', line):
                                add_finding(
                                    title=f"Potential NoSQL Injection in {file}",
                                    severity="Critical",
                                    description="Untrusted input might be interpolated directly into a SurrealDB query via `format!`.",
                                    remediation="Use parameterized queries with `$` placeholders and `.bind()`.",
                                    effort="Medium",
                                    file_path=path,
                                    line=i+1
                                )
                except Exception:
                    pass

def main():
    check_hardcoded_secrets_source()
    check_git_history()
    check_unsafe_rust()
    check_dependencies()
    check_execution_paths()

    report = "# Security Advisory Report\n\n"
    severity_order = {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}
    sorted_findings = sorted(FINDINGS, key=lambda x: severity_order.get(x['severity'], 4))

    seen_titles = set()
    deduped = []
    for f in sorted_findings:
        if f['title'] not in seen_titles:
            deduped.append(f)
            seen_titles.add(f['title'])

    existing_issues = set()
    repo = os.environ.get('GITHUB_REPOSITORY', '')
    token = os.environ.get('GITHUB_TOKEN', '')
    if repo and token:
        try:
            req = urllib.request.Request(f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100")
            req.add_header('Authorization', f'Bearer {token}')
            with urllib.request.urlopen(req) as response:
                issues = json.loads(response.read().decode())
                for issue in issues:
                    existing_issues.add(issue['title'])
        except Exception as e:
            print(f"Failed to fetch existing issues: {e}")

    for f in deduped:
        report += f"## [{f['severity']}] {f['title']}\n"
        report += f"**Description:** {f['description']}\n"
        if f['file_path']:
            loc = f"**Location:** {f['file_path']}"
            if f['line']:
                loc += f" (Line {f['line']})"
            report += loc + "\n"
        report += f"**Remediation:** {f['remediation']}\n"
        report += f"**Estimated Effort:** {f['effort']}\n\n"

        if f['severity'] in ['Critical', 'High'] and repo and token:
            if f['title'] not in existing_issues:
                try:
                    issue_data = {
                        "title": f['title'],
                        "body": f"### Security Finding: {f['title']}\n\n**Severity:** {f['severity']}\n\n**Description:** {f['description']}\n\n**Location:** {f['file_path']} (Line {f['line']})\n\n**Remediation:** {f['remediation']}\n\n**Estimated Effort:** {f['effort']}",
                        "labels": ["security", "bug", f"severity:{f['severity'].lower()}"]
                    }
                    req = urllib.request.Request(f"https://api.github.com/repos/{repo}/issues",
                                                 data=json.dumps(issue_data).encode('utf-8'),
                                                 headers={'Authorization': f'Bearer {token}', 'Accept': 'application/vnd.github.v3+json', 'Content-Type': 'application/json'})
                    with urllib.request.urlopen(req) as response:
                        print(f"Created issue for: {f['title']}")
                except Exception as e:
                    print(f"Failed to create issue {f['title']}: {e}")
            else:
                print(f"Issue already exists for: {f['title']}")

    with open('security_advisory_report.md', 'w') as f:
        f.write(report)

if __name__ == "__main__":
    main()
