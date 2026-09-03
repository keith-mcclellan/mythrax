import os
import json
import subprocess
import urllib.request
import urllib.error
import fnmatch

def main():
    print("Running Adversarial CTO Review...")

    # Load the configuration to parse the system prompt dynamically
    try:
        with open(".agents/adversarial_cto/config.json", "r") as f:
            config = json.load(f)
            system_prompt = "\n".join(config["config"]["customAgent"]["systemPromptSections"])
    except Exception as e:
        print(f"Failed to load config: {e}")
        return

    openai_api_key = os.environ.get("OPENAI_API_KEY")
    gh_token = os.environ.get("GH_TOKEN")
    is_pr = os.environ.get("GITHUB_EVENT_NAME") == "pull_request"
    base_ref = os.environ.get("GITHUB_BASE_REF")

    files_to_analyze = []

    if is_pr and base_ref:
        print("Running in PR mode, analyzing modified files.")
        try:
            diff_output = subprocess.check_output(
                ["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"],
                text=True
            )
            changed_files = [line.strip() for line in diff_output.splitlines() if line.strip()]
            for f in changed_files:
                if (f == "ARCHITECTURE.md" or f.startswith("mythrax-core/src/") or f.startswith(".agents/")):
                    if os.path.exists(f):
                        files_to_analyze.append(f)
        except Exception as e:
            print(f"Failed to get changed files: {e}")
            files_to_analyze = ["ARCHITECTURE.md", "mock_audit_report.md"]
    else:
        print("Running in full mode, analyzing codebase.")
        files_to_analyze = ["ARCHITECTURE.md", "mock_audit_report.md"]
        # Add a few core files just as a sample if they exist
        for root, dirs, files in os.walk("evals/"):
            for file in files:
                files_to_analyze.append(os.path.join(root, file))

    context_text = ""
    for file_path in files_to_analyze:
        if os.path.exists(file_path):
            try:
                with open(file_path, "r") as f:
                    content = f.read()
                    context_text += f"\n--- File: {file_path} ---\n{content}\n"
            except Exception as e:
                print(f"Could not read {file_path}: {e}")

    # Also explicitly ingest evals/ directory if not already
    try:
        evals_ls = subprocess.check_output(["ls", "-la", "evals/"], text=True)
        context_text += f"\n--- Directory: evals/ ---\n{evals_ls}\n"
    except Exception as e:
        pass

    if openai_api_key:
        print("Invoking LLM for analysis...")
        url = "https://api.openai.com/v1/chat/completions"
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {openai_api_key}"
        }

        user_prompt = f"Analyze the following codebase context and provide your findings:\n\n{context_text[:100000]}"

        data = {
            "model": "gpt-4-turbo",
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "response_format": {"type": "json_object"}
        }

        req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
        try:
            with urllib.request.urlopen(req) as response:
                result = json.loads(response.read().decode("utf-8"))
                llm_output = result["choices"][0]["message"]["content"]
                parsed_output = json.loads(llm_output)
                findings = parsed_output.get("findings", [])
        except Exception as e:
            print(f"Failed to invoke LLM: {e}")
            findings = []
    else:
        print("No OPENAI_API_KEY provided. Simulating dynamic analysis.")
        findings = []
        try:
            with open("ARCHITECTURE.md", "r") as f:
                arch_content = f.read()
                if "Vector Similarity" in arch_content:
                     findings.append({
                         "finding_title": "Over-reliance on Vector Similarity Scoring",
                         "current_assumption": "Assuming Vector Similarity provides robust enough semantic matching.",
                         "attack_scenario": "Adversarial text inputs crafted to evade similarity bounds but retain malicious payload.",
                         "blast_radius": "Search logic compromise, feeding toxic data to agent context.",
                         "recommended_structural_change": "Introduce independent deterministic filter checks."
                     })
        except Exception as e:
            pass

        try:
            evals_content = subprocess.check_output(["ls", "-la", "evals/"], text=True)
            findings.append({
                "finding_title": "Eval Framework Lacks True Adversarial Path Testing",
                "current_assumption": "Current evals/ cover necessary bounds.",
                "attack_scenario": "Agent loop encounters prompt injection or recursive logic bomb not captured in evals.",
                "blast_radius": "Agent gets stuck in unbounded recursion, causing OOM or API exhaustion.",
                "recommended_structural_change": "Mandate negative-path and injection-payload tests in evals/."
            })
        except Exception as e:
            pass

    print("--- Red Team Architecture Brief ---")
    for idx, finding in enumerate(findings):
        print(f"Finding {idx + 1}: {finding.get('finding_title', 'Unknown')}")
        print(f"Current Assumption: {finding.get('current_assumption', 'Unknown')}")
        print(f"Attack Scenario: {finding.get('attack_scenario', 'Unknown')}")
        print(f"Blast Radius: {finding.get('blast_radius', 'Unknown')}")
        print(f"Recommended Structural Change: {finding.get('recommended_structural_change', 'Unknown')}")
        print("-" * 40)

        if gh_token:
            repo = os.environ.get("GITHUB_REPOSITORY", "user/repo")
            url = f"https://api.github.com/repos/{repo}/issues"

            headers = {
                "Authorization": f"token {gh_token}",
                "Accept": "application/vnd.github.v3+json"
            }

            body = (
                f"**Current Assumption:** {finding.get('current_assumption')}\n\n"
                f"**Attack Scenario:** {finding.get('attack_scenario')}\n\n"
                f"**Blast Radius:** {finding.get('blast_radius')}\n\n"
                f"**Recommended Structural Change:** {finding.get('recommended_structural_change')}\n\n"
                f"*(Automated finding by Adversarial CTO)*"
            )

            data = {
                "title": f"Adversarial CTO Finding: {finding.get('finding_title')}",
                "body": body,
                "labels": ["architecture-review", "adversarial"]
            }

            req = urllib.request.Request(url, data=json.dumps(data).encode("utf-8"), headers=headers)
            try:
                with urllib.request.urlopen(req) as response:
                    print(f"Successfully created GitHub issue for {finding.get('finding_title')}")
            except Exception as e:
                print(f"Failed to create GitHub issue: {e}")
        else:
            print(f"No GH_TOKEN found, skipping GitHub issue creation for {finding.get('finding_title')}.")

if __name__ == "__main__":
    main()
