#!/usr/bin/env python3
import os
import sys
import json
import subprocess
import urllib.request
import re
import time
from datetime import datetime, timedelta, timezone

def load_ciso_prompt():
    config_path = ".agents/agents/ciso_agent/config.json"
    with open(config_path, "r") as f:
        config = json.load(f)
    return config["config"]["customAgent"]["systemPromptSections"][0]["content"]

def get_modified_files():
    base_ref = os.environ.get("GITHUB_BASE_REF")
    if not base_ref:
        return None
    try:
        subprocess.check_call(["git", "fetch", "origin", base_ref])
        result = subprocess.check_output(["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"])
        return [f for f in result.decode("utf-8").splitlines() if f.endswith(".rs") or f == "Cargo.toml"]
    except subprocess.CalledProcessError:
        return None

def chunk_files(files):
    chunks = []
    current_chunk = ""
    for f in files:
        if not os.path.exists(f):
            continue
        try:
            with open(f, "r", encoding="utf-8") as file:
                content = file.read()
                file_repr = f"\n\n--- {f} ---\n{content}"
                if len(current_chunk) + len(file_repr) > 100000:
                    chunks.append(current_chunk)
                    current_chunk = file_repr
                else:
                    current_chunk += file_repr
        except Exception:
            pass
    if current_chunk:
        chunks.append(current_chunk)
    return chunks

def call_llm(prompt, chunk):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("Warning: OPENAI_API_KEY not found. Skipping LLM analysis.", file=sys.stderr)
        return {"findings": []}

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    data = {
        "model": "gpt-4o-mini",
        "response_format": {"type": "json_object"},
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": f"Analyze this code and return a JSON object with a 'findings' array: {chunk}"}
        ]
    }

    req = urllib.request.Request(url, json.dumps(data).encode("utf-8"), headers)
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode("utf-8"))
            return json.loads(result["choices"][0]["message"]["content"])
    except Exception as e:
        print(f"LLM API Error: {e}", file=sys.stderr)
        return {"findings": []}

def run_cargo_audit():
    findings = []
    try:
        os.chdir("mythrax-core")
        output = subprocess.check_output(["cargo", "audit", "--json"])
        os.chdir("..")
        audit_res = json.loads(output)

        for vuln in audit_res.get("vulnerabilities", {}).get("list", []):
            findings.append({
                "severity": "High",
                "title": f"Vulnerability in {vuln['advisory']['package']}",
                "file": "Cargo.lock",
                "line": "N/A",
                "description": f"CVE: {vuln['advisory'].get('id', 'Unknown')}. {vuln['advisory']['title']}",
                "remediation": "Update the crate to a patched version.",
                "effort": "Low"
            })
    except subprocess.CalledProcessError as e:
        os.chdir("..")
        try:
            audit_res = json.loads(e.output)
            for vuln in audit_res.get("vulnerabilities", {}).get("list", []):
                findings.append({
                    "severity": "High",
                    "title": f"Vulnerability in {vuln['advisory']['package']}",
                    "file": "Cargo.lock",
                    "line": "N/A",
                    "description": f"CVE: {vuln['advisory'].get('id', 'Unknown')}. {vuln['advisory']['title']}",
                    "remediation": "Update the crate to a patched version.",
                    "effort": "Low"
                })
        except Exception:
            pass
    except FileNotFoundError:
         print("Warning: cargo-audit not installed. Skipping.", file=sys.stderr)

    return findings

def check_crates_io(package, version):
    try:
        url = f"https://crates.io/api/v1/crates/{package}/{version}"
        req = urllib.request.Request(url, headers={'User-Agent': 'Mythrax-CISO-Audit'})
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode("utf-8"))
            is_yanked = data.get("version", {}).get("yanked", False)
            created_at_str = data.get("version", {}).get("created_at")

            stale = False
            if created_at_str:
                created_at = datetime.fromisoformat(created_at_str.replace('Z', '+00:00'))
                if datetime.now(timezone.utc) - created_at > timedelta(days=365 * 3): # 3 years
                    stale = True

            return is_yanked, stale
    except Exception as e:
        return False, False

def audit_dependencies():
    findings = []
    try:
        with open("mythrax-core/Cargo.toml", "r") as f:
            content = f.read()

        deps = re.findall(r'^([a-zA-Z0-9_-]+)\s*=\s*(?:\{.*version\s*=\s*|)"([^"]+)"', content, re.MULTILINE)
        for pkg, ver in deps:
            yanked, stale = check_crates_io(pkg, ver)
            time.sleep(1)
            if yanked:
                findings.append({
                    "severity": "High",
                    "title": f"Yanked crate dependency: {pkg}",
                    "file": "mythrax-core/Cargo.toml",
                    "line": "N/A",
                    "description": f"The crate {pkg} version {ver} has been yanked from crates.io.",
                    "remediation": "Update to a non-yanked version.",
                    "effort": "Low"
                })
            if stale:
                 findings.append({
                    "severity": "Medium",
                    "title": f"Stale crate dependency: {pkg}",
                    "file": "mythrax-core/Cargo.toml",
                    "line": "N/A",
                    "description": f"The crate {pkg} has no recent commits/publications (>3 years).",
                    "remediation": "Investigate alternative crates or fork to maintain.",
                    "effort": "Medium"
                })
    except Exception as e:
         pass
    return findings

def check_git_secrets():
    findings = []
    try:
        subprocess.check_call(["git", "fetch", "--unshallow"])
    except:
        try:
             subprocess.check_call(["git", "fetch", "--all"])
        except:
             pass

    try:
        # A basic check for potential secrets in history
        output = subprocess.check_output(["git", "log", "-p", "-G", "(?i)(AKIA[0-9A-Z]{16}|secret|token|password|api_key)"], text=True)
        if output:
            findings.append({
                "severity": "Critical",
                "title": "AWS Key found in git history",
                "file": "Git History",
                "line": "N/A",
                "description": "An AWS Access Key ID pattern was found in git history.",
                "remediation": "Revoke the key immediately and rewrite git history to remove it.",
                "effort": "High"
            })
    except Exception:
        pass
    return findings

def create_github_issue(finding):
    token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        # Fallback to local mock issue if not in GH Actions
        os.makedirs("issues", exist_ok=True)
        issue_path = f"issues/{finding['title'].replace(' ', '_')}.md"
        with open(issue_path, "w") as f:
            f.write(f"# {finding['title']}\n\n")
            f.write(f"**Severity:** {finding['severity']}\n")
            f.write(f"**File:** {finding['file']}:{finding['line']}\n\n")
            f.write(f"## Description\n{finding['description']}\n\n")
            f.write(f"## Remediation\n{finding['remediation']}\n\n")
            f.write(f"**Effort:** {finding['effort']}\n")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }
    body = f"**Severity:** {finding['severity']}\n**File:** {finding['file']}:{finding['line']}\n\n## Description\n{finding['description']}\n\n## Remediation\n{finding['remediation']}\n\n**Effort:** {finding['effort']}"

    data = {
        "title": f"Security Audit: {finding['title']}",
        "body": body,
        "labels": ["security", "agent-found", finding['severity'].lower()]
    }

    req = urllib.request.Request(url, json.dumps(data).encode("utf-8"), headers)
    try:
         urllib.request.urlopen(req)
    except Exception as e:
         print(f"Failed to create GitHub issue: {e}", file=sys.stderr)

def main():
    prompt = load_ciso_prompt()
    all_findings = []

    # 1. Dependency Audit & Cargo Audit
    all_findings.extend(run_cargo_audit())
    all_findings.extend(audit_dependencies())

    # 2. Git History Secrets
    all_findings.extend(check_git_secrets())

    # 3. LLM Codebase Analysis
    files_to_scan = get_modified_files()
    if files_to_scan is None:
        # Scheduled run - full codebase (or a subset for practical limits)
        files_to_scan = []
        for root, _, files in os.walk("mythrax-core/src"):
            for f in files:
                if f.endswith(".rs"):
                    files_to_scan.append(os.path.join(root, f))

    chunks = chunk_files(files_to_scan)
    for chunk in chunks:
        res = call_llm(prompt, chunk)
        all_findings.extend(res.get("findings", []))

    # Output structure
    print("# Security Advisory Report\n")
    for finding in sorted(all_findings, key=lambda x: {"Critical":0, "High":1, "Medium":2, "Low":3}.get(x.get("severity", "Low"), 4)):
        print(f"## [{finding['severity']}] {finding['title']}")
        print(f"**Location:** {finding['file']}:{finding['line']}")
        print(f"**Description:** {finding['description']}")
        print(f"**Remediation:** {finding['remediation']}")
        print(f"**Effort:** {finding['effort']}\n")

        if finding['severity'] in ["Critical", "High"]:
            create_github_issue(finding)

if __name__ == "__main__":
    main()
