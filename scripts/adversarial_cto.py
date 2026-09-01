import os
import json
import urllib.request
import sys
import subprocess

def get_all_files():
    files_to_check = []
    for root, _, files in os.walk("mythrax-core/src"):
        for file in files:
            if file.endswith(".rs"): files_to_check.append(os.path.join(root, file))
    for root, _, files in os.walk("evals"):
        for file in files:
            if file.endswith((".py", ".sh", ".rs")): files_to_check.append(os.path.join(root, file))
    for root, _, files in os.walk(".agents"):
        for file in files:
            if file.endswith(".json"): files_to_check.append(os.path.join(root, file))
    return files_to_check

def get_files_to_check():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "")

    if event_name == "pull_request":
        print("PR detected, analyzing only modified files...")
        try:
            base_ref = os.environ.get("GITHUB_BASE_REF", "main")
            diff_cmd = f"git diff --name-only origin/{base_ref}"
            output = subprocess.check_output(diff_cmd, shell=True, text=True)
            changed_files = output.strip().split('\n')
            files_to_check = []
            for f in changed_files:
                if f and os.path.exists(f) and f.endswith((".rs", ".py", ".sh", ".json")):
                    if f.startswith("mythrax-core/src") or f.startswith("evals") or f.startswith(".agents"):
                        files_to_check.append(f)
            return files_to_check
        except Exception as e:
            print(f"Failed to get diff files: {e}. Falling back to full scan.")
            return get_all_files()
    else:
        print("Full run detected, analyzing all relevant files...")
        return get_all_files()

def chunk_files(files_to_check, max_chars=40000):
    chunks = []
    current_chunk = ""

    for f in files_to_check:
        try:
            with open(f, "r", encoding="utf-8", errors="ignore") as fp:
                content = fp.read()
                file_str = f"\n--- {f} ---\n{content}\n"
                if len(current_chunk) + len(file_str) > max_chars and current_chunk:
                    chunks.append(current_chunk)
                    current_chunk = file_str
                else:
                    current_chunk += file_str
        except Exception as e:
            print(f"Error reading {f}: {e}")

    if current_chunk:
        chunks.append(current_chunk)

    return chunks

def run_analysis():
    agent_config = json.load(open(".agents/agents/adversarial_cto/agent.json"))
    prompt = agent_config["config"]["customAgent"]["systemPromptSections"][0]["content"]

    files_to_check = get_files_to_check()
    if not files_to_check:
        print("No files to check.")
        return

    chunks = chunk_files(files_to_check)
    print(f"Divided files into {len(chunks)} chunks.")

    api_key = os.environ.get("OPENAI_API_KEY")
    gh_token = os.environ.get("GH_TOKEN")

    if not api_key:
        print("Skipping LLM analysis, no API key")
        return

    all_findings = []
    has_errors = False

    for i, chunk in enumerate(chunks):
        print(f"Processing chunk {i+1}/{len(chunks)}...")
        req = urllib.request.Request("https://api.openai.com/v1/chat/completions", method="POST")
        req.add_header("Authorization", f"Bearer {api_key}")
        req.add_header("Content-Type", "application/json")

        data = {
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": prompt},
                {"role": "user", "content": chunk}
            ],
            "response_format": {"type": "json_object"}
        }

        try:
            response = urllib.request.urlopen(req, data=json.dumps(data).encode("utf-8"))
            res_body = json.loads(response.read())
            content_str = res_body["choices"][0]["message"]["content"]
            findings_dict = json.loads(content_str)

            if isinstance(findings_dict, dict) and "findings" in findings_dict:
                findings = findings_dict["findings"]
                all_findings.extend(findings)
            else:
                print("Unexpected JSON structure:", findings_dict)
                has_errors = True
        except Exception as e:
            print(f"Error calling LLM for chunk {i+1}: {e}")
            has_errors = True

    print("--- ADVERSARIAL CTO FINDINGS ---")
    for f in all_findings:
        print(json.dumps(f, indent=2))

        if gh_token:
            print(f"Filing issue for {f.get('title')}")
            issue_title = f.get('title', 'Adversarial CTO Finding')
            issue_body = f"{f.get('description', '')}\n\n**File:** {f.get('file')}:{f.get('line')}\n**Scenario:** {f.get('scenario')}\n**Severity:** {f.get('severity')}\n**Suggested Fix:** {f.get('fix')}\n**Effort:** {f.get('effort')}"
            issue_labels = f.get('labels', ['bug', 'agent-found'])

            repo = os.environ.get("GITHUB_REPOSITORY")
            if repo:
                issue_req = urllib.request.Request(
                    f"https://api.github.com/repos/{repo}/issues",
                    method="POST"
                )
                issue_req.add_header("Authorization", f"Bearer {gh_token}")
                issue_req.add_header("Accept", "application/vnd.github.v3+json")
                issue_req.add_header("Content-Type", "application/json")

                issue_data = {
                    "title": issue_title,
                    "body": issue_body,
                    "labels": issue_labels
                }

                try:
                    issue_resp = urllib.request.urlopen(issue_req, data=json.dumps(issue_data).encode("utf-8"))
                    print(f"Successfully filed issue: {issue_resp.getcode()}")
                except Exception as e:
                    print(f"Failed to file issue: {e}")
                    has_errors = True
            else:
                print("GITHUB_REPOSITORY not set, skipping issue creation")

    if has_errors:
        sys.exit(1)

if __name__ == "__main__":
    run_analysis()
