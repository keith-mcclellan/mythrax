import os
import re
import json
import subprocess
import urllib.request
import tarfile

def run_cmd(cmd, cwd=None):
    res = subprocess.run(cmd, cwd=cwd, shell=True, text=True, capture_output=True)
    return res.stdout, res.stderr, res.returncode

def ensure_cargo_audit():
    stdout, _, rc = run_cmd("cargo audit --version")
    if rc != 0:
        url = "https://github.com/rustsec/rustsec/releases/download/cargo-audit%2Fv0.22.2/cargo-audit-x86_64-unknown-linux-gnu-v0.22.2.tgz"
        urllib.request.urlretrieve(url, "cargo-audit.tgz")
        with tarfile.open("cargo-audit.tgz", "r:gz") as tar:
            tar.extractall()
        os.environ["PATH"] += os.pathsep + os.path.abspath("cargo-audit-x86_64-unknown-linux-gnu-v0.22.2")

def audit():
    report = []

    # 1. Hardcoded Secrets
    secrets_re = re.compile(r'(?i)(api[_-]?key|secret|token|password|cred)[\s:=]+[\'"]([A-Za-z0-9_\-]{8,})[\'"]')
    for root, _, files in os.walk('mythrax-core'):
        if 'target' in root: continue
        for file in files:
            if file.endswith('.rs') or file.endswith('.toml'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r', encoding='utf-8') as f:
                        content = f.read()
                        for match in secrets_re.finditer(content):
                            val = match.group(2)
                            if val in ['test_key', 'test_secret', 'dummy', '••••••••']: continue
                            if 'test' in val.lower(): continue
                            report.append({
                                'type': 'Hardcoded Secret',
                                'severity': 'Critical',
                                'file': filepath,
                                'match': match.group(0),
                                'recommendation': 'Move secret to environment variable or secure vault.',
                                'effort': 'Low'
                            })
                except Exception:
                    pass

    # 2. Unsafe Rust
    unsafe_re = re.compile(r'unsafe\s*\{')
    for root, _, files in os.walk('mythrax-core/src'):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r', encoding='utf-8') as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            if unsafe_re.search(line):
                                has_comment = False
                                for j in range(max(0, i-3), i):
                                    if '// safety:' in lines[j].lower():
                                        has_comment = True
                                        break

                                report.append({
                                    'type': 'Unsafe Rust Block',
                                    'severity': 'High' if not has_comment else 'Low',
                                    'file': filepath,
                                    'line': i + 1,
                                    'risk': 'Potential memory corruption, use-after-free, or undefined behavior if invariants are violated.',
                                    'recommendation': 'Add a // SAFETY: comment explaining why this block is safe, or refactor to safe Rust.' if not has_comment else 'Maintain invariants documented in the safety comment.',
                                    'effort': 'Medium'
                                })
                except Exception:
                    pass

    # 3. Execution Paths (Sanitization)
    for root, _, files in os.walk('mythrax-core/src/mcp_routes'):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r', encoding='utf-8') as f:
                        content = f.read()
                        if 'Command::new' in content and 'args.get' in content:
                            report.append({
                                'type': 'Unsanitized External Input (Command Injection Risk)',
                                'severity': 'Critical',
                                'file': filepath,
                                'recommendation': 'Ensure all inputs passed to Command::new are strictly validated against an allowlist.',
                                'effort': 'Medium'
                            })
                except Exception:
                    pass

    # 4. Git history secrets
    stdout, _, rc = run_cmd("git log -p mythrax-core")
    if rc == 0:
        found_secrets = set()
        for match in secrets_re.finditer(stdout):
            val = match.group(2)
            if val in ['test_key', 'test_secret', 'dummy', '••••••••']: continue
            if 'test' in val.lower(): continue
            if val not in found_secrets:
                found_secrets.add(val)
                report.append({
                    'type': 'Secret in Git History',
                    'severity': 'Critical',
                    'match': match.group(0),
                    'recommendation': 'Rotate secret and rewrite git history to remove it.',
                    'effort': 'High'
                })

    # 5. Parse Cargo Audit
    ensure_cargo_audit()
    run_cmd("cargo audit --json > audit_output.json", cwd="mythrax-core")
    try:
        with open("mythrax-core/audit_output.json", "r") as f:
            data = json.load(f)

        vulnerabilities = data.get("vulnerabilities", {})
        if isinstance(vulnerabilities, dict):
            vuln_list = vulnerabilities.get("list", [])
        else:
            vuln_list = []

        for v in vuln_list:
            report.append({
                'type': 'Vulnerable Dependency',
                'severity': 'High',
                'package': v.get('package', {}).get('name'),
                'title': v.get('advisory', {}).get('title'),
                'recommendation': 'Update the vulnerable dependency to a patched version.',
                'effort': 'Low'
            })

        warnings = data.get("warnings", {})
        for kind, w_list in warnings.items():
            if isinstance(w_list, list):
                for w in w_list:
                     report.append({
                        'type': f'Dependency Warning ({kind})',
                        'severity': 'Medium',
                        'package': w.get('package', {}).get('name'),
                        'recommendation': f'Investigate {kind} dependency.',
                        'effort': 'Low'
                    })
    except Exception as e:
        print("Failed to parse audit JSON:", e)

    # Output Markdown Report
    with open('security_advisory_report.md', 'w') as f:
        f.write("# Security Advisory Report\n\n")

        f.write("## Critical / High Findings\n\n")
        for r in report:
            if r['severity'] in ['Critical', 'High']:
                f.write(f"### [{r['severity']}] {r['type']}\n")
                for k, v in r.items():
                    if k not in ['type', 'severity']:
                        f.write(f"- **{k.capitalize()}**: `{v}`\n")
                f.write("\n")

        f.write("## Medium / Low Findings\n\n")
        for r in report:
            if r['severity'] not in ['Critical', 'High']:
                f.write(f"### [{r['severity']}] {r['type']}\n")
                for k, v in r.items():
                    if k not in ['type', 'severity']:
                        f.write(f"- **{k.capitalize()}**: `{v}`\n")
                f.write("\n")

    # GitHub Issues
    is_github_actions = os.environ.get('GITHUB_ACTIONS') == 'true'
    if not is_github_actions:
        os.makedirs('issues', exist_ok=True)

    issue_count = 1
    for r in report:
        if r['severity'] in ['Critical', 'High']:
            title_suffix = ""
            if 'file' in r:
                title_suffix = f" in {os.path.basename(r['file'])}"
            elif 'package' in r:
                title_suffix = f" for {r['package']}"
            else:
                title_suffix = f" {issue_count}"

            title = f"{r['severity']} Security Finding: {r['type']}{title_suffix}"
            title = title[:100]

            body = f"# {r['severity']} Security Finding: {r['type']}\n\n"
            for k, v in r.items():
                if k not in ['type', 'severity']:
                    body += f"**{k.capitalize()}**: `{v}`\n"

            if is_github_actions:
                 try:
                     search_res = subprocess.check_output(['gh', 'issue', 'list', '--state', 'all', '--search', title], text=True)
                     if title in search_res:
                         continue
                 except Exception:
                     pass

                 try:
                    subprocess.run(['gh', 'issue', 'create', '--title', title, '--body', body], check=True)
                 except Exception as e:
                    print(f"Failed to create GitHub Issue: {e}")
            else:
                with open(f'issues/issue_{issue_count}.md', 'w') as f:
                    f.write(body)
                issue_count += 1

if __name__ == '__main__':
    audit()
