import os
import re
import json
import subprocess
import shlex
import sys

def run_cmd(cmd, cwd=None):
    result = subprocess.run(cmd, shell=True, cwd=cwd, capture_output=True, text=True)
    return result.stdout, result.stderr, result.returncode

findings = []

def add_finding(title, severity, description, remediation, estimated_effort, file_path=None, line=None):
    findings.append({
        "title": title,
        "severity": severity,
        "description": description,
        "remediation": remediation,
        "estimated_effort": estimated_effort,
        "file_path": file_path,
        "line": line
    })

def scan_files():
    secret_pattern = re.compile(r'(?i)(api[_-]?key|secret|token|password)[\s]*[:=]\s*["\']([^"\']+)["\']')
    safe_extensions = {'.rs', '.toml', '.json', '.yaml', '.yml', '.md'}

    for root, dirs, files in os.walk('mythrax-core'):
        dirs[:] = [d for d in dirs if d not in ['target', '.git', '.venv']]
        for file in files:
            ext = os.path.splitext(file)[1]
            if ext not in safe_extensions and file != '.env' and 'Cargo' not in file:
                continue

            filepath = os.path.join(root, file)
            try:
                with open(filepath, 'r', encoding='utf-8') as f:
                    lines = f.readlines()
            except Exception:
                continue

            content = "".join(lines)

            for i, line in enumerate(lines):
                # Hardcoded secrets
                if secret_pattern.search(line):
                    add_finding(
                        title=f"Hardcoded Secret in {os.path.basename(file)}",
                        severity="Critical",
                        description=f"Hardcoded credential/secret found in {filepath} on line {i+1}: `{line.strip()}`",
                        remediation="Extract secret to an environment variable or secure vault.",
                        estimated_effort="Low",
                        file_path=filepath,
                        line=i+1
                    )

                # Unsafe Rust blocks
                if ext == '.rs' and re.search(r'\bunsafe\s*\{|\bunsafe\s+fn\b', line):
                    has_safety = False
                    start_idx = max(0, i-5)
                    for j in range(start_idx, i+1):
                        if 'SAFETY:' in lines[j] or 'Safety:' in lines[j] or 'safety:' in lines[j]:
                            has_safety = True
                            break
                    if not has_safety:
                        add_finding(
                            title=f"Unjustified unsafe block in {os.path.basename(file)}",
                            severity="High",
                            description=f"Unsafe block lacking a 'SAFETY:' justification comment in {filepath} on line {i+1}. Risk of memory safety violation.",
                            remediation="Add a 'SAFETY:' comment explaining why the unsafe block is memory safe, or refactor to safe Rust.",
                            estimated_effort="Low",
                            file_path=filepath,
                            line=i+1
                        )
                    else:
                        add_finding(
                            title=f"Documented unsafe block in {os.path.basename(file)}",
                            severity="Low",
                            description=f"Documented unsafe block in {filepath} on line {i+1}. Risk of memory safety violation if invariants change.",
                            remediation="Regularly review memory safety invariants.",
                            estimated_effort="Medium",
                            file_path=filepath,
                            line=i+1
                        )

            # Execution paths accepting untrusted external input (Command::new)
            if ext == '.rs':
                for match in re.finditer(r'Command::new\s*\(\s*[^)]+\s*\)([\s\S]{0,150}?)\.args?\s*\(\s*([^"\'&)]+)\)', content):
                    start_pos = match.start()
                    line_no = content.count('\n', 0, start_pos) + 1
                    var_name = match.group(2).strip()
                    if not var_name.startswith('vec!') and not var_name.startswith('['):
                        add_finding(
                            title=f"Unsanitized Input in Command in {os.path.basename(file)}",
                            severity="High",
                            description=f"External process invocation passes non-literal variable `{var_name}` as argument in {filepath} on line {line_no}. Potential command injection risk.",
                            remediation="Sanitize external input or use strict boundary enforcement before passing to Command.",
                            estimated_effort="Medium",
                            file_path=filepath,
                            line=line_no
                        )

def scan_dependencies():
    stdout, stderr, code = run_cmd('cargo audit --json', cwd='mythrax-core')
    if stdout:
        try:
            data = json.loads(stdout)
            vulnerabilities = data.get('vulnerabilities', {})
            warnings = data.get('warnings', {})

            if 'list' in vulnerabilities:
                for vuln in vulnerabilities['list']:
                    pkg_name = vuln['advisory']['package']
                    vuln_id = vuln['advisory']['id']
                    add_finding(
                        title=f"CVE in Dependency: {pkg_name}",
                        severity="Critical",
                        description=f"Known CVE {vuln_id} in {pkg_name}: {vuln['advisory']['title']}",
                        remediation=f"Update {pkg_name} to a secure patched version.",
                        estimated_effort="Low"
                    )

            for kind, warn_list in warnings.items():
                for warn in warn_list:
                    pkg_name = warn.get('package', {}).get('name', 'unknown')
                    add_finding(
                        title=f"Dependency Warning: {pkg_name} ({kind})",
                        severity="Medium",
                        description=f"Package {pkg_name} triggered a warning: {kind}. May be yanked or unmaintained.",
                        remediation=f"Consider replacing or updating {pkg_name}.",
                        estimated_effort="Medium"
                    )
        except json.JSONDecodeError:
            print("Failed to parse cargo audit JSON output", file=sys.stderr)

def scan_git_history():
    pattern = r"(?i)(api[_-]?key|secret|token|password)[\s]*[:=][\s]*[\"'][^\"']+[\"']"
    cmd = f"git log -G '{pattern}' -P --pretty=format:'%H'"
    stdout, stderr, code = run_cmd(cmd)

    if stdout:
        commits = set(stdout.strip().split('\n'))
        for commit in commits:
            if not commit: continue
            add_finding(
                title=f"Secret in Git History (Commit: {commit[:8]})",
                severity="Critical",
                description=f"Commit {commit} contains a potential hardcoded secret.",
                remediation="Rotate the exposed secret and rewrite git history using filter-repo.",
                estimated_effort="High"
            )

def generate_report():
    report = "# CISO Security Advisory Report\n\n"
    for sev in ['Critical', 'High', 'Medium', 'Low']:
        sev_findings = [f for f in findings if f['severity'] == sev]
        if sev_findings:
            report += f"## {sev} Severity Findings\n\n"
            for f in sev_findings:
                report += f"### {f['title']}\n"
                report += f"- **Severity:** {f['severity']}\n"
                report += f"- **Description:** {f['description']}\n"
                report += f"- **Remediation:** {f['remediation']}\n"
                report += f"- **Estimated Effort:** {f['estimated_effort']}\n"
                if f.get('file_path'):
                    report += f"- **Location:** {f['file_path']}"
                    if f.get('line'):
                        report += f":{f['line']}"
                    report += "\n"
                report += "\n"
    with open('security_advisory_report.md', 'w') as f:
        f.write(report)
    print("Generated security_advisory_report.md")

def file_github_issues():
    for f in findings:
        if f['severity'] in ['Critical', 'High']:
            title = f"🛡️ CISO: [{f['severity'].upper()}] {f['title']}"
            body = f"🚨 **Severity:** {f['severity']}\n" \
                   f"💡 **Vulnerability:** {f['description']}\n" \
                   f"🎯 **Impact:** Potential security compromise\n" \
                   f"🔧 **Remediation:** {f['remediation']}\n" \
                   f"⏱️ **Estimated Effort:** {f['estimated_effort']}\n"

            if f.get('file_path'):
                body += f"\n📁 **Location:** {f['file_path']}:{f.get('line', '')}"

            safe_title = shlex.quote(title)
            safe_body = shlex.quote(body)

            check_cmd = f"gh issue list --search in:title {safe_title} --json number"
            stdout, _, _ = run_cmd(check_cmd)
            try:
                issues = json.loads(stdout)
                if len(issues) > 0:
                    continue
            except:
                pass

            create_cmd = f"gh issue create --title {safe_title} --body {safe_body} --label security"
            stdout, stderr, code = run_cmd(create_cmd)
            if code != 0:
                print(f"Failed to create issue for: {title}. Note: gh CLI might not be authenticated locally.", file=sys.stderr)
            else:
                print(f"Filed issue: {title}")

if __name__ == "__main__":
    scan_files()
    scan_dependencies()
    scan_git_history()
    generate_report()
    file_github_issues()
