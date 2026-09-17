import os
import re
import subprocess
import urllib.request
import json
import logging
from urllib.error import URLError
import time
from datetime import datetime

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Constants
MYTHRAX_CORE_DIR = "mythrax-core"
GITHUB_API_URL = "https://api.github.com"
REPO = os.environ.get("GITHUB_REPOSITORY")
TOKEN = os.environ.get("GITHUB_TOKEN")

SECRETS_REGEX = [
    r'(?i)api[_-]?key[\s:=]+[\"\'\'][a-zA-Z0-9_\-]{16,}[\"\'\']',
    r'(?i)secret[\s:=]+[\"\'\'][a-zA-Z0-9_\-]{16,}[\"\'\']',
    r'(?i)token[\s:=]+[\"\'\'][a-zA-Z0-9_\-]{16,}[\"\'\']',
    r'(?i)password[\s:=]+[\"\'\'][a-zA-Z0-9_\-\.\@]{8,}[\"\'\']',
    r'(?i)credentials?[\s:=]+[\"\'\'][a-zA-Z0-9_\-]{16,}[\"\'\']',
]

def check_secrets(directory):
    findings = []
    for root, _, files in os.walk(directory):
        for file in files:
            file_path = os.path.join(root, file)
            if '.git' in file_path or 'target' in file_path or 'Cargo.lock' in file_path:
                continue
            try:
                with open(file_path, 'r', encoding='utf-8') as f:
                    for i, line in enumerate(f):
                        for regex in SECRETS_REGEX:
                            if re.search(regex, line):
                                findings.append({
                                    'type': 'Hardcoded Secret',
                                    'file': file_path,
                                    'line': i + 1,
                                    'severity': 'Critical',
                                    'description': 'Potential hardcoded secret found.',
                                    'recommendation': 'Remove the hardcoded secret and use environment variables or a secret management system.',
                                    'effort': 'Low'
                                })
            except Exception:
                pass
    return findings

def check_git_history_secrets():
    findings = []
    try:
        # Check the last 100 commits, using git ls-tree and git show per memory to properly inspect historical contents.
        # However, the user asked to scan git history for secrets. The memory said: "When parsing `git log --patch` output (e.g., `git log -G`) to scan for secrets, avoid using `break` statements after finding the first match in a commit diff, as a single commit can contain multiple distinct secrets that all need to be flagged."

        # We can use git log -G to find secrets in git history efficiently
        for regex in SECRETS_REGEX:
            # git log -G expects POSIX ERE. Remove (?i) and map \s to [[:space:]]
            posix_regex = regex.replace('(?i)', '').replace(r'\s', '[[:space:]]')
            result = subprocess.run(['git', 'log', '-G', posix_regex, '-i', '--patch'], capture_output=True, text=True)
            lines = result.stdout.split('\n')
            current_commit = ""
            current_file = ""
            for line in lines:
                if line.startswith('commit '):
                    current_commit = line.split()[1]
                elif line.startswith('+++ b/'):
                    current_file = line[6:]
                elif line.startswith('+') and not line.startswith('+++'):
                    # Added line
                    if re.search(regex, line[1:]):
                        findings.append({
                            'type': 'Secret in Git History',
                            'file': current_file,
                            'commit': current_commit,
                            'severity': 'Critical',
                            'description': f'Potential secret found in git history at commit {current_commit[:8]}.',
                            'recommendation': 'Rotate the secret immediately and rewrite git history using tools like BFG Repo-Cleaner or git filter-repo.',
                            'effort': 'Medium'
                        })
    except Exception as e:
        logger.error(f"Failed to scan git history: {e}")
    return findings

def check_unsafe_blocks(directory):
    findings = []
    unsafe_regex = re.compile(r'\bunsafe\s*\{')
    for root, _, files in os.walk(directory):
        for file in files:
            if not file.endswith('.rs'):
                continue
            file_path = os.path.join(root, file)
            try:
                with open(file_path, 'r', encoding='utf-8') as f:
                    lines = f.readlines()
                    for i, line in enumerate(lines):
                        if unsafe_regex.search(line):
                            # Check if previous lines have comments
                            has_comment = False
                            for j in range(max(0, i - 3), i):
                                if '//' in lines[j] or '/*' in lines[j]:
                                    has_comment = True
                                    break

                            desc = 'Usage of unsafe block without documented justification comment. Unsafe Rust bypasses the compiler\'s memory safety guarantees, potentially introducing vulnerabilities such as use-after-free, buffer overflows, or data races.' if not has_comment else 'Usage of unsafe block. Unsafe Rust bypasses the compiler\'s memory safety guarantees, potentially introducing vulnerabilities such as use-after-free, buffer overflows, or data races.'
                            findings.append({
                                'type': 'Unsafe Rust Block',
                                'file': file_path,
                                'line': i + 1,
                                'severity': 'High' if not has_comment else 'Medium',
                                'description': desc,
                                'recommendation': 'Ensure memory safety. Provide a documented justification comment if missing.',
                                'effort': 'Medium',
                                'has_comment': has_comment
                            })
            except Exception:
                pass
    return findings

def check_dependencies(directory):
    findings = []

    cargo_lock_path = os.path.join(directory, 'Cargo.lock')
    if not os.path.exists(cargo_lock_path):
        return findings

    crates = []
    try:
        with open(cargo_lock_path, 'r', encoding='utf-8') as f:
            lines = f.readlines()
            current_name = None
            current_version = None
            for line in lines:
                if line.startswith('name = '):
                    parts = line.split('"')
                    if len(parts) >= 2:
                        current_name = parts[1]
                elif line.startswith('version = '):
                    parts = line.split('"')
                    if len(parts) >= 2:
                        current_version = parts[1]
                    if current_name and current_version:
                        crates.append((current_name, current_version))
                        current_name = None
                        current_version = None
    except Exception as e:
        logger.error(f"Failed to read Cargo.lock: {e}")

    # Remove duplicates
    crates = list(set(crates))

    # Check crates.io API for yanked status and recent commits (updated_at)
    for name, version in crates:
        try:
            url = f"https://crates.io/api/v1/crates/{name}/{version}"
            req = urllib.request.Request(url, headers={'User-Agent': 'Mythrax-Audit/1.0'})
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read().decode())
                version_info = data.get('version', {})

                # Check Yanked
                if version_info.get('yanked', False):
                    findings.append({
                        'type': 'Yanked Dependency',
                        'file': cargo_lock_path,
                        'severity': 'High',
                        'description': f'Crate {name} version {version} is yanked.',
                        'recommendation': f'Update {name} to a non-yanked version.',
                        'effort': 'Low'
                    })

                # Check recent commits (updated_at)
                updated_at_str = version_info.get('updated_at')
                if updated_at_str:
                    updated_at = datetime.strptime(updated_at_str.split('T')[0], "%Y-%m-%d")
                    months_diff = (datetime.now() - updated_at).days / 30
                    if months_diff > 24: # No commits in 2 years
                        findings.append({
                            'type': 'Stale Dependency',
                            'file': cargo_lock_path,
                            'severity': 'Medium',
                            'description': f'Crate {name} has no recent updates (last updated {updated_at_str}).',
                            'recommendation': f'Consider replacing {name} with an actively maintained alternative.',
                            'effort': 'Medium'
                        })

            time.sleep(0.1) # Be nice to the API
        except Exception as e:
            logger.debug(f"Failed to check crate {name}@{version}: {e}")

    # Check OSV.dev API for vulnerabilities
    for name, version in crates:
        try:
            osv_url = "https://api.osv.dev/v1/query"
            payload = json.dumps({
                "version": version,
                "package": {
                    "name": name,
                    "ecosystem": "crates.io"
                }
            }).encode('utf-8')
            req = urllib.request.Request(osv_url, data=payload, headers={'Content-Type': 'application/json'}, method='POST')
            with urllib.request.urlopen(req) as response:
                osv_data = json.loads(response.read().decode())
                if 'vulns' in osv_data:
                    for vuln in osv_data['vulns']:
                        findings.append({
                            'type': 'Dependency CVE',
                            'file': cargo_lock_path,
                            'severity': 'Critical',
                            'description': f"Crate {name}@{version} has vulnerability: {vuln.get('id', 'Unknown')} - {vuln.get('summary', 'No summary')}",
                            'recommendation': 'Update the vulnerable dependency.',
                            'effort': 'Low'
                        })
            time.sleep(0.1)
        except Exception as e:
            logger.debug(f"Failed to check OSV for {name}@{version}: {e}")

    return findings

def check_untrusted_input(directory):
    findings = []
    surreal_query_regex = re.compile(r'query\s*\(\s*format!\s*\(')
    for root, _, files in os.walk(directory):
        for file in files:
            if not file.endswith('.rs'):
                continue
            file_path = os.path.join(root, file)
            try:
                with open(file_path, 'r', encoding='utf-8') as f:
                    for i, line in enumerate(f):
                        if surreal_query_regex.search(line):
                            findings.append({
                                'type': 'Untrusted Execution Path',
                                'file': file_path,
                                'line': i + 1,
                                'severity': 'High',
                                'description': 'Potential NoSQL injection via formatted query strings.',
                                'recommendation': 'Use parameterized queries with $ placeholders and .bind().',
                                'effort': 'Low'
                            })
            except Exception:
                pass
    return findings

def file_github_issue(finding):
    if not REPO or not TOKEN:
        logger.warning("GITHUB_REPOSITORY or GITHUB_TOKEN not set. Skipping issue creation.")
        return

    url = f"{GITHUB_API_URL}/repos/{REPO}/issues?state=open&per_page=100"
    headers = {
        "Authorization": f"Bearer {TOKEN}",
        "Accept": "application/vnd.github.v3+json"
    }

    try:
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode())
    except URLError as e:
        logger.error(f"Failed to fetch issues: {e}")
        return

    issue_title = f"Security Audit Finding: {finding['type']} in {os.path.basename(finding.get('file', ''))}"
    for issue in issues:
        if issue.get('title') == issue_title:
            logger.info(f"Issue already exists for {finding['type']} in {finding.get('file')}. Skipping.")
            return

    body = f"## Security Advisory Report\n\n**Type:** {finding['type']}\n**Severity:** {finding['severity']}\n**File:** `{finding.get('file')}`\n**Line:** {finding.get('line', 'N/A')}\n\n### Description\n{finding['description']}\n\n### Recommendation\n{finding['recommendation']}\n\n**Estimated Effort:** {finding['effort']}"

    data = json.dumps({
        "title": issue_title,
        "body": body,
        "labels": ["security", "audit"]
    }).encode('utf-8')

    post_url = f"{GITHUB_API_URL}/repos/{REPO}/issues"
    try:
        req = urllib.request.Request(post_url, data=data, headers=headers, method='POST')
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                logger.info(f"Created issue for {finding['type']} in {finding.get('file')}")
            else:
                logger.error(f"Failed to create issue. Status: {response.status}")
    except URLError as e:
        logger.error(f"Failed to create issue: {e}")

def main():
    logger.info("Starting CISO Security Audit...")
    findings = []

    findings.extend(check_secrets(MYTHRAX_CORE_DIR))
    findings.extend(check_git_history_secrets())
    findings.extend(check_unsafe_blocks(MYTHRAX_CORE_DIR))
    findings.extend(check_dependencies(MYTHRAX_CORE_DIR))
    findings.extend(check_untrusted_input(MYTHRAX_CORE_DIR))

    report = {"findings": findings}

    print(json.dumps(report, indent=2))

    for finding in findings:
        if finding['severity'] in ['Critical', 'High']:
            file_github_issue(finding)

    logger.info("Audit complete.")

if __name__ == "__main__":
    main()
