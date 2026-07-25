import os
import subprocess
import json
import re
import shlex

def analyze_unsafe_context(lines, index):
    """
    Look around the unsafe block to find out what it's doing
    and explain the memory safety risk.
    """
    context = "".join(lines[max(0, index-2):min(len(lines), index+3)]).lower()
    if "ffi" in context or "libc" in context or "kill" in context:
        return "Calling raw C/FFI functions can lead to undefined behavior if invariants are violated (e.g. invalid pointers or PIDs)."
    elif "send" in context or "sync" in context:
        return "Manually implementing Send/Sync bypasses the compiler's thread-safety checks, risking data races if the type is not actually thread-safe."
    elif "env::set_var" in context or "env::remove_var" in context:
        return "Modifying environment variables in a multi-threaded Rust context (like tokio) poses a significant risk for data races and undefined behavior."
    elif "mem::zeroed" in context or "mem::uninitialized" in context:
        return "Using uninitialized memory or zeroed out memory for types that do not permit zero bit-patterns can lead to immediate undefined behavior."
    else:
        return "Raw pointer dereferences or unsafe operations bypass compiler borrow checking, which can lead to memory corruption, use-after-free, or segfaults."

def issue_exists(title):
    if not os.environ.get('GH_TOKEN'):
        return False
    try:
        # Search for issues with this exact title
        # Properly escape the title for the shell/gh cli
        query = f'in:title "{title}"'
        res = subprocess.run(["gh", "issue", "list", "--search", query, "--json", "title"], capture_output=True, text=True, check=True)
        data = json.loads(res.stdout)
        for issue in data:
            if issue.get('title') == title:
                return True
        return False
    except Exception as e:
        print(f"Error checking issue existence: {e}")
        return False

def main():
    print("Starting Security Audit Script...")

    # 1. Look for hardcoded secrets, tokens, API keys, credentials
    print("Checking for hardcoded secrets...")
    secret_patterns = [
        r'(?i)(password|secret|token|api_key|credential|auth)[\s:=]+[\"\'][^\s\"\']+[\"\']',
    ]
    findings = []

    for root, dirs, files in os.walk('mythrax-core'):
        if '/target/' in root or 'tests' in root or 'scratch' in root or 'bench_data' in root:
            continue
        for file in files:
            if file.endswith('.rs') or file.endswith('.toml'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r') as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            for pattern in secret_patterns:
                                if re.search(pattern, line):
                                    findings.append({
                                        "type": "Hardcoded Secret",
                                        "file": filepath,
                                        "line": i + 1,
                                        "content": line.strip()[:200], # Trucate just in case
                                        "severity": "Critical",
                                        "effort": "Low",
                                        "recommendation": "Use environment variables or a secure vault (e.g. SecretStore) instead of hardcoding credentials in source code."
                                    })
                except Exception:
                    pass

    # 2. Unsafe Rust Blocks
    print("Checking for unsafe rust blocks...")
    unsafe_pattern = r'unsafe\s*\{'
    unsafe_impl_pattern = r'unsafe\s+impl'
    unsafe_findings = []

    for root, dirs, files in os.walk('mythrax-core'):
        if '/target/' in root or 'tests' in root or 'scratch' in root:
            continue
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r') as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            if re.search(unsafe_pattern, line) or re.search(unsafe_impl_pattern, line):
                                has_comment = False
                                for j in range(max(0, i-2), i):
                                    if "SAFETY:" in lines[j].upper() or "SAFETY :" in lines[j].upper() or "Safety:" in lines[j]:
                                        has_comment = True
                                        break

                                explanation = analyze_unsafe_context(lines, i)

                                unsafe_findings.append({
                                    "type": "Unsafe Rust",
                                    "file": filepath,
                                    "line": i + 1,
                                    "content": line.strip()[:200],
                                    "severity": "High" if not has_comment else "Medium",
                                    "effort": "Medium",
                                    "recommendation": f"Avoid unsafe blocks where possible. If strictly necessary, provide a comprehensive // SAFETY: comment explaining the invariants maintained. Memory Risk: {explanation}",
                                    "has_comment": has_comment
                                })
                except Exception:
                    pass

    # 3. Known CVEs in Dependencies
    print("Running cargo audit...")
    cargo_audit_cmd = ["cargo", "audit", "--json"]
    try:
        result = subprocess.run(cargo_audit_cmd, cwd="mythrax-core", capture_output=True, text=True)
        audit_data = json.loads(result.stdout)
    except Exception as e:
        audit_data = None
        print(f"Cargo audit failed to run or parse: {e}")

    cve_findings = []
    if audit_data:
        for vuln in audit_data.get("vulnerabilities", {}).get("list", []):
            severity = "High"
            try:
                if vuln.get('advisory') and vuln['advisory'].get('cvss'):
                    if isinstance(vuln['advisory']['cvss'], dict) and vuln['advisory']['cvss'].get('score', 0) >= 9.0:
                        severity = "Critical"
            except:
                pass
            cve_findings.append({
                "type": "Dependency Vulnerability",
                "file": "Cargo.lock",
                "line": 0,
                "content": f"{vuln['package']['name']} {vuln['package']['version']} - {vuln['advisory']['title']} ({vuln['advisory']['id']})",
                "severity": severity,
                "effort": "Low",
                "recommendation": f"Upgrade {vuln['package']['name']} to a patched version ({vuln['versions']['patched'][0] if vuln.get('versions') and vuln['versions'].get('patched') else 'Check for alternative crates'})."
            })

        warnings_dict = audit_data.get("warnings", {})
        for kind, warnings_list in warnings_dict.items():
            for warning in warnings_list:
                try:
                    name = warning.get('package', {}).get('name', 'unknown')
                    version = warning.get('package', {}).get('version', 'unknown')
                    title = warning.get('advisory', {}).get('title', 'N/A') if isinstance(warning.get('advisory'), dict) else 'N/A'
                    cve_findings.append({
                        "type": "Dependency Warning",
                        "file": "Cargo.lock",
                        "line": 0,
                        "content": f"{name} {version} - {kind} - {title}",
                        "severity": "Medium",
                        "effort": "Medium",
                        "recommendation": f"Consider replacing or removing the unmaintained or yanked crate {name}."
                    })
                except:
                    pass

    # 4. Untrusted Input / Input Sanitization Issues
    print("Checking for input sanitization issues...")
    sql_patterns = [r'query\s*\(\s*format!\(']
    shell_patterns = [r'Command::new\s*\(\s*(.*?)\s*\)', r'sh\s+-c', r'bash\s+-c']
    input_findings = []
    for root, dirs, files in os.walk('mythrax-core'):
        if '/target/' in root or 'tests' in root or 'scratch' in root:
            continue
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r') as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            for p in shell_patterns:
                                if re.search(p, line):
                                    input_findings.append({
                                        "type": "Potential Shell Injection",
                                        "file": filepath,
                                        "line": i + 1,
                                        "content": line.strip()[:200],
                                        "severity": "Critical",
                                        "effort": "Medium",
                                        "recommendation": "Avoid invoking shell directly. Ensure all inputs passed to Command::new or external processes are strictly sanitized and parameterized."
                                    })
                            for p in sql_patterns:
                                if re.search(p, line):
                                    input_findings.append({
                                        "type": "Potential SQL Injection",
                                        "file": filepath,
                                        "line": i + 1,
                                        "content": line.strip()[:200],
                                        "severity": "Critical",
                                        "effort": "Medium",
                                        "recommendation": "Use parameterized queries/prepared statements instead of string formatting (format!) for database queries."
                                    })
                except Exception:
                    pass

    # 5. Secrets in Git History
    print("Checking git history for secrets...")
    git_log_cmd = ["git", "log", "-p"]
    history_findings = []
    try:
        result = subprocess.run(git_log_cmd, capture_output=True, text=True, check=True)
        current_commit = ""
        for line in result.stdout.split('\n'):
            if line.startswith('commit '):
                current_commit = line.split()[1]
            elif line.startswith('+') and not line.startswith('+++'):
                for pattern in secret_patterns:
                    if re.search(pattern, line):
                        history_findings.append({
                            "type": "Secret in Git History",
                            "file": f"Commit: {current_commit[:8]}",
                            "line": 0,
                            "content": line.strip()[:100],
                            "severity": "High",
                            "effort": "High",
                            "recommendation": "Use a tool like BFG Repo-Cleaner or git filter-repo to scrub the secret from Git history, and rotate the exposed credential."
                        })
    except Exception as e:
        print(f"Git log failed: {e}")

    all_findings = findings + unsafe_findings + cve_findings + input_findings + history_findings

    # Sort findings by severity
    severity_order = {"Critical": 0, "High": 1, "Medium": 2, "Low": 3}
    all_findings.sort(key=lambda x: severity_order.get(x["severity"], 4))

    report = {
        "audit_date": "Now",
        "findings": all_findings
    }

    print("Generating Markdown Advisory Report...")
    md = "# Mythrax Security Advisory Report\n\n"
    md += "## Findings Summary\n\n"

    severity_counts = {"Critical": 0, "High": 0, "Medium": 0, "Low": 0}
    for finding in report['findings']:
        severity_counts[finding['severity']] += 1

    for sev in ["Critical", "High", "Medium", "Low"]:
        md += f"- **{sev}**: {severity_counts[sev]}\n"

    md += "\n## Detailed Findings\n\n"

    for finding in report['findings']:
        md += f"### [{finding['severity']}] {finding['type']}\n"
        md += f"- **Location**: `{finding['file']}` (Line: {finding['line']})\n"
        md += f"- **Content**:\n```\n{finding['content']}\n```\n"
        md += f"- **Recommendation**: {finding['recommendation']}\n"
        md += f"- **Effort**: {finding['effort']}\n\n"

    if not os.path.exists("docs"):
        os.makedirs("docs")
    with open("docs/security_advisory.md", "w") as f:
        f.write(md)

    if not os.path.exists("issues"):
        os.makedirs("issues")

    # File GitHub Issues for Critical or High findings
    print("Filing GitHub issues...")
    for finding in report['findings']:
        if finding['severity'] in ["Critical", "High"]:
            # 1. Create a local mock issue just in case
            issue_md = f"# [{finding['severity']}] {finding['type']}\n\n"
            issue_md += f"**Location**: `{finding['file']}` (Line: {finding['line']})\n\n"
            issue_md += "## Description\n"
            issue_md += f"A {finding['severity'].lower()} security finding was discovered during the nightly CISO audit.\n\n"
            issue_md += "**Content**:\n```\n" + finding['content'] + "\n```\n\n"
            issue_md += "## Recommendation\n"
            issue_md += f"{finding['recommendation']}\n\n"
            issue_md += f"**Estimated Effort**: {finding['effort']}\n"

            # 2. Use `gh` CLI to actually file the issue
            if os.environ.get('GH_TOKEN'):
                title = f"[{finding['severity']}] {finding['type']} in {finding['file']}"

                # Deduplication check
                if issue_exists(title):
                    print(f"Issue already exists: {title}")
                    continue

                try:
                    subprocess.run(
                        ["gh", "issue", "create", "--title", title, "--body", issue_md],
                        check=False,
                        capture_output=True
                    )
                    print(f"Filed issue: {title}")
                except Exception as e:
                    print(f"Could not file issue with gh: {e}")

    print("Generated Markdown and Mock Issues. Make sure not to commit these files.")

if __name__ == "__main__":
    main()
