import os
import json
import urllib.request
import subprocess
import re
from datetime import datetime

def load_config():
    system_prompt = "You are the CISO of a company whose most critical asset is github.com/keith-mcclellan/mythrax. You are paranoid by design. Your threat model assumes external attackers, compromised dependencies, and insider risk. Evaluate the provided codebase for hardcoded secrets, tokens, API keys, or credentials anywhere in source or config files. Enumerate every instance of unsafe Rust blocks (unsafe {}), explain the memory safety risk, and flag any that lack a documented justification comment. Identify execution paths that accept untrusted external input without sanitization or boundary enforcement. Detect secrets potentially committed in git history, not just current HEAD. Output a structured JSON array of findings with keys: id (a unique string for deduplication), title, description, severity (Critical/High/Medium/Low), remediation, effort."

    try:
        with open(".agents/agents/ciso/config.json", "r") as f:
            data = json.load(f)
            if "config" in data and "customAgent" in data["config"] and "systemPromptSections" in data["config"]["customAgent"]:
                return data["config"]["customAgent"]["systemPromptSections"]
    except Exception:
        pass

    return system_prompt

def get_git_history_chunks(max_chars=60000):
    subprocess.run("git fetch --unshallow || git fetch --all", shell=True, capture_output=True)
    # Only check the last 5 commits for speed and API limits
    res = subprocess.run(["git", "log", "-p", "-n", "5"], capture_output=True, text=True)
    history = res.stdout
    chunks = []
    for i in range(0, len(history), max_chars):
        chunks.append(history[i:i+max_chars])
    return chunks

def run_cargo_audit():
    try:
        cwd = "mythrax-core" if os.path.exists("mythrax-core") else "."
        res = subprocess.run(["cargo", "audit", "--json"], cwd=cwd, capture_output=True, text=True, check=False)
        output = res.stdout.strip()
        if not output:
             return None
        return json.loads(output)
    except subprocess.CalledProcessError as e:
        output = e.stdout.strip()
        if not output:
            return None
        try:
             return json.loads(output)
        except:
             print(f"cargo-audit error parsing JSON: {e}")
             return None
    except Exception as e:
        print(f"cargo-audit error: {e}")
        return None

def check_stale_crates():
    findings = []
    try:
        cwd = "mythrax-core" if os.path.exists("mythrax-core") else "."
        res = subprocess.run(["cargo", "tree", "--depth", "1", "--prefix", "none"], cwd=cwd, capture_output=True, text=True, check=False)
        lines = res.stdout.strip().split('\n')
        crates = set()
        for line in lines:
            parts = line.split(' ')
            if parts and parts[0]:
                crates.add(parts[0])

        for crate in crates:
            if not crate or crate == 'mythrax-core': continue
            try:
                url = f"https://crates.io/api/v1/crates/{crate}"
                req = urllib.request.Request(url, headers={'User-Agent': 'mythrax-ciso-bot'})
                with urllib.request.urlopen(req) as response:
                    data = json.loads(response.read().decode('utf-8'))
                    if 'crate' in data and 'updated_at' in data['crate']:
                        updated = data['crate']['updated_at']
                        # Simple string comparison to see if it's older than 1 year
                        # Format is 2023-11-20T...
                        year = int(updated.split('-')[0])
                        current_year = datetime.now().year
                        if current_year - year > 1:
                            findings.append({
                                'id': f"stale_crate_{crate}",
                                'title': f"Stale Dependency: {crate}",
                                'description': f"The crate {crate} has no recent commits/updates (last updated {updated}).",
                                'severity': "Medium",
                                'remediation': f"Investigate replacing {crate} or vendoring it if necessary.",
                                'effort': "Medium"
                            })
            except Exception as e:
                pass # Skip if api fails
    except Exception as e:
        pass
    return findings

def get_source_chunks(max_chars=60000):
    chunks = []
    current_chunk = ""
    target_dir = "mythrax-core" if os.path.exists("mythrax-core") else "."

    for root, dirs, files in os.walk(target_dir):
        # Prevent checking .git directory, but allow .github
        if "/.git" in root or root.endswith(".git") or "target" in root:
            continue

        for f in files:
            if f.endswith(".rs") or f.endswith(".toml") or f.endswith(".lock") or f.endswith(".json") or f.endswith(".yml") or f.endswith(".yaml") or f.endswith(".env"):
                path = os.path.join(root, f)
                try:
                    with open(path, "r", encoding="utf-8") as file:
                        file_content = f"\n--- {path} ---\n" + file.read()
                        if len(current_chunk) + len(file_content) > max_chars:
                            chunks.append(current_chunk)
                            current_chunk = file_content
                        else:
                            current_chunk += file_content
                except:
                    pass
    if current_chunk:
        chunks.append(current_chunk)
    return chunks

def call_llm(system_prompt, user_content):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("Warning: OPENAI_API_KEY not set. Returning empty findings.")
        return []
    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    sys_prompt = system_prompt + "\nReturn a JSON object with a single key 'findings' containing an array of objects. Keys for each object: id (a unique, deterministic string combining the issue type and location, e.g. 'unsafe_db_rs_45' or 'secret_config_yml_db_pass'), title, description, severity (Critical/High/Medium/Low), remediation, effort."

    data = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": sys_prompt},
            {"role": "user", "content": user_content}
        ],
        "response_format": {"type": "json_object"}
    }
    req = urllib.request.Request(url, json.dumps(data).encode('utf-8'), headers)
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            return json.loads(result['choices'][0]['message']['content']).get('findings', [])
    except Exception as e:
        print(f"Error calling LLM: {e}")
        return []

def get_existing_issue_ids():
    token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        return set()

    # Get both open and closed issues to prevent refiling
    url = f"https://api.github.com/repos/{repo}/issues?state=all&labels=security"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
    }

    existing_ids = set()
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            issues = json.loads(response.read().decode('utf-8'))
            for issue in issues:
                body = issue.get('body', '')
                # Extract ID from body if present
                match = re.search(r'\*\*ID:\*\* ([^\s]+)', body)
                if match:
                    existing_ids.add(match.group(1))
    except Exception as e:
        print(f"Error fetching issues: {e}")

    return existing_ids

def file_github_issue(finding, existing_ids):
    token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repo:
        print("GH_TOKEN or GITHUB_REPOSITORY not set. Cannot file issue.")
        return

    finding_id = finding.get('id', 'unknown_id')

    # Deduplication check using the deterministic ID
    if finding_id in existing_ids:
        print(f"Issue with ID {finding_id} already exists or was closed.")
        return

    title = f"[CISO Audit] {finding.get('title', 'Security Finding')}"
    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json",
        "Content-Type": "application/json"
    }

    body = f"**ID:** {finding_id}\n\n"
    body += f"**Description:**\n{finding.get('description', '')}\n\n"
    body += f"**Severity:** {finding.get('severity', '')}\n\n"
    body += f"**Remediation:** {finding.get('remediation', '')}\n\n"
    body += f"**Effort:** {finding.get('effort', '')}"

    data = {
        "title": title,
        "body": body,
        "labels": ["security", finding.get('severity', 'Medium').lower()]
    }

    req = urllib.request.Request(url, json.dumps(data).encode('utf-8'), headers)
    try:
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Successfully filed issue: {title}")
                existing_ids.add(finding_id)
            else:
                print(f"Failed to file issue: {response.status}")
    except Exception as e:
        print(f"Error filing issue via API: {e}")

def main():
    system_prompt = load_config()
    all_findings = []

    existing_ids = get_existing_issue_ids()

    # Stale dependencies
    print("Checking for stale crates...")
    stale_findings = check_stale_crates()
    all_findings.extend(stale_findings)

    # Cargo audit (process locally instead of LLM)
    print("Running cargo audit...")
    audit_json = run_cargo_audit()
    if audit_json:
        if 'vulnerabilities' in audit_json and audit_json['vulnerabilities']['list']:
            for vuln in audit_json['vulnerabilities']['list']:
                adv = vuln['advisory']
                all_findings.append({
                    'id': f"cve_{adv['id']}",
                    'title': f"Vulnerability in {vuln['package']['name']}: {adv['title']}",
                    'description': adv['description'],
                    'severity': "High", # Map to high/critical based on cvss if available, simplify for now
                    'remediation': f"Update {vuln['package']['name']} to a safe version.",
                    'effort': "Low"
                })

        # yanked crates
        if 'warnings' in audit_json:
            for warning in audit_json['warnings'].get('yanked', []):
                all_findings.append({
                    'id': f"yanked_{warning['package']['name']}_{warning['package']['version']}",
                    'title': f"Yanked crate in use: {warning['package']['name']}",
                    'description': f"Version {warning['package']['version']} of {warning['package']['name']} has been yanked.",
                    'severity': "Medium",
                    'remediation': f"Update {warning['package']['name']} to a non-yanked version.",
                    'effort': "Low"
                })

    # Git History
    git_chunks = get_git_history_chunks()
    for i, chunk in enumerate(git_chunks):
        print(f"Analyzing git history chunk {i+1}/{len(git_chunks)}...")
        hist_findings = call_llm(system_prompt, f"Git History Diffs (look for secrets):\n{chunk}")
        all_findings.extend(hist_findings)

    # Source Files
    print("Analyzing source files...")
    chunks = get_source_chunks()
    for i, chunk in enumerate(chunks):
        print(f"Analyzing chunk {i+1}/{len(chunks)}...")
        chunk_findings = call_llm(system_prompt, f"Source Files Chunk (look for secrets, unsafe blocks, untrusted input):\n{chunk}")
        all_findings.extend(chunk_findings)

    print("\n" + "="*50)
    print("SECURITY ADVISORY REPORT")
    print("="*50 + "\n")

    for finding in all_findings:
        title = finding.get('title', 'Unknown Title')
        desc = finding.get('description', '')
        sev = finding.get('severity', 'Unknown')
        rem = finding.get('remediation', '')
        eff = finding.get('effort', '')

        print(f"Title: {title}")
        print(f"Severity: {sev}")
        print(f"Description: {desc}")
        print(f"Remediation: {rem}")
        print(f"Effort: {eff}")
        print("-" * 50)

        if sev in ["Critical", "High"]:
            file_github_issue(finding, existing_ids)

if __name__ == "__main__":
    main()
