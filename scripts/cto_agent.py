import os
import json
import urllib.request
import subprocess

def get_modified_files():
    base_ref = os.environ.get("GITHUB_BASE_REF")
    if not base_ref:
        return None
    try:
        subprocess.run(["git", "fetch", "origin", base_ref], check=True)
        result = subprocess.run(
            ["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"],
            capture_output=True, text=True, check=True
        )
        return [f for f in result.stdout.splitlines() if f.strip()]
    except subprocess.CalledProcessError:
        return None

def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found. Degrading gracefully and bypassing LLM logic.")
        return

    try:
        with open(".agents/cto_agent/config.json", "r") as f:
            config = json.load(f)
        system_prompt = config["config"]["customAgent"]["systemPromptSections"]
    except Exception as e:
        print(f"Error loading config: {e}")
        return

    modified_files = get_modified_files()

    files_to_analyze = []
    targets = ["ARCHITECTURE.md", "mock_audit_report.md", "mythrax-core/src", ".agents", "evals"]
    for target in targets:
        if os.path.isfile(target):
            if modified_files is None or target in modified_files or target in ["ARCHITECTURE.md", "mock_audit_report.md"]:
                files_to_analyze.append(target)
        elif os.path.isdir(target):
            for root, _, files in os.walk(target):
                for file in files:
                    filepath = os.path.join(root, file)
                    if filepath.endswith((".rs", ".md", ".json", ".py")):
                        if modified_files is None or filepath in modified_files:
                            files_to_analyze.append(filepath)

    if not files_to_analyze:
        print("No relevant files to analyze.")
        return

    codebase_content = ""
    for filepath in files_to_analyze:
        try:
            with open(filepath, "r") as f:
                codebase_content += f"\n--- {filepath} ---\n"
                codebase_content += f.read()[:50000]
        except Exception:
            pass

    chunks = []
    chunk_size = 80000
    for i in range(0, len(codebase_content), chunk_size):
        chunks.append(codebase_content[i:i+chunk_size])

    findings = []
    for chunk in chunks:
        payload = {
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": f"Analyze this codebase chunk and return findings:\n{chunk}"}
            ],
            "response_format": {"type": "json_object"}
        }

        req = urllib.request.Request(
            "https://api.openai.com/v1/chat/completions",
            data=json.dumps(payload).encode("utf-8"),
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {api_key}"
            },
            method="POST"
        )

        try:
            with urllib.request.urlopen(req) as response:
                result = json.loads(response.read().decode("utf-8"))
                content = result["choices"][0]["message"]["content"]
                parsed = json.loads(content)
                if "findings" in parsed:
                    findings.extend(parsed["findings"])
        except Exception as e:
            print(f"Error querying LLM: {e}")

    print("--- Detailed Findings ---")
    for f in findings:
        print(json.dumps(f, indent=2))

    github_token = os.environ.get("GITHUB_TOKEN")
    github_repo = os.environ.get("GITHUB_REPOSITORY")

    if github_token and github_repo:
        for finding in findings:
            title = f"Adversarial CTO Finding: {finding.get('Finding', 'Architecture Issue')}"
            body = (
                f"**Current Assumption:** {finding.get('Current Assumption', '')}\n"
                f"**Attack Scenario:** {finding.get('Attack Scenario', '')}\n"
                f"**Blast Radius:** {finding.get('Blast Radius', '')}\n"
                f"**Recommended Structural Change:** {finding.get('Recommended Structural Change', '')}\n"
            )
            issue_payload = {
                "title": title,
                "body": body,
                "labels": ["architecture-review", "adversarial"]
            }
            issue_req = urllib.request.Request(
                f"https://api.github.com/repos/{github_repo}/issues",
                data=json.dumps(issue_payload).encode("utf-8"),
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {github_token}",
                    "Accept": "application/vnd.github.v3+json"
                },
                method="POST"
            )
            try:
                urllib.request.urlopen(issue_req)
                print(f"Filed issue: {title}")
            except Exception as e:
                print(f"Failed to file issue: {e}")
    else:
        print("GITHUB_TOKEN or GITHUB_REPOSITORY not found, skipping issue creation.")

if __name__ == "__main__":
    main()
