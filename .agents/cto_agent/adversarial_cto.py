import os
import sys
import subprocess
import json
import urllib.request
import urllib.error
from datetime import datetime, timezone

def file_issue(title, body):
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print(f"Skipping issue creation for '{title}' (missing token or repo).")
        return
    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    data = json.dumps({"title": title, "body": body}).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Successfully created issue: {title}")
    except urllib.error.URLError as e:
        print(f"Failed to create issue: {e}")

def run_cargo_audit():
    findings = []
    try:
        result = subprocess.run(
            ["cargo", "audit"],
            cwd="mythrax-core",
            capture_output=True,
            text=True,
            check=True
        )
        output = result.stdout
    except subprocess.CalledProcessError as e:
        output = e.stdout + "\n" + e.stderr
    except FileNotFoundError:
        output = "cargo audit command not found"

    if "error: vulnerabilities found!" in output or "Vulnerability" in output:
        findings.append({
            "title": "Dependencies with Known CVEs (cargo-audit)",
            "severity": "High",
            "description": f"cargo audit discovered vulnerabilities:\n```\n{output[:2000]}\n```",
            "remediation": "Update the affected crates to the patched versions.",
            "effort": "Low"
        })
    return findings

def check_crates_io():
    findings = []
    try:
        import toml
        with open("mythrax-core/Cargo.toml", "r") as f:
            cargo_toml = toml.load(f)
    except Exception as e:
        print(f"Failed to read Cargo.toml: {e}")
        return findings

    deps = []
    if "dependencies" in cargo_toml:
        deps.extend(cargo_toml["dependencies"].keys())
    if "dev-dependencies" in cargo_toml:
        deps.extend(cargo_toml["dev-dependencies"].keys())

    for crate in set(deps):
        url = f"https://crates.io/api/v1/crates/{crate}"
        req = urllib.request.Request(url, headers={"User-Agent": "Mythrax-Audit"})
        try:
            with urllib.request.urlopen(req) as response:
                if response.status == 200:
                    data = json.loads(response.read().decode("utf-8"))
                    versions = data.get("versions", [])
                    if versions:
                        latest = versions[0]
                        if latest.get("yanked", False):
                            findings.append({
                                "title": f"Yanked crate dependency: {crate}",
                                "severity": "Medium",
                                "description": f"The crate '{crate}' is yanked on crates.io.",
                                "remediation": "Remove or update this dependency.",
                                "effort": "Low"
                            })
                        updated_at = latest.get("updated_at")
                        if updated_at:
                            updated_dt = datetime.fromisoformat(updated_at.replace("Z", "+00:00"))
                            now = datetime.now(timezone.utc)
                            if (now - updated_dt).days > 730:
                                findings.append({
                                    "title": f"Stale crate dependency: {crate}",
                                    "severity": "Low",
                                    "description": f"The crate '{crate}' has no recent commits (last updated over 2 years ago).",
                                    "remediation": "Evaluate if this dependency is still maintained and consider alternatives.",
                                    "effort": "Medium"
                                })
        except urllib.error.URLError as e:
            print(f"Failed to check crate {crate}: {e}")
    return findings

def check_git_history():
    findings = []
    try:
        proc = subprocess.Popen(
            ["git", "log", "-G", "secret|token|key|password|credential|api_key", "-p", "--all"],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True
        )
        lines_read = 0
        suspicious_lines = []
        for line in proc.stdout:
            lines_read += 1
            if line.startswith("+") and not line.startswith("+++"):
                lower_line = line.lower()
                if any(x in lower_line for x in ["secret=", "token=", "api_key", "password="]):
                    suspicious_lines.append(line.strip()[:200])
            if lines_read > 50000:
                break
        proc.stdout.close()
        proc.wait()

        if suspicious_lines:
            findings.append({
                "title": "Potential Secrets in Git History",
                "severity": "High",
                "description": "Found potential secrets committed in git history. Sample:\n```\n" + "\n".join(suspicious_lines[:10]) + "\n```",
                "remediation": "Rotate affected secrets immediately. Consider rewriting git history.",
                "effort": "High"
            })
    except Exception as e:
        print(f"Failed to check git history: {e}")
    return findings

def run_llm_analysis(files_to_check):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found. Skipping LLM analysis.")
        return []

    findings = []
    for filepath in files_to_check:
        try:
            with open(filepath, "r") as f:
                content = f.read()
            if not content.strip():
                continue

            chunk_size = 30000
            chunks = [content[i:i+chunk_size] for i in range(0, len(content), chunk_size)]

            for chunk_idx, chunk in enumerate(chunks):
                prompt = f"""
You are an expert security auditor. Analyze the following Rust code snippet for:
1. Hardcoded secrets, tokens, API keys, or credentials.
2. Unsafe Rust blocks (`unsafe {{}}`). Enumerate every instance, explain the memory safety risk, and flag any that lack a documented justification comment.
3. Execution paths that accept untrusted external input without sanitization or boundary enforcement.

Code ({filepath} - part {chunk_idx + 1}):
```rust
{chunk}
```

Return a valid JSON object containing a single key "findings" which holds a JSON array of results. Each result should have:
- title
- severity (Critical, High, Medium, Low)
- description (explain the risk)
- remediation
- effort
"""
                url = "https://api.openai.com/v1/chat/completions"
                headers = {
                    "Authorization": f"Bearer {api_key}",
                    "Content-Type": "application/json"
                }
                data = json.dumps({
                    "model": "gpt-4o",
                    "response_format": {"type": "json_object"},
                    "messages": [{"role": "user", "content": prompt}],
                    "temperature": 0.0
                }).encode("utf-8")

                req = urllib.request.Request(url, data=data, headers=headers, method="POST")
                with urllib.request.urlopen(req) as response:
                    res_body = json.loads(response.read().decode("utf-8"))
                    content_str = res_body["choices"][0]["message"]["content"]
                    res_json = json.loads(content_str)
                    if "findings" in res_json:
                        findings.extend(res_json["findings"])
        except Exception as e:
            print(f"LLM analysis failed for {filepath}: {e}")

    return findings

def get_files_to_check():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "schedule")
    files_to_check = []

    if event_name == "push" or event_name == "pull_request":
        try:
            base_ref = os.environ.get("GITHUB_BASE_REF")
            if base_ref:
                result = subprocess.run(["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"], capture_output=True, text=True)
            else:
                result = subprocess.run(["git", "diff", "--name-only", "HEAD~1", "HEAD"], capture_output=True, text=True)

            if result.returncode == 0:
                files_to_check = [f for f in result.stdout.splitlines() if f.endswith(".rs") and os.path.exists(f)]
        except Exception as e:
            print(f"Failed to get modified files: {e}")

    if not files_to_check:
        for root, dirs, files in os.walk("mythrax-core/src"):
            for file in files:
                if file.endswith((".rs", ".toml", ".yaml", ".json")):
                    files_to_check.append(os.path.join(root, file))

    return files_to_check

def main():
    print("Starting Security Audit...")
    findings = []

    findings.extend(run_cargo_audit())
    findings.extend(check_crates_io())
    findings.extend(check_git_history())

    files = get_files_to_check()
    findings.extend(run_llm_analysis(files))

    print("\n" + "="*50)
    print("SECURITY ADVISORY REPORT")
    print("="*50)
    if not findings:
        print("No vulnerabilities found.")
    else:
        for f in findings:
            print(f"\n[{f.get('severity', 'Low')}] {f.get('title', 'Finding')}")
            print(f"Description: {f.get('description', '')}")
            print(f"Remediation: {f.get('remediation', '')}")
            print(f"Effort: {f.get('effort', '')}")

            if f.get('severity') in ["Critical", "High"]:
                body = f"**Description:**\n{f.get('description', '')}\n\n**Remediation:**\n{f.get('remediation', '')}\n\n**Effort:** {f.get('effort', '')}"
                file_issue(f"Security Alert: {f.get('title', 'Issue')}", body)

    print("\nAudit completed.")

if __name__ == "__main__":
    main()
