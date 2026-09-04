#!/usr/bin/env python3
import os
import json
import subprocess
import urllib.request
import re
from datetime import datetime

# Read config if available
system_prompt = "You are a CISO auditor..."
try:
    with open(".agents/cto_agent/config.json", "r") as f:
        cfg = json.load(f)
        system_prompt = cfg.get("config", {}).get("customAgent", {}).get("systemPromptSections", system_prompt)
except Exception:
    pass

findings = []

def add_finding(severity, title, description, file_path, line, remediation, effort):
    findings.append({
        "severity": severity,
        "title": title,
        "description": description,
        "file": file_path,
        "line": line,
        "remediation": remediation,
        "effort": effort
    })

api_key = os.environ.get("OPENAI_API_KEY")
github_token = os.environ.get("GITHUB_TOKEN")
github_repository = os.environ.get("GITHUB_REPOSITORY", "keith-mcclellan/mythrax")

def llm_analyze(content, file_path):
    if not api_key:
        return []

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    prompt = f"""
    Analyze the following file for security vulnerabilities based on the CISO prompt.
    File: {file_path}
    Content:
    {content}
    """

    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": prompt}
        ],
        "response_format": {"type": "json_object"}
    }

    try:
        req = urllib.request.Request(url, json.dumps(data).encode("utf-8"), headers)
        with urllib.request.urlopen(req, timeout=30) as response:
            res_body = json.loads(response.read().decode("utf-8"))
            llm_res = res_body["choices"][0]["message"]["content"]
            parsed = json.loads(llm_res)
            return parsed.get("findings", [])
    except Exception as e:
        print(f"LLM request failed for {file_path}: {e}")
        return []

def run_cargo_audit():
    print("Running cargo audit...")
    try:
        result = subprocess.run(["cargo", "audit", "--json"], cwd="mythrax-core", capture_output=True, text=True)
        output = result.stdout
        if not output and result.returncode != 0:
            output = result.stderr

        if "{" in output:
            try:
                json_start = output.find("{")
                audit_data = json.loads(output[json_start:])
                for vuln in audit_data.get("vulnerabilities", {}).get("list", []):
                    pkg_name = vuln.get("package", {}).get("name", "Unknown")
                    pkg_version = vuln.get("package", {}).get("version", "Unknown")
                    advisory = vuln.get("advisory", {})
                    title = advisory.get("title", "Unknown Vulnerability")
                    desc = advisory.get("description", "")

                    add_finding(
                        "High",
                        f"Vulnerable Dependency: {pkg_name} {pkg_version}",
                        f"{title}\n{desc}",
                        "mythrax-core/Cargo.lock",
                        0,
                        f"Update {pkg_name} to a secure version.",
                        "Low"
                    )
            except Exception as e:
                print(f"Error parsing cargo audit JSON: {e}")

    except subprocess.CalledProcessError as e:
        pass
    except FileNotFoundError:
        print("cargo audit not found, skipping.")

def scan_codebase():
    import glob
    print("Scanning codebase...")

    secret_regex = re.compile(r'(?i)(?:api_key|token|secret|password|credential)\s*(?:=|:)\s*["\'][a-zA-Z0-9_\-]+["\']')
    unsafe_regex = re.compile(r'unsafe\s*\{')

    files_to_scan = []
    for root, dirs, files in os.walk("mythrax-core"):
        if "target" in root or ".git" in root:
            continue
        for file in files:
            if file.endswith((".rs", ".toml", ".yaml", ".json", ".md")):
                files_to_scan.append(os.path.join(root, file))

    for file_path in files_to_scan:
        try:
            with open(file_path, "r", encoding="utf-8") as f:
                content = f.read()

            lines = content.split('\n')
            for i, line in enumerate(lines):
                if secret_regex.search(line):
                    add_finding(
                        "Critical",
                        "Hardcoded Secret",
                        f"Potential hardcoded secret found: {line.strip()}",
                        file_path,
                        i + 1,
                        "Remove hardcoded secret and use environment variables or a secrets manager.",
                        "Medium"
                    )

                if "unsafe {" in line or "unsafe {" in line.replace(" ", ""):
                    justified = False
                    if i > 0 and ("// SAFETY" in lines[i-1].upper() or "// JUSTIFICATION" in lines[i-1].upper()):
                        justified = True

                    if not justified:
                        add_finding(
                            "High",
                            "Unjustified Unsafe Block",
                            "Unsafe Rust block found without a documentation comment explaining the memory safety risk.",
                            file_path,
                            i + 1,
                            "Document the safety invariants using a '// SAFETY:' comment above the block, or refactor to safe Rust.",
                            "Medium"
                        )
                    else:
                        add_finding(
                            "Medium",
                            "Unsafe Block (Justified)",
                            "Unsafe Rust block found. Ensure the safety invariants hold.",
                            file_path,
                            i + 1,
                            "Review safety invariants.",
                            "Low"
                        )

            if api_key and file_path.endswith(".rs"):
                llm_findings = llm_analyze(content, file_path)
                findings.extend(llm_findings)

        except Exception as e:
            print(f"Error scanning {file_path}: {e}")

def scan_git_history():
    print("Scanning git history...")
    try:
        subprocess.run(["git", "fetch", "--unshallow"], stderr=subprocess.DEVNULL)
        subprocess.run(["git", "fetch", "--all"], stderr=subprocess.DEVNULL)

        result = subprocess.run(["git", "log", "-i", "-p", "-G", r'(api_key|token|secret).+'], capture_output=True, text=True)
        if result.stdout:
            add_finding(
                "Critical",
                "Secret in Git History",
                "Found potential secrets in git commit history. This requires a full rewrite of history or immediate credential rotation.",
                "Git History",
                0,
                "Rotate the exposed credentials immediately. Consider using BFG Repo-Cleaner or git filter-repo.",
                "High"
            )
    except Exception as e:
        print(f"Error scanning git history: {e}")

def check_crates_activity():
    print("Checking crates.io for unmaintained or yanked crates...")
    try:
        with open("mythrax-core/Cargo.toml", "r") as f:
            content = f.read()
            deps = re.findall(r'^([a-zA-Z0-9_\-]+)\s*=\s*(?:".*"|\{.*\})', content, re.MULTILINE)

            for dep in deps:
                url = f"https://crates.io/api/v1/crates/{dep}"
                req = urllib.request.Request(url, headers={'User-Agent': 'Mythrax-Audit/1.0'})
                try:
                    with urllib.request.urlopen(req, timeout=10) as response:
                        data = json.loads(response.read().decode('utf-8'))
                        crate_info = data.get("crate", {})

                        updated_at = crate_info.get("updated_at")
                        if updated_at:
                            try:
                                dt = datetime.fromisoformat(updated_at.replace("Z", "+00:00"))
                                diff = datetime.now(dt.tzinfo) - dt
                                if diff.days > 730:
                                    add_finding(
                                        "Medium",
                                        f"Unmaintained Crate: {dep}",
                                        f"The crate {dep} has not been updated in over 2 years (last updated: {updated_at}).",
                                        "mythrax-core/Cargo.toml",
                                        0,
                                        "Consider replacing with a more actively maintained alternative.",
                                        "High"
                                    )
                            except ValueError:
                                pass

                        versions = data.get("versions", [])
                        if versions:
                            latest_version = versions[0]
                            if latest_version.get("yanked", False):
                                add_finding(
                                    "High",
                                    f"Yanked Crate Version: {dep}",
                                    f"The latest version of crate {dep} has been yanked.",
                                    "mythrax-core/Cargo.toml",
                                    0,
                                    "Update to a non-yanked version or switch to an alternative.",
                                    "Medium"
                                )

                except urllib.error.HTTPError as e:
                    if e.code == 404:
                        print(f"Crate {dep} not found on crates.io")
                except Exception as e:
                    pass
    except Exception as e:
        print(f"Error checking crates: {e}")

def file_github_issue(finding):
    if not github_token:
        print(f"Skipping GitHub issue creation for '{finding['title']}' (GITHUB_TOKEN not set)")
        return

    url = f"https://api.github.com/repos/{github_repository}/issues"
    headers = {
        "Accept": "application/vnd.github.v3+json",
        "Authorization": f"token {github_token}",
        "Content-Type": "application/json"
    }

    body = f"**Severity:** {finding['severity']}\n" \
           f"**File:** {finding['file']}:{finding['line']}\n\n" \
           f"## Description\n{finding['description']}\n\n" \
           f"## Remediation\n{finding['remediation']}\n\n" \
           f"**Estimated Effort:** {finding['effort']}"

    data = {
        "title": f"[{finding['severity']}] {finding['title']}",
        "body": body,
        "labels": ["security", "agent-found"]
    }

    try:
        req = urllib.request.Request(url, json.dumps(data).encode("utf-8"), headers)
        with urllib.request.urlopen(req, timeout=10) as response:
            res_data = json.loads(response.read().decode("utf-8"))
            print(f"Filed issue: {res_data.get('html_url')}")
    except Exception as e:
        print(f"Failed to file GitHub issue: {e}")

def main():
    run_cargo_audit()
    scan_codebase()
    scan_git_history()
    check_crates_activity()

    unique_findings = {}
    for f in findings:
        key = (f["title"], f["file"], f["line"])
        if key not in unique_findings:
            unique_findings[key] = f

    final_findings = list(unique_findings.values())

    print("\n" + "="*50)
    print("MYTHRAX SECURITY AUDIT REPORT")
    print("="*50)

    critical_count = sum(1 for f in final_findings if f["severity"] == "Critical")
    high_count = sum(1 for f in final_findings if f["severity"] == "High")
    medium_count = sum(1 for f in final_findings if f["severity"] == "Medium")
    low_count = sum(1 for f in final_findings if f["severity"] == "Low")

    print(f"Summary: {critical_count} Critical, {high_count} High, {medium_count} Medium, {low_count} Low")

    os.makedirs("issues", exist_ok=True)

    for i, f in enumerate(final_findings):
        print(f"\n[{f['severity']}] {f['title']}")
        print(f"File: {f['file']}:{f['line']}")
        print(f"Description: {f['description']}")
        print(f"Remediation: {f['remediation']}")
        print(f"Effort: {f['effort']}")

        if f["severity"] in ["Critical", "High"]:
            # Optionally write to local file as fallback/log
            issue_content = f"# {f['title']}\n\n**Severity:** {f['severity']}\n**File:** {f['file']}:{f['line']}\n\n## Description\n{f['description']}\n\n## Remediation\n{f['remediation']}\n\n**Estimated Effort:** {f['effort']}\n"
            issue_file = f"issues/issue_{i}_{f['severity']}.md"
            with open(issue_file, "w") as iss_f:
                iss_f.write(issue_content)

            file_github_issue(f)

    try:
        with open(".gitignore", "a+") as gi:
            gi.seek(0)
            if "issues/" not in gi.read():
                gi.write("\nissues/\n")
    except Exception:
        pass

if __name__ == "__main__":
    main()
