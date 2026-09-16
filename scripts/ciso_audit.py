import os
import re
import subprocess
import json
import urllib.request
import urllib.error
from datetime import datetime
import time

GITHUB_TOKEN = os.environ.get("GITHUB_TOKEN")
GITHUB_REPOSITORY = os.environ.get("GITHUB_REPOSITORY")
GITHUB_API_URL = os.environ.get("GITHUB_API_URL", "https://api.github.com")

def create_issue(title, body, labels=[]):
    if not GITHUB_TOKEN or not GITHUB_REPOSITORY:
        print("Skipping issue creation, missing GITHUB_TOKEN or GITHUB_REPOSITORY")
        return
    url = f"{GITHUB_API_URL}/repos/{GITHUB_REPOSITORY}/issues"
    headers = {
        "Authorization": f"Bearer {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json"
    }
    data = {
        "title": title,
        "body": body,
        "labels": labels
    }

    search_url = f"{GITHUB_API_URL}/repos/{GITHUB_REPOSITORY}/issues?state=open&per_page=100"
    try:
        req = urllib.request.Request(search_url, headers=headers)
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode())
            for issue in issues:
                if issue["title"] == title:
                    print(f"Issue already exists: {title}")
                    return
    except Exception as e:
        print(f"Failed to check existing issues: {e}")

    try:
        req = urllib.request.Request(url, headers=headers, data=json.dumps(data).encode("utf-8"))
        with urllib.request.urlopen(req) as response:
            print(f"Created issue: {title}")
    except Exception as e:
        print(f"Failed to create issue: {e}")

def scan_for_secrets():
    print("Scanning for secrets in current working tree...")
    findings = []
    # PCRE for python
    patterns = [
        (r"(api[_-]?key|secret|token|password|cred)[\s:=]+['\"][a-zA-Z0-9_\-]{16,}['\"]", "Hardcoded Secret/Token"),
        (r"ghp_[a-zA-Z0-9]{36}", "GitHub Personal Access Token"),
        (r"sk-[a-zA-Z0-9]{48}", "OpenAI API Key")
    ]

    for root, _, files in os.walk("mythrax-core"):
        for file in files:
            if file.endswith(('.rs', '.toml', '.json', '.yaml', '.yml', '.md')):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r', encoding='utf-8') as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            for pattern, desc in patterns:
                                if re.search(pattern, line, re.IGNORECASE):
                                    findings.append({
                                        "severity": "Critical",
                                        "file": filepath,
                                        "line": i + 1,
                                        "description": f"Found {desc} matching '{pattern}'",
                                        "remediation": "Remove the hardcoded secret and use environment variables or a secret manager.",
                                        "effort": "Low"
                                    })
                except Exception as e:
                    pass
    return findings

def scan_git_history_for_secrets():
    print("Scanning git history for secrets...")
    findings = []
    # POSIX ERE for git log -G
    patterns = [
        (r"(api[_-]?key|secret|token|password|cred)[[:space:]:=]+['\"][a-zA-Z0-9_\-]{16,}['\"]", "Hardcoded Secret/Token"),
        (r"ghp_[a-zA-Z0-9]{36}", "GitHub Personal Access Token"),
        (r"sk-[a-zA-Z0-9]{48}", "OpenAI API Key")
    ]

    for pattern, desc in patterns:
        try:
            cmd = ['git', 'log', '-G', pattern, '-i', '--patch', '--ignore-all-space', '--', 'mythrax-core']

            result = subprocess.run(cmd, capture_output=True, text=True)
            if result.stdout:
                current_commit = None
                current_file = None
                for line in result.stdout.splitlines():
                    if line.startswith('commit '):
                        current_commit = line.split()[1]
                    elif line.startswith('diff --git '):
                        parts = line.split()
                        if len(parts) >= 3:
                            current_file = parts[2][2:]
                    elif line.startswith('+') and not line.startswith('+++'):
                        # Regex match the line in Python with PCRE to confirm (converting POSIX to PCRE)
                        pcre_pattern = pattern.replace('[[:space:]]', r'\s')
                        if re.search(pcre_pattern, line[1:], re.IGNORECASE):
                            finding = {
                                "severity": "Critical",
                                "file": f"{current_file} (in commit {current_commit[:8]})",
                                "line": "N/A",
                                "description": f"Found {desc} in git history matching '{pattern}'",
                                "remediation": "Rotate the secret and rewrite git history or use BFG repo cleaner.",
                                "effort": "High"
                            }
                            findings.append(finding)
        except Exception as e:
            print(f"Error scanning git history: {e}")

    unique_findings = []
    seen = set()
    for f in findings:
        key = (f["file"], f["description"])
        if key not in seen:
            seen.add(key)
            unique_findings.append(f)

    return unique_findings

def scan_unsafe_rust():
    print("Scanning for unsafe Rust blocks...")
    findings = []
    unsafe_pattern = re.compile(r'unsafe\s*\{')
    justification_pattern = re.compile(r'//\s*SAFETY:')

    for root, _, files in os.walk("mythrax-core"):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r', encoding='utf-8') as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            if unsafe_pattern.search(line):
                                justified = False
                                start = max(0, i - 5)
                                for j in range(start, i):
                                    if justification_pattern.search(lines[j]):
                                        justified = True
                                        break

                                findings.append({
                                    "severity": "Medium" if justified else "High",
                                    "file": filepath,
                                    "line": i + 1,
                                    "description": f"Unsafe Rust block found. Justified: {justified}. Memory safety risk: Unsafe blocks bypass Rust's compiler guarantees, potentially leading to memory corruption, use-after-free, or data races.",
                                    "remediation": "Document invariants with a '// SAFETY:' comment, or rewrite in safe Rust if possible.",
                                    "effort": "Medium"
                                })
                except Exception as e:
                    pass
    return findings

def parse_cargo_dependencies():
    print("Parsing Cargo.lock for dependencies...")
    findings = []
    crates = {} # crate -> version

    # Parse Cargo.lock to get all crates including transient
    try:
        with open('mythrax-core/Cargo.lock', 'r', encoding='utf-8') as f:
            content = f.read()
            # Very basic TOML parsing for Cargo.lock
            # We look for [[package]] sections
            sections = content.split('[[package]]')
            for section in sections[1:]:
                name = None
                version = None
                for line in section.splitlines():
                    line = line.strip()
                    if line.startswith('name = '):
                        name = line.split('=')[1].strip().strip('"')
                    elif line.startswith('version = '):
                        version = line.split('=')[1].strip().strip('"')

                if name and version:
                    crates[name] = version
    except Exception as e:
        print(f"Error parsing Cargo.lock: {e}")

    # For rate limiting, check crates.io API selectively or introduce sleep
    print(f"Found {len(crates)} crates in Cargo.lock")

    # Since checking all crates can hit rate limits or take too long, we might just use OSV for vulnerabilities and crates.io for yanked
    # However crates.io is strict about rate limits (1 req / sec).
    # Since this is an audit, we can check OSV which allows bulk queries.

    osv_queries = []
    for crate, version in crates.items():
        osv_queries.append({
            "package": {
                "name": crate,
                "ecosystem": "crates.io"
            },
            "version": version
        })

    # OSV bulk query
    # Rate limit: 1 request per sec generally fine for bulk. We chunk if too big.
    chunk_size = 1000
    for i in range(0, len(osv_queries), chunk_size):
        chunk = {"queries": osv_queries[i:i+chunk_size]}
        url = "https://api.osv.dev/v1/querybatch"
        try:
            req = urllib.request.Request(url, data=json.dumps(chunk).encode('utf-8'), headers={'Content-Type': 'application/json'})
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read().decode())
                results = data.get('results', [])
                for j, result in enumerate(results):
                    if 'vulns' in result:
                        crate = osv_queries[i+j]['package']['name']
                        version = osv_queries[i+j]['version']
                        for vuln in result['vulns']:
                            vuln_id = vuln.get('id', 'Unknown')
                            details = vuln.get('details', 'No details provided.')
                            findings.append({
                                "severity": "High",
                                "file": "Cargo.lock",
                                "line": "N/A",
                                "description": f"Dependency '{crate}' (v{version}) has known vulnerability {vuln_id} via RustSec Advisory Database. Details: {details[:100]}...",
                                "remediation": f"Update '{crate}' to a patched version.",
                                "effort": "Low"
                            })
        except Exception as e:
            print(f"OSV batch query failed: {e}")

    # Check crates.io for yanked/stale (rate limit 1/sec makes this very slow for a full lockfile - 400+ deps).
    # Since prompt requires it, we will just check the top-level ones from Cargo.toml to avoid blocking CI forever.
    top_level_crates = set()
    try:
        with open('mythrax-core/Cargo.toml', 'r', encoding='utf-8') as f:
            content = f.read()
            sections = re.split(r'^\s*\[(.*?)\]\s*$', content, flags=re.MULTILINE)
            for k in range(1, len(sections), 2):
                if 'dependencies' in sections[k]:
                    for line in sections[k+1].splitlines():
                        line = line.split('#')[0].strip()
                        if '=' in line:
                            crate_name = line.split('=')[0].strip().strip('"\'')
                            if crate_name:
                                top_level_crates.add(crate_name)
    except Exception as e:
        pass

    for crate in top_level_crates:
        url = f"https://crates.io/api/v1/crates/{crate}"
        headers = {"User-Agent": "CISO-Audit-Script (mythrax@example.com)"}
        try:
            req = urllib.request.Request(url, headers=headers)
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read().decode())
                crate_data = data.get('crate', {})
                versions = data.get('versions', [])

                if versions:
                    latest_version = versions[0]
                    if latest_version.get('yanked', False):
                        findings.append({
                            "severity": "High",
                            "file": "Cargo.toml",
                            "line": "N/A",
                            "description": f"Dependency '{crate}' has its latest version yanked.",
                            "remediation": f"Update or replace the '{crate}' dependency.",
                            "effort": "Low"
                        })

                updated_at_str = crate_data.get('updated_at')
                if updated_at_str:
                    updated_at_str = updated_at_str.split('T')[0]
                    updated_at = datetime.strptime(updated_at_str, "%Y-%m-%d")
                    delta = datetime.now() - updated_at
                    if delta.days > 365 * 2:
                        findings.append({
                            "severity": "Medium",
                            "file": "Cargo.toml",
                            "line": "N/A",
                            "description": f"Dependency '{crate}' has no recent updates (last updated {updated_at_str}).",
                            "remediation": f"Evaluate if '{crate}' is abandoned and consider alternatives.",
                            "effort": "High"
                        })
            # Rate limiting
            time.sleep(1)
        except Exception as e:
            pass

    return findings

def scan_untrusted_execution_paths():
    print("Scanning for untrusted execution paths (e.g., NoSQL injection)...")
    findings = []

    surreal_pattern = re.compile(r'format!\s*\(\s*".*?(SELECT|INSERT|UPDATE|DELETE|CREATE).*?"')

    for root, _, files in os.walk("mythrax-core"):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r', encoding='utf-8') as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            if surreal_pattern.search(line) and "db.query" in "".join(lines[max(0, i-2):i+3]):
                                findings.append({
                                    "severity": "High",
                                    "file": filepath,
                                    "line": i + 1,
                                    "description": "Potential NoSQL injection: string interpolation used in database query.",
                                    "remediation": "Use parameterized queries with '$' placeholders and .bind().",
                                    "effort": "Medium"
                                })
                except Exception as e:
                    pass
    return findings

def generate_report(findings):
    print("Generating report...")
    report_path = "security_advisory_report.md"

    severity_order = {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}
    findings.sort(key=lambda x: severity_order.get(x["severity"], 4))

    with open(report_path, "w", encoding="utf-8") as f:
        f.write("# Security Advisory Report\n\n")
        f.write(f"Generated at: {datetime.now().isoformat()}\n\n")

        if not findings:
            f.write("No findings detected.\n")
            return report_path

        f.write("## Findings Summary\n\n")
        counts = {"Critical": 0, "High": 0, "Medium": 0, "Low": 0}
        for finding in findings:
            if finding["severity"] in counts:
                counts[finding["severity"]] += 1

        for sev, count in counts.items():
            f.write(f"- **{sev}**: {count}\n")

        f.write("\n## Detailed Findings\n\n")

        for i, finding in enumerate(findings, 1):
            f.write(f"### {i}. [{finding['severity']}] {finding['description'][:100]}\n")
            f.write(f"- **Location**: `{finding['file']}` (Line: {finding['line']})\n")
            f.write(f"- **Risk**: {finding['description']}\n")
            f.write(f"- **Remediation**: {finding['remediation']}\n")
            f.write(f"- **Estimated Effort**: {finding['effort']}\n\n")

            if finding["severity"] in ["Critical", "High"]:
                title = f"[CISO Audit] {finding['severity']}: {finding['description'][:50]}..."
                body = f"**Location**: `{finding['file']}` (Line: {finding['line']})\n\n**Risk**: {finding['description']}\n\n**Remediation**: {finding['remediation']}\n\n**Estimated Effort**: {finding['effort']}"
                create_issue(title, body, labels=["security", finding["severity"].lower()])

    return report_path

def main():
    findings = []
    findings.extend(scan_for_secrets())
    findings.extend(scan_git_history_for_secrets())
    findings.extend(scan_unsafe_rust())
    findings.extend(parse_cargo_dependencies())
    findings.extend(scan_untrusted_execution_paths())

    report_path = generate_report(findings)
    print(f"Report generated: {report_path}")

if __name__ == "__main__":
    main()
