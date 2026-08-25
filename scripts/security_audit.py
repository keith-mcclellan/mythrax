#!/usr/bin/env python3
import subprocess
import json
import re
from pathlib import Path
from datetime import datetime, timezone
import urllib.request
import time
import os
import shlex
import sys

def get_recent_commits(repo_url):
    repo_url = repo_url.replace('.git', '')
    if 'github.com' not in repo_url:
        return True

    parts = repo_url.split('github.com/')
    if len(parts) < 2:
         return True

    repo_path = parts[1]
    api_url = f"https://api.github.com/repos/{repo_path}/commits"

    try:
        req = urllib.request.Request(api_url, headers={'User-Agent': 'Mythrax-Security-Bot'})
        token = os.environ.get("GH_TOKEN")
        if token:
             req.add_header('Authorization', f'Bearer {token}')

        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            if not data or not isinstance(data, list):
                return False

            latest_commit_date = data[0]['commit']['author']['date']
            commit_time = datetime.fromisoformat(latest_commit_date.replace('Z', '+00:00'))

            if (datetime.now(timezone.utc) - commit_time).days > 365:
                return False
            return True
    except urllib.error.HTTPError as e:
        if e.code == 403 or e.code == 429:
             print(f"Rate limited fetching commits for {repo_url}")
             return True
        print(f"Error fetching commits for {repo_url}: {e}")
        return True
    except Exception as e:
        print(f"Error fetching commits for {repo_url}: {e}")
        return True

print("Starting security audit...")

cargo_audit_result = subprocess.run(["cargo", "audit", "--json"], capture_output=True, text=True, cwd="mythrax-core")
findings = []

try:
    if cargo_audit_result.stdout:
        data = json.loads(cargo_audit_result.stdout)

        vulns = data.get('vulnerabilities', {}).get('list', [])
        for vuln in vulns:
            advisory = vuln.get('advisory', {})
            package = advisory.get('package', 'unknown')
            patched_versions = vuln.get('versions', {}).get('patched', [])
            version = patched_versions[0] if patched_versions else 'latest'
            findings.append({
                'title': f"Vulnerable Dependency: {package} ({advisory.get('id', 'unknown')})",
                'severity': 'Critical' if 'critical' in advisory.get('title', '').lower() else 'High',
                'description': advisory.get('title', ''),
                'remediation': f"Upgrade `{package}` to version {version} or later.",
                'effort': 'Low'
            })
except json.JSONDecodeError as e:
    print(f"Error parsing cargo audit output: {e}")

secrets_pattern = re.compile(r'(?i)(password|token|secret|key|api)[ \t]*=[ \t]*[^ ]+')
for root, _, files in os.walk('.'):
    if '.git' in root or 'target' in root:
        continue
    for file in files:
        if file.endswith(('.rs', '.toml', '.json', '.yaml', '.yml', '.ini', '.env')):
            filepath = os.path.join(root, file)
            with open(filepath, 'r') as f:
                try:
                    for idx, line in enumerate(f):
                        if secrets_pattern.search(line):
                             if 'format!' in line or 'env::var' in line or 'Uuid::new_v4' in line or 'load_api_key' in line or '.and_then' in line or 'get_or_create_token' in line or '&keys[i]' in line or 'String::from_utf8' in line or 'None' in line or 'Some(' in line or 'uuid::' in line or 'auth::' in line or '::auth::' in line:
                                 continue

                             findings.append({
                                 'title': f"Hardcoded Secret in {file}",
                                 'severity': 'Critical',
                                 'description': f"Potential hardcoded secret found in `{filepath}` line {idx+1}: `{line.strip()}`",
                                 'remediation': "Replace with environment variable or vault reference.",
                                 'effort': 'Low'
                             })
                except UnicodeDecodeError:
                    pass


unsafe_pattern = re.compile(r'unsafe\s*\{')
for root, _, files in os.walk('mythrax-core/src'):
    for file in files:
        if file.endswith('.rs'):
            filepath = os.path.join(root, file)
            with open(filepath, 'r') as f:
                lines = f.readlines()
                for idx, line in enumerate(lines):
                    if unsafe_pattern.search(line):
                        start_idx = max(0, idx - 5)
                        end_idx = min(len(lines), idx + 10)
                        context = "".join(lines[start_idx:end_idx])

                        justified = False
                        justification = ""
                        for i in range(max(0, idx - 3), idx):
                            if 'SAFETY:' in lines[i] or 'Safety:' in lines[i]:
                                justified = True
                                justification = lines[i].strip()
                                break

                        if not justified:
                            findings.append({
                                'title': f"Undocumented Unsafe Block in {file}",
                                'severity': 'High',
                                'description': f"Unsafe block in `{filepath}` line {idx+1} lacks a documented `SAFETY:` comment.\n\nContext:\n```rust\n{context}\n```\n\nMemory Safety Risk: Unsafe blocks can bypass Rust's borrow checker and type system, potentially leading to memory corruption, undefined behavior, or security vulnerabilities if the invariants are not strictly upheld. Without documentation, it is impossible for reviewers to verify the soundness.",
                                'remediation': "Add a `// SAFETY: ...` comment explaining why the unsafe block is sound, detailing the invariants that must hold true, or remove if not needed.",
                                'effort': 'Medium'
                            })

command_pattern = re.compile(r'Command::new\(')
for root, _, files in os.walk('mythrax-core/src'):
    for file in files:
        if file.endswith('.rs'):
            filepath = os.path.join(root, file)
            with open(filepath, 'r') as f:
                for idx, line in enumerate(f):
                    if command_pattern.search(line):
                         findings.append({
                             'title': f"Potential Command Injection in {file}",
                             'severity': 'High',
                             'description': f"Command execution found in `{filepath}` line {idx+1}: `{line.strip()}`. Check if it accepts untrusted input.",
                             'remediation': "Ensure all arguments passed to `Command::new` are properly sanitized and do not rely on untrusted external input.",
                             'effort': 'Medium'
                         })

git_log_cmd = ["git", "log", "-p", "-E", "-i", "-G", "(password|token|secret|key|api)[[:space:]]*=[[:space:]]*[^ ]+"]
git_result = subprocess.run(git_log_cmd, capture_output=True, text=True, cwd="mythrax-core")
if git_result.stdout:
    findings.append({
        'title': "Secrets in Git History",
        'severity': 'Critical',
        'description': "Potential secrets found in git commit history.",
        'remediation': "Use BFG Repo-Cleaner or git filter-repo to rewrite history and remove secrets.",
        'effort': 'High'
    })

import tomli
dependencies_to_check = set()
try:
    with open('mythrax-core/Cargo.toml', 'rb') as f:
        toml_dict = tomli.load(f)
        for dep_type in ['dependencies', 'dev-dependencies', 'build-dependencies']:
            dependencies_to_check.update(toml_dict.get(dep_type, {}).keys())
except Exception as e:
    pass

try:
    with open('mythrax-core/Cargo.lock', 'rb') as f:
        lock_dict = tomli.load(f)
        for package in lock_dict.get('package', []):
            if 'name' in package:
                dependencies_to_check.add(package['name'])
except Exception as e:
    pass

for dep in dependencies_to_check:
    time.sleep(0.1)
    url = f"https://crates.io/api/v1/crates/{dep}"
    req = urllib.request.Request(url, headers={'User-Agent': 'Mythrax-Security-Bot'})
    try:
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            crate_data = data.get('crate', {})
            repo = crate_data.get('repository')

            versions = data.get('versions', [])
            if versions and versions[0].get('yanked'):
                 findings.append({
                     'title': f"Yanked Dependency: {dep}",
                     'severity': 'Medium',
                     'description': f"Dependency `{dep}` is using a yanked version.",
                     'remediation': f"Update `{dep}` to a non-yanked version.",
                     'effort': 'Low'
                 })

            if repo:
                if not get_recent_commits(repo):
                    findings.append({
                        'title': f"Stale Dependency: {dep}",
                        'severity': 'Low',
                        'description': f"Dependency `{dep}` has no recent commits in its repository (older than 1 year).",
                        'remediation': f"Evaluate alternatives or adopt `{dep}`.",
                        'effort': 'Medium'
                    })
    except urllib.error.HTTPError as e:
         pass

report_path = "security_advisory_report.md"
with open(report_path, "w") as f:
    f.write("# Mythrax Core Security Advisory Report\n\n")
    for idx, finding in enumerate(findings):
        f.write(f"## {idx+1}. [{finding['severity']}] {finding['title']}\n")
        f.write(f"**Description:** {finding['description']}\n\n")
        f.write(f"**Remediation:** {finding['remediation']}\n\n")
        f.write(f"**Estimated Effort:** {finding['effort']}\n\n")
        f.write("---\n\n")

print(f"Report generated at {report_path}")

has_gh_cli = subprocess.run(["which", "gh"], capture_output=True).returncode == 0
token = os.environ.get("GH_TOKEN")

for finding in findings:
    if finding['severity'] in ['Critical', 'High']:
        title = finding['title']
        body = f"**Description:** {finding['description']}\n\n**Remediation:** {finding['remediation']}\n\n**Estimated Effort:** {finding['effort']}"

        if has_gh_cli and token:
             try:
                 # Pass arguments as a list to avoid shell=True injection vulnerabilities
                 search_result = subprocess.run(["gh", "issue", "list", "--search", f'{title} in:title'], capture_output=True, text=True, check=True)
                 if title in search_result.stdout:
                     print(f"Issue already exists for: {title}")
                     continue

                 subprocess.run(["gh", "issue", "create", "--title", title, "--body", body], check=True)
                 print(f"Filed issue for: {title}")
             except subprocess.CalledProcessError as e:
                 print(f"Failed to file issue for {title}: {e}")
        else:
             print(f"Would file issue for: {title} (gh cli or GH_TOKEN not available in this environment)")
