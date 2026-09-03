import os
import sys
import json
import urllib.request
import urllib.error
import subprocess

def get_git_diff_files():
    try:
        # Assuming run in a PR context if GITHUB_BASE_REF is set
        base_ref = os.environ.get("GITHUB_BASE_REF")
        if base_ref:
            subprocess.run(["git", "fetch", "origin", base_ref], check=True)
            result = subprocess.run(["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"], capture_output=True, text=True, check=True)
            return [line.strip() for line in result.stdout.split('\n') if line.strip()]
        else:
            return None # All files
    except subprocess.CalledProcessError as e:
        print(f"Error fetching git diff: {e}")
        return None

def find_untested_functions():
    print("Finding untested public functions...", flush=True)
    untested_findings = []
    try:
        # Search for 'pub fn' in mythrax-core/src/
        result = subprocess.run(["grep", "-rn", "pub fn", "mythrax-core/src/"], capture_output=True, text=True, check=True)
        pub_fns = [line.strip() for line in result.stdout.split('\n') if line.strip()]

        # Simple heuristic: cross reference with files containing tests
        # A more complex analysis would parse AST, but for this script we look for missing corresponding test files
        # Or missing `#[test]` in the same file if no test file exists.
        for fn_line in pub_fns:
            parts = fn_line.split(":")
            if len(parts) >= 3:
                file_path = parts[0]
                line_num = parts[1]
                fn_decl = ":".join(parts[2:])

                # Check if file has tests
                has_tests = False
                try:
                    with open(file_path, "r", encoding="utf-8") as f:
                        content = f.read()
                        if "#[test]" in content or "#[cfg(test)]" in content:
                            has_tests = True
                except Exception:
                    pass

                # If no inline tests, check if there's a corresponding test file in tests/ or eval/
                # A naive approach for demonstration: flag it if no inline test and no tests/ counterpart
                file_name = os.path.basename(file_path).replace(".rs", "")
                test_file_path = f"mythrax-core/tests/test_{file_name}.rs"

                if not has_tests and not os.path.exists(test_file_path):
                    # Need to format as requested
                    untested_findings.append({
                        "file": file_path,
                        "line_number": int(line_num),
                        "minimal_reproducible_scenario": f"Public function `{fn_decl.strip()}` has no inline tests and no corresponding test file.",
                        "severity": "medium",
                        "suggested_fix": f"Add a test module with #[cfg(test)] in {file_path} or create {test_file_path}.",
                        "labels": ["bug", "agent-found", "test-gap"]
                    })
    except subprocess.CalledProcessError as e:
        print(f"Error running grep: {e}")
    return untested_findings

def read_file_content(filepath):
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            return f.read()
    except Exception as e:
        print(f"Error reading {filepath}: {e}")
        return ""

def main():
    print("Starting CTO Agent...", flush=True)

    # 1. Load config
    config_path = ".agents/cto_agent/config.json"
    if not os.path.exists(config_path):
        print(f"Config file not found: {config_path}")
        sys.exit(1)

    with open(config_path, "r") as f:
        config_data = json.load(f)

    sys_prompt_sections = config_data.get("config", {}).get("customAgent", {}).get("systemPromptSections", {})
    if not sys_prompt_sections:
        print("Invalid config format: missing systemPromptSections")
        sys.exit(1)

    persona = sys_prompt_sections.get("persona", "")
    task = sys_prompt_sections.get("task", "")
    rules = "\n".join(sys_prompt_sections.get("rules", []))
    output_format = sys_prompt_sections.get("outputFormat", "")

    system_prompt = f"{persona}\n\n{task}\n\nRules:\n{rules}\n\nOutput Format:\n{output_format}"

    # 2. Get files to analyze
    files_to_analyze = get_git_diff_files()
    if files_to_analyze is None:
        # Full run, let's just pick mythrax-core/src/ and evals/ for now to avoid context limit
        files_to_analyze = []
        for root, _, files in os.walk("mythrax-core/src/"):
            for file in files:
                if file.endswith(".rs"):
                    files_to_analyze.append(os.path.join(root, file))
        for root, _, files in os.walk("evals/"):
            for file in files:
                if file.endswith(".rs") or file.endswith(".py") or file.endswith(".sh"):
                    files_to_analyze.append(os.path.join(root, file))
        for root, _, files in os.walk(".agents/"):
            for file in files:
                if file.endswith(".json") or file.endswith(".md"):
                    files_to_analyze.append(os.path.join(root, file))

    print(f"Analyzing {len(files_to_analyze)} files.", flush=True)

    # 3. Call LLM for each chunk of files to find logic bugs
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY environment variable is missing.", file=sys.stderr)
        # Proceed with just grep analysis if no API key in CI?
        # Let's exit 1 if this is a real CI run expecting LLM
        sys.exit(1)

    all_findings = []

    # Chunking files
    chunk_size = 5
    for i in range(0, len(files_to_analyze), chunk_size):
        chunk_files = files_to_analyze[i:i+chunk_size]
        code_context = ""
        for file in chunk_files:
            content = read_file_content(file)
            code_context += f"\\n--- FILE: {file} ---\\n{content}\\n"

        user_prompt = f"Analyze the following code for bugs based on your rules:\\n\\n{code_context}"

        # Prepare request
        req_data = {
            "model": "gpt-4o",
            "response_format": { "type": "json_object" },
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "temperature": 0.2
        }

        req = urllib.request.Request(
            "https://api.openai.com/v1/chat/completions",
            data=json.dumps(req_data).encode("utf-8"),
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {api_key}"
            }
        )

        try:
            with urllib.request.urlopen(req) as response:
                resp_data = json.loads(response.read().decode("utf-8"))
                content = resp_data["choices"][0]["message"]["content"]
                result = json.loads(content)
                if "findings" in result:
                    all_findings.extend(result["findings"])
        except urllib.error.HTTPError as e:
            print(f"LLM API HTTP Error: {e.code} {e.read().decode('utf-8')}")
        except Exception as e:
            print(f"LLM API Error: {e}")

    # 4. Add explicitly found untested functions
    untested_functions = find_untested_functions()
    all_findings.extend(untested_functions)

    # 5. Output findings and file issues
    print("\n--- DETAILED FINDINGS ---")
    print(json.dumps(all_findings, indent=2))
    print("-------------------------\n")

    gh_token = os.environ.get("GH_TOKEN")
    github_repository = os.environ.get("GITHUB_REPOSITORY")

    if all_findings and gh_token and github_repository:
        print("Filing GitHub issues for findings...", flush=True)
        for finding in all_findings:
            title = f"[Adversarial CTO] Issue in {finding.get('file', 'Unknown')}:{finding.get('line_number', '?')}"
            body = f"**File:** `{finding.get('file', 'Unknown')}`\n"
            body += f"**Line:** `{finding.get('line_number', '?')}`\n"
            body += f"**Severity:** {finding.get('severity', 'Unknown')}\n\n"
            body += f"### Minimal Reproducible Scenario\n{finding.get('minimal_reproducible_scenario', 'N/A')}\n\n"
            body += f"### Suggested Fix\n{finding.get('suggested_fix', 'N/A')}\n"

            labels = finding.get('labels', ['bug', 'agent-found'])

            issue_data = {
                "title": title,
                "body": body,
                "labels": labels
            }

            req = urllib.request.Request(
                f"https://api.github.com/repos/{github_repository}/issues",
                data=json.dumps(issue_data).encode("utf-8"),
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {gh_token}",
                    "Accept": "application/vnd.github.v3+json",
                    "User-Agent": "CTO-Agent-Script"
                }
            )

            try:
                with urllib.request.urlopen(req) as response:
                    if response.status == 201:
                        print(f"Successfully filed issue: {title}")
                    else:
                        print(f"Failed to file issue {title}: Status {response.status}")
            except urllib.error.HTTPError as e:
                print(f"GitHub API Error for issue '{title}': {e.code} {e.read().decode('utf-8')}")
            except Exception as e:
                print(f"GitHub API Error for issue '{title}': {e}")
    else:
        if not gh_token:
            print("GH_TOKEN not set, skipping issue filing.")
        if not github_repository:
            print("GITHUB_REPOSITORY not set, skipping issue filing.")

    if all_findings:
        print(f"Total findings: {len(all_findings)}")
        # We don't fail the CI just because we found bugs, we file issues.
        # But if you want it to fail, you could sys.exit(1) here.
    else:
        print("No bugs found. LGTM!")

if __name__ == "__main__":
    main()
