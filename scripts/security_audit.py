import os
import re
import subprocess
import json
import shlex
import sys
import hashlib
from datetime import datetime

def run_cmd(cmd, cwd=None, env=None):
    res = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=cwd, env=env)
    return res.stdout, res.stderr, res.returncode

def get_files(directory):
    for root, dirs, files in os.walk(directory):
        if 'target' in dirs:
            dirs.remove('target')
        if '.git' in dirs:
            dirs.remove('.git')
        for file in files:
            yield os.path.join(root, file)

def scan_secrets(files):
    findings = []
    secret_regex = re.compile(r'(?i)(?:api_key|token|secret|password|bearer|auth)\s*[:=]\s*["\']([a-zA-Z0-9_\-\.]{8,})["\']')

    for filepath in files:
        if filepath.endswith('.rs') or filepath.endswith('.toml') or filepath.endswith('.yml') or filepath.endswith('.yaml') or filepath.endswith('.md'):
            try:
                with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
                    for idx, line in enumerate(f):
                        if secret_regex.search(line):
                            findings.append({
                                'title': f"Hardcoded Secret in {os.path.basename(filepath)}:{idx+1}",
                                'file': filepath,
                                'line': idx + 1,
                                'content': line.strip(),
                                'risk': 'Critical',
                                'effort': 'Low',
                                'recommendation': 'Extract to environment variable or secure secret manager.',
                                'desc': 'Found potential hardcoded secret or token.'
                            })
            except Exception:
                pass
    return findings

def scan_unsafe(files):
    findings = []
    for filepath in files:
        if not filepath.endswith('.rs'):
            continue
        try:
            with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
                lines = f.readlines()
                for idx, line in enumerate(lines):
                    if 'unsafe {' in line or 'unsafe fn' in line:
                        has_comment = False
                        for prev in range(max(0, idx - 5), idx):
                            if 'SAFETY:' in lines[prev]:
                                has_comment = True
                                break
                        if not has_comment:
                            findings.append({
                                'title': f"Unjustified Unsafe Rust Block in {os.path.basename(filepath)}:{idx+1}",
                                'file': filepath,
                                'line': idx + 1,
                                'content': line.strip(),
                                'risk': 'High',
                                'effort': 'Medium',
                                'recommendation': 'Add SAFETY comment justifying memory safety, or refactor to safe Rust.',
                                'desc': "Unjustified unsafe block missing SAFETY comment. Memory safety risk: manual memory management or unchecked invariants could lead to memory corruption, use-after-free, or data races."
                            })
                        else:
                             findings.append({
                                'title': f"Unsafe Rust Block with SAFETY comment in {os.path.basename(filepath)}:{idx+1}",
                                'file': filepath,
                                'line': idx + 1,
                                'content': line.strip(),
                                'risk': 'Medium',
                                'effort': 'Medium',
                                'recommendation': 'Review SAFETY comment to ensure it fully justifies memory safety.',
                                'desc': "Unsafe block with SAFETY justification."
                            })
        except Exception:
            pass
    return findings

def scan_dependencies():
    findings = []
    stdout, stderr, code = run_cmd('cargo audit -q --json', cwd='mythrax-core')
    if code != 0 and 'cargo: No such file or directory' not in stderr and 'no such command: `audit`' not in stderr:
        try:
            data = json.loads(stdout)
            vulns = data.get('vulnerabilities', {}).get('list', [])
            for v in vulns:
                adv = v.get('advisory', {})
                package = v.get('package', {}).get('name', 'unknown')
                version = v.get('package', {}).get('version', 'unknown')
                title = adv.get('title', 'Vulnerability')
                findings.append({
                    'title': f"Vulnerable Dependency: {package} {version}",
                    'file': 'Cargo.lock',
                    'line': 0,
                    'content': f"{package} v{version}: {title}",
                    'risk': 'Critical' if adv.get('severity') == 'critical' else 'High',
                    'effort': 'Low',
                    'recommendation': f"Update {package} to a patched version.",
                    'desc': f"CVE/Advisory: {adv.get('id')}. {title}"
                })
            warnings = data.get('warnings', {})
            for w in warnings.values():
                for warn in w:
                    kind = warn.get('kind')
                    package = warn.get('package', {}).get('name', 'unknown')
                    version = warn.get('package', {}).get('version', 'unknown')
                    if kind == 'yanked':
                        findings.append({
                            'title': f"Yanked Dependency: {package} {version}",
                            'file': 'Cargo.lock',
                            'line': 0,
                            'content': f"{package} v{version} is yanked",
                            'risk': 'Medium',
                            'effort': 'Low',
                            'recommendation': f"Update {package} to a non-yanked version.",
                            'desc': f"The crate version {package} {version} has been yanked."
                        })
                    elif kind == 'unmaintained':
                        findings.append({
                            'title': f"Unmaintained Dependency: {package} {version}",
                            'file': 'Cargo.lock',
                            'line': 0,
                            'content': f"{package} v{version} is unmaintained",
                            'risk': 'High',
                            'effort': 'Medium',
                            'recommendation': f"Migrate away from {package} as it has no recent commits and is unmaintained.",
                            'desc': f"The crate {package} is unmaintained."
                        })
        except Exception:
            pass

    return findings

def scan_inputs(files):
    findings = []
    cmd_regex = re.compile(r'Command::new\s*\(')
    for filepath in files:
        if not filepath.endswith('.rs'):
            continue
        try:
            with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
                lines = f.readlines()
                for idx, line in enumerate(lines):
                    if cmd_regex.search(line):
                        findings.append({
                            'title': f"External Command Invocation in {os.path.basename(filepath)}:{idx+1}",
                            'file': filepath,
                            'line': idx + 1,
                            'content': line.strip(),
                            'risk': 'High',
                            'effort': 'Medium',
                            'recommendation': 'Ensure inputs are heavily sanitized, use explicit arguments instead of shell strings, or avoid this pattern.',
                            'desc': 'External process invocation via std::process::Command can lead to command injection if external input is not sanitized.'
                        })
        except Exception:
            pass
    return findings

def scan_git_history():
    findings = []
    cmd = "git log -p --all"
    stdout, stderr, code = run_cmd(cmd)

    secret_regex = re.compile(r'(?i)(?:api_key|token|secret|password|bearer|auth)\s*[:=]\s*["\']([a-zA-Z0-9_\-\.]{8,})["\']')

    current_commit = ""
    current_file = ""
    for line in stdout.split('\n'):
        if line.startswith('commit '):
            current_commit = line.split(' ')[1]
        elif line.startswith('+++ b/'):
            current_file = line[6:]
        elif line.startswith('+') and not line.startswith('+++'):
            if secret_regex.search(line):
                findings.append({
                    'title': f"Secret in Git History: {current_file} in {current_commit[:8]}",
                    'file': current_file,
                    'line': 0,
                    'content': line[1:].strip()[:100],
                    'risk': 'Critical',
                    'effort': 'High',
                    'recommendation': 'Rotate the secret immediately and rewrite git history using BFG or git-filter-repo.',
                    'desc': 'Found potential secret committed in git history.'
                })
                # Cap the findings just to be safe
                if len(findings) > 20:
                    break
    return findings

def get_issue_id(title):
    return hashlib.md5(title.encode()).hexdigest()[:8]

def file_issue(finding):
    title = finding['title']

    body = f"**Finding:** {finding['desc']}\n"
    body += f"**File:** `{finding['file']}:{finding['line']}`\n"
    body += "**Code:**\n```rust\n"
    body += f"{finding['content']}\n"
    body += "```\n"
    body += f"**Risk:** {finding['risk']}\n"
    body += f"**Estimated Effort:** {finding['effort']}\n"
    body += f"**Recommendation:** {finding['recommendation']}\n"

    # Write issue via GH API if available
    if os.environ.get('GITHUB_ACTIONS'):
        import tempfile
        with tempfile.NamedTemporaryFile(mode='w', delete=False) as f:
            f.write(body)
            temp_path = f.name

        # Check if issue already exists
        escaped_title = shlex.quote(title)
        cmd_check = f"gh issue list --search \"in:title {escaped_title}\" --state open --json title"
        stdout, _, _ = run_cmd(cmd_check)
        try:
             issues = json.loads(stdout)
             if issues:
                 return # Issue exists
        except:
             pass

        cmd = f"gh issue create --title {shlex.quote(title)} --body-file {shlex.quote(temp_path)} --label security --label {finding['risk'].lower()}"
        run_cmd(cmd)
        os.remove(temp_path)
    else:
        # Local mock
        os.makedirs('issues', exist_ok=True)
        issue_id = get_issue_id(title)
        filepath = f"issues/{issue_id}.md"
        if not os.path.exists(filepath):
            with open(filepath, 'w') as f:
                f.write(f"# {title}\n\n{body}")

def main():
    files = list(get_files('.'))
    findings = []
    findings.extend(scan_secrets(files))
    findings.extend(scan_unsafe(files))
    findings.extend(scan_dependencies())
    findings.extend(scan_inputs(files))
    findings.extend(scan_git_history())

    # Sort by risk
    risk_order = {'Critical': 0, 'High': 1, 'Medium': 2, 'Low': 3}
    findings.sort(key=lambda x: risk_order.get(x['risk'], 4))

    # Output report
    report = "# Security Advisory Report\n\n"
    report += f"Date: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n\n"

    for f in findings:
        report += f"## [{f['risk']}] {f['title']}\n"
        report += f"- **Description:** {f['desc']}\n"
        report += f"- **Location:** `{f['file']}:{f['line']}`\n"
        report += f"- **Code:** `{f['content']}`\n"
        report += f"- **Remediation:** {f['recommendation']}\n"
        report += f"- **Estimated Effort:** {f['effort']}\n\n"

    print(report)

    # File issues for Critical/High
    for f in findings:
        if f['risk'] in ['Critical', 'High']:
            file_issue(f)

if __name__ == '__main__':
    main()
