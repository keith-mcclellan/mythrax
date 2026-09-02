import subprocess
import json
import urllib.request
import os
import re
import datetime

def print_finding(finding):
    print(f"\n--- {finding['severity'].upper()}: {finding['title']} ---")
    print(f"Description: {finding['description']}")
    print(f"Remediation: {finding['remediation']}")
    print(f"Estimated Effort: {finding['effort']}")
    if 'file' in finding:
        print(f"File: {finding['file']}:{finding.get('line', '')}")

def get_recent_commits(crate_name):
    try:
        req = urllib.request.Request(f"https://crates.io/api/v1/crates/{crate_name}")
        req.add_header('User-Agent', 'mythrax-security-audit')
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            return data.get('crate', {}).get('updated_at', '')
    except Exception:
        return None

def main():
    try:
        with open('.agents/agents/ciso_agent/config.json', 'r') as f:
            config = json.load(f)
            system_prompt_sections = config.get('config', {}).get('customAgent', {}).get('systemPromptSections', [])
    except Exception as e:
        pass

    findings = []

    # 1. Unsafe blocks
    try:
        result = subprocess.run("grep -rn 'unsafe' mythrax-core/src/ | grep -v 'libc::' | grep -v 'Send' | grep -v 'Sync'", shell=True, capture_output=True, text=True)
        for line in result.stdout.splitlines():
            parts = line.split(':', 2)
            if len(parts) >= 3:
                file_path, line_num, _ = parts
                try:
                    with open(file_path, 'r') as f:
                        lines = f.readlines()
                        idx = int(line_num) - 1
                        justified = any("Safety:" in lines[i] or "SAFETY:" in lines[i] for i in range(max(0, idx - 5), idx))
                        if not justified:
                            findings.append({
                                'title': f'Undocumented unsafe block in {file_path}',
                                'severity': 'high',
                                'description': 'Unsafe Rust block found without a corresponding "Safety:" justification comment.',
                                'remediation': 'Add a `// Safety: ...` comment explaining why this unsafe block is sound.',
                                'effort': 'Medium',
                                'file': file_path,
                                'line': line_num
                            })
                except Exception:
                    pass
    except Exception:
        pass

    # 2. Hardcoded secrets
    secret_patterns = [
        r'ghp_[A-Za-z0-9_]{36}',
        r'glpat-[A-Za-z0-9_\-]{20}',
        r'xox[baprs]-[0-9]{12}-[0-9]{12}-[a-zA-Z0-9]{24}',
        r'(?i)(?:api_key|apikey|secret|password|token)\s*=\s*["\'][a-zA-Z0-9_\-]{16,}["\']'
    ]
    for root, dirs, files in os.walk('.'):
        dirs[:] = [d for d in dirs if d not in ['.git', 'target', '.agents']]
        for file in files:
            if file.endswith(('.rs', '.py', '.js', '.json', '.toml', '.yml', '.yaml', '.sh')):
                file_path = os.path.join(root, file)
                try:
                    with open(file_path, 'r', errors='ignore') as f:
                        for i, line in enumerate(f):
                            if any(re.search(p, line) for p in secret_patterns):
                                findings.append({
                                    'title': f'Potential Hardcoded Secret found in {file_path}',
                                    'severity': 'critical',
                                    'description': 'A string matching a known secret pattern was found.',
                                    'remediation': 'Remove the hardcoded secret. Use environment variables.',
                                    'effort': 'Low',
                                    'file': file_path,
                                    'line': i + 1
                                })
                except Exception:
                    pass

    # 3. Cargo.toml vulnerable / stale dependencies
    try:
        result = subprocess.run(["cargo", "audit", "--json"], cwd="mythrax-core", capture_output=True, text=True)
        if result.stdout:
            try:
                audit_data = json.loads(result.stdout)
                if 'vulnerabilities' in audit_data and 'list' in audit_data['vulnerabilities']:
                    for vuln in audit_data['vulnerabilities']['list']:
                        findings.append({
                            'title': f"Vulnerable Dependency: {vuln['advisory']['package']} {vuln['advisory']['version']}",
                            'severity': 'high',
                            'description': vuln['advisory']['description'],
                            'remediation': "Update to a safe version.",
                            'effort': 'Medium',
                            'file': 'mythrax-core/Cargo.toml'
                        })

                    if 'warnings' in audit_data:
                        for warning in audit_data['warnings']:
                            if warning.get('kind') == 'yanked':
                                package_info = warning.get('package', {})
                                findings.append({
                                    'title': f"Yanked Dependency: {package_info.get('name')} {package_info.get('version')}",
                                    'severity': 'high',
                                    'description': 'This crate version has been yanked from crates.io.',
                                    'remediation': "Update to a non-yanked version.",
                                    'effort': 'Medium',
                                    'file': 'mythrax-core/Cargo.toml'
                                })

            except Exception:
                pass
    except Exception:
        pass

    try:
        with open('mythrax-core/Cargo.toml', 'r') as f:
            deps = re.findall(r'^([a-zA-Z0-9_\-]+)\s*=\s*(?:\{.*\}|".*")', f.read(), re.MULTILINE)
            for dep in deps:
                updated_at = get_recent_commits(dep)
                if updated_at:
                    updated_date = datetime.datetime.strptime(updated_at.split('T')[0], '%Y-%m-%d')
                    if (datetime.datetime.now() - updated_date).days > 730:
                        findings.append({
                            'title': f'Stale Dependency: {dep}',
                            'severity': 'low',
                            'description': f'Crate {dep} has not been updated on crates.io since {updated_at}.',
                            'remediation': 'Consider a more actively maintained alternative.',
                            'effort': 'High',
                            'file': 'mythrax-core/Cargo.toml'
                        })
    except Exception:
        pass

    # 4. Input sanitization (grep)
    try:
        result = subprocess.run("grep -rn 'req.body(' mythrax-core/src/ || true", shell=True, capture_output=True, text=True)
        for line in result.stdout.splitlines():
            parts = line.split(':', 2)
            if len(parts) >= 2:
                findings.append({
                    'title': 'Potential unvalidated input',
                    'severity': 'medium',
                    'description': 'Execution path accepts untrusted external input without obvious sanitization.',
                    'remediation': 'Ensure input is validated and sanitized before processing.',
                    'effort': 'Medium',
                    'file': parts[0],
                    'line': parts[1]
                })
    except Exception:
        pass

    # 5. Git history secrets
    try:
        result = subprocess.run("git log -p | grep -E -i 'api_key|secret|password|token' || true", shell=True, capture_output=True, text=True)
        if result.stdout:
            findings.append({
                'title': 'Secrets potentially committed in git history',
                'severity': 'critical',
                'description': 'Found matches for secret-like keywords in the git history.',
                'remediation': 'Review git history, rotate compromised secrets, and rewrite history if necessary.',
                'effort': 'High'
            })
    except Exception:
        pass

    for finding in findings:
        print_finding(finding)

    gh_token = os.environ.get('GH_TOKEN')
    if gh_token:
        repo = os.environ.get('GITHUB_REPOSITORY', 'keith-mcclellan/mythrax')
        for finding in findings:
            if finding['severity'] in ['critical', 'high']:
                try:
                    url = f"https://api.github.com/repos/{repo}/issues"
                    headers = {'Authorization': f'token {gh_token}', 'Accept': 'application/vnd.github.v3+json'}
                    body = f"**Description:**\n{finding['description']}\n\n**Remediation:**\n{finding['remediation']}\n\n**Estimated Effort:**\n{finding['effort']}"
                    if 'file' in finding:
                        body += f"\n\n**Location:** `{finding['file']}:{finding.get('line', '')}`"
                    data = {'title': f"[SECURITY] {finding['title']}", 'body': body, 'labels': ['security', 'bug', finding['severity']]}
                    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
                    urllib.request.urlopen(req)
                except Exception:
                    pass

if __name__ == "__main__":
    main()
