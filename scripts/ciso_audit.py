import json
import os
import sys
import subprocess
import urllib.request
import urllib.error
import time

def get_system_prompt():
    config_path = ".agents/ciso_agent/config.json"
    if not os.path.exists(config_path):
        return "You are a security auditor."
    with open(config_path, "r") as f:
        config = json.load(f)
        return "\n".join(config.get("config", {}).get("customAgent", {}).get("systemPromptSections", []))

def call_llm(prompt, text_content):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not set. Gracefully degrading LLM check.")
        return '{"findings": []}'

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    data = {
        "model": "gpt-4o",
        "response_format": {"type": "json_object"},
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": f"Analyze the following codebase and dependency output:\n\n{text_content}"}
        ]
    }
    req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            res_body = response.read().decode("utf-8")
            res_json = json.loads(res_body)
            return res_json["choices"][0]["message"]["content"]
    except Exception as e:
        print(f"LLM API call failed: {e}")
        return '{"findings": []}'

def run_command(cmd, cwd=None):
    try:
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=cwd, check=True)
        return result.stdout
    except subprocess.CalledProcessError as e:
        # e.g., cargo audit fails if vulnerabilities are found
        return e.stdout + e.stderr

def analyze():
    # 1. Get git history for secrets
    git_history_diff = run_command("git log -p -i -G 'secret|token|key|password|credential' --all -- mythrax-core")

    # 2. Get cargo audit output
    cargo_audit_out = run_command("cargo audit", cwd="mythrax-core")

    # 3. Get files content
    files_content = ""
    for root, dirs, files in os.walk("mythrax-core"):
        for file in files:
            if file.endswith((".rs", ".json", ".yaml", ".yml", ".env", ".toml", ".lock")):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, "r", encoding="utf-8") as f:
                        files_content += f"\n--- {filepath} ---\n{f.read()}"
                except Exception:
                    pass

    # Get crates.io data for dependencies in Cargo.lock
    crates_io_data = ""
    try:
        import tomllib
        if os.path.exists("mythrax-core/Cargo.lock"):
            with open("mythrax-core/Cargo.lock", "rb") as f:
                lock_data = tomllib.load(f)

            crates_io_data = "CRATES.IO DATA:\n"
            for package in lock_data.get('package', []):
                name = package.get('name')
                version = package.get('version')
                if name:
                    try:
                        req = urllib.request.Request(f"https://crates.io/api/v1/crates/{name}", headers={'User-Agent': 'mythrax-security-auditor (contact@example.com)'})
                        with urllib.request.urlopen(req, timeout=10) as response:
                            crate_info = json.loads(response.read().decode('utf-8'))
                            updated_at = crate_info.get('crate', {}).get('updated_at')

                            is_yanked = "Unknown"
                            for v_info in crate_info.get('versions', []):
                                if v_info.get('num') == version:
                                    is_yanked = v_info.get('yanked')
                                    break

                            crates_io_data += f"Crate: {name}, Version: {version}, Yanked: {is_yanked}, Updated at: {updated_at}\n"
                        # Rate limit: max 1 request per second for crates.io API
                        time.sleep(1.1)
                    except Exception as e:
                        print(f"Failed to fetch crates.io data for {name}: {e}")
    except Exception as e:
        crates_io_data = f"Failed to fetch crates.io data: {e}"


    combined_content = f"GIT HISTORY (Potential Secrets):\n{git_history_diff}\n\nCARGO AUDIT:\n{cargo_audit_out}\n\n{crates_io_data}\n\nFILES:\n{files_content}"

    prompt = get_system_prompt()
    llm_response = call_llm(prompt, combined_content)

    try:
        findings_obj = json.loads(llm_response)
        findings = findings_obj.get("findings", [])
    except Exception as e:
        print(f"Failed to parse LLM response: {e}")
        findings = []

    # Output structured report
    print("# Security Advisory Report\n")
    for finding in findings:
        print(f"## [{finding.get('severity', 'Unknown')}] {finding.get('title', 'Untitled')}")
        print(f"**Description**: {finding.get('description', '')}")
        print(f"**Remediation**: {finding.get('remediation', '')}")
        print(f"**Effort**: {finding.get('effort', '')}\n")

    # File GitHub Issues for Critical or High findings
    github_token = os.environ.get("GITHUB_TOKEN")
    github_repo = os.environ.get("GITHUB_REPOSITORY")

    if github_token and github_repo:
        url = f"https://api.github.com/repos/{github_repo}/issues"
        headers = {
            "Authorization": f"Bearer {github_token}",
            "Accept": "application/vnd.github.v3+json"
        }

        for finding in findings:
            severity = finding.get("severity", "").upper()
            if severity in ["CRITICAL", "HIGH"]:
                title = f"[Security - {severity}] {finding.get('title', 'Untitled')}"
                body = f"**Description**: {finding.get('description', '')}\n\n**Remediation**: {finding.get('remediation', '')}\n\n**Effort**: {finding.get('effort', '')}"
                data = {"title": title, "body": body, "labels": ["security", severity.lower()]}
                req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
                try:
                    with urllib.request.urlopen(req, timeout=10) as response:
                        print(f"Filed issue for: {title}")
                except Exception as e:
                    print(f"Failed to file issue for {title}: {e}")
    else:
        print("GITHUB_TOKEN or GITHUB_REPOSITORY not set. Skipping issue filing.")

if __name__ == "__main__":
    analyze()
