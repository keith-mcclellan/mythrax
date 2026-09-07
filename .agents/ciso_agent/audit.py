import os
import sys
import subprocess
import urllib.request
import json
import re
import toml
from datetime import datetime, timezone

def run_cmd(cmd, cwd=None, fail_on_error=True):
    try:
        result = subprocess.run(cmd, cwd=cwd, shell=True, check=fail_on_error, capture_output=True, text=True)
        return result.stdout
    except subprocess.CalledProcessError as e:
        if fail_on_error:
            print(f"Command failed: {cmd}\nOutput: {e.output}\nError: {e.stderr}", file=sys.stderr)
            sys.exit(1)
        return e.output + e.stderr

def check_dependencies():
    findings = []
    try:
        cargo_path = "mythrax-core/Cargo.toml"
        if not os.path.exists(cargo_path):
            return findings

        with open(cargo_path, "r") as f:
            cargo_toml = toml.load(f)

        deps = cargo_toml.get("dependencies", {})
        deps.update(cargo_toml.get("dev-dependencies", {}))
        deps.update(cargo_toml.get("build-dependencies", {}))

        for crate_name, crate_info in deps.items():
            if not isinstance(crate_name, str):
                continue
            if isinstance(crate_info, dict) and ('path' in crate_info or 'git' in crate_info):
                continue

            try:
                url = f"https://crates.io/api/v1/crates/{crate_name}"
                req = urllib.request.Request(url, headers={'User-Agent': 'Mythrax-CISO-Agent'})
                with urllib.request.urlopen(req) as response:
                    data = json.loads(response.read().decode())
                    crate_data = data.get("crate", {})
                    versions = data.get("versions", [])

                    if versions:
                        latest = versions[0]
                        if latest.get("yanked"):
                            findings.append({
                                "severity": "High",
                                "title": f"Yanked Dependency: {crate_name}",
                                "description": f"The dependency `{crate_name}` has been yanked from crates.io.",
                                "remediation": "Remove the dependency or update to a non-yanked version.",
                                "effort": "Low"
                            })

                    updated_at_str = crate_data.get("updated_at")
                    if updated_at_str:
                        clean_str = updated_at_str.split('.')[0].replace('Z', '')
                        dt = datetime.fromisoformat(clean_str)
                        dt = dt.replace(tzinfo=timezone.utc)
                        now = datetime.now(timezone.utc)
                        diff = now - dt

                        if diff.days > 730:
                            findings.append({
                                "severity": "Medium",
                                "title": f"Stale Dependency: {crate_name}",
                                "description": f"The crate `{crate_name}` has not been updated since {clean_str} (>2 years).",
                                "remediation": "Evaluate alternatives as this may be an abandoned project.",
                                "effort": "Medium"
                            })
            except Exception:
                pass
    except Exception as e:
        print(f"Error checking dependencies: {e}", file=sys.stderr)

    return findings

def audit_rustsec():
    findings = []
    output = run_cmd("cd mythrax-core && cargo audit --json", fail_on_error=False)
    if output:
        for line in output.splitlines():
            if line.startswith('{'):
                try:
                    data = json.loads(line)
                    vulnerabilities = data.get("vulnerabilities", {}).get("list", [])
                    for vuln in vulnerabilities:
                        adv = vuln.get("advisory", {})
                        cvss = adv.get("cvss")
                        score = float(cvss.get("score", 0)) if cvss else 0
                        findings.append({
                            "severity": "Critical" if score >= 9.0 else "High",
                            "title": f"RustSec Advisory: {adv.get('id')} in {vuln.get('package', {}).get('name')}",
                            "description": adv.get("description", "Vulnerability found"),
                            "remediation": f"Upgrade to a patched version: {', '.join(adv.get('patched_versions', []))}",
                            "effort": "Low"
                        })
                except:
                    pass
    return findings

def find_hardcoded_secrets():
    findings = []
    patterns = [
        r"AKIA[0-9A-Z]{16}",
        r"ghp_[0-9a-zA-Z]{36}",
        r"sk_live_[0-9a-zA-Z]{24}",
        r"xox[baprs]-[0-9a-zA-Z]{10,48}"
    ]

    for root, dirs, files in os.walk("mythrax-core"):
        if "target" in dirs: dirs.remove("target")
        for file in files:
            if not file.endswith(('.rs', '.toml', '.json', '.yaml', '.md', '.conf')):
                continue
            filepath = os.path.join(root, file)
            try:
                with open(filepath, 'r', encoding='utf-8') as f:
                    content = f.read()
                for pattern in patterns:
                    for match in re.finditer(pattern, content):
                        line_start = content.rfind('\n', 0, match.start()) + 1
                        line_end = content.find('\n', match.end())
                        if line_end == -1: line_end = len(content)
                        line = content[line_start:line_end].strip()

                        findings.append({
                            "severity": "Critical",
                            "title": f"Hardcoded Secret in {os.path.basename(filepath)}",
                            "description": f"Potential secret found in `{filepath}`:\n```\n{line}\n```",
                            "remediation": "Remove the hardcoded secret and use environment variables.",
                            "effort": "Low"
                        })
            except Exception:
                pass
    return findings

def find_unsafe_rust():
    findings = []
    output = run_cmd("grep -rn 'unsafe {' mythrax-core/src", fail_on_error=False)
    if output:
        for line in output.splitlines():
            if not line.strip(): continue
            parts = line.split(":", 2)
            if len(parts) >= 3:
                file_path, line_num = parts[0], parts[1]
                has_safety_comment = False
                try:
                    with open(file_path, 'r', encoding='utf-8') as f:
                        lines = f.readlines()
                        idx = int(line_num) - 1
                        start_idx = max(0, idx - 5)
                        for i in range(start_idx, idx):
                            if "SAFETY:" in lines[i] or "Safety:" in lines[i]:
                                has_safety_comment = True
                                break
                except:
                    pass

                if not has_safety_comment:
                    findings.append({
                        "severity": "High",
                        "title": f"Undocumented unsafe block in {os.path.basename(file_path)}:{line_num}",
                        "description": f"Found `unsafe {{}}` block at `{file_path}:{line_num}` without `// SAFETY:` justification. Elevated memory safety risk.",
                        "remediation": "Add a `// SAFETY:` comment explaining why it is sound, or refactor to safe Rust.",
                        "effort": "Medium"
                    })
    return findings

def find_git_history_secrets():
    findings = []
    regex = r"AKIA|ghp_|sk_live_|xox[baprs]-"
    cmd = f"git log -G '{regex}' --format='%H' --name-status"
    commits = set()
    try:
        with subprocess.Popen(cmd, shell=True, stdout=subprocess.PIPE, text=True) as proc:
            for line in proc.stdout:
                line = line.strip()
                if line and len(line) == 40:
                    commits.add(line)
                    if len(commits) > 5:
                        break
    except Exception as e:
        print(f"Error scanning git history: {e}")

    if commits:
        findings.append({
            "severity": "Critical",
            "title": "Potential Secrets in Git History",
            "description": f"Found potential secrets matching known patterns in git history. Commits include: {', '.join(list(commits)[:5])}",
            "remediation": "Rotate all exposed credentials and rewrite git history.",
            "effort": "High"
        })
    return findings

def find_untrusted_input():
    findings = []
    # Use -B 2 and -A 2 to capture surrounding lines to check for format!
    output = run_cmd("grep -rn -A 2 -B 2 'db.query(' mythrax-core/src | grep 'format!'", fail_on_error=False)
    if output and output.strip():
        files = set()
        for line in output.splitlines():
            parts = line.split(":", 1)
            if len(parts) >= 2 and (parts[0].endswith('.rs') or parts[0].endswith('.rs-')):
                files.add(parts[0].replace('-', ''))

        for file_path in files:
            findings.append({
                "severity": "High",
                "title": f"NoSQL Injection Risk in {os.path.basename(file_path)}",
                "description": f"Found `format!` used with `db.query()` in `{file_path}`. Risk of NoSQL injection if input is untrusted.",
                "remediation": "Use parameterized queries with `$` placeholders and `.bind((\"var\", value))`.",
                "effort": "Medium"
            })
    return findings

def get_existing_issues():
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo: return []
    try:
        url = f"https://api.github.com/repos/{repo}/issues?state=open"
        req = urllib.request.Request(url, headers={'Authorization': f'token {token}', 'User-Agent': 'Mythrax-CISO'})
        with urllib.request.urlopen(req) as response:
            return [issue['title'] for issue in json.loads(response.read().decode())]
    except Exception:
        return []

def file_github_issue(finding):
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo: return

    existing = get_existing_issues()
    if finding['title'] in existing: return

    try:
        url = f"https://api.github.com/repos/{repo}/issues"
        body = f"**CISO Audit Finding**\n\n**Severity:** {finding['severity']}\n**Effort:** {finding['effort']}\n\n**Description:**\n{finding['description']}\n\n**Remediation:**\n{finding['remediation']}"
        data = {"title": finding['title'], "body": body, "labels": ["security", finding['severity'].lower()]}

        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers={
            'Authorization': f'token {token}', 'Accept': 'application/vnd.github.v3+json',
            'Content-Type': 'application/json', 'User-Agent': 'Mythrax-CISO'
        })
        urllib.request.urlopen(req)
    except Exception as e:
        print(f"Error filing issue: {e}", file=sys.stderr)

def main():
    findings = []
    findings.extend(check_dependencies())
    findings.extend(audit_rustsec())
    findings.extend(find_hardcoded_secrets())
    findings.extend(find_unsafe_rust())
    findings.extend(find_git_history_secrets())
    findings.extend(find_untrusted_input())

    severity_rank = {"Critical": 1, "High": 2, "Medium": 3, "Low": 4}
    findings.sort(key=lambda x: severity_rank.get(x['severity'], 5))

    print("# CISO Security Advisory Report\n")
    for f in findings:
        print(f"## [{f['severity']}] {f['title']}")
        print(f"**Description:** {f['description']}")
        print(f"**Remediation:** {f['remediation']}")
        print(f"**Effort:** {f['effort']}\n")

        if f['severity'] in ["Critical", "High"]:
            file_github_issue(f)

if __name__ == "__main__":
    main()
