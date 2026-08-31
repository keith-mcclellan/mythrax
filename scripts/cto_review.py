import json
import os
import urllib.request
import urllib.error

def main():
    agent_json_path = ".agents/agents/cto_reviewer/agent.json"
    with open(agent_json_path, "r") as f:
        config = json.load(f)

    system_prompt_sections = config.get("config", {}).get("customAgent", {}).get("systemPromptSections", [])
    system_prompt = "\n".join([sec.get("content", "") for sec in system_prompt_sections])

    target_dirs = ["mythrax-core", "evals", ".agents"]
    codebase_content = ""
    for d in target_dirs:
        for root, _, files in os.walk(d):
            for file in files:
                if file.endswith((".rs", ".py", ".md", ".json", ".sh")):
                    filepath = os.path.join(root, file)
                    try:
                        with open(filepath, "r") as f:
                            codebase_content += f"\n--- {filepath} ---\n{f.read()}\n"
                    except Exception:
                        pass

    openai_api_key = os.environ.get("OPENAI_API_KEY")
    if not openai_api_key:
        print("Warning: OPENAI_API_KEY not set. Cannot perform genuine LLM call.")
        return

    messages = [
        {"role": "system", "content": system_prompt},
        {"role": "user", "content": f"Please review the following codebase:\n{codebase_content[:80000]}"}
    ]

    req_data = json.dumps({
        "model": "gpt-4-turbo",
        "messages": messages,
        "response_format": {"type": "json_object"}
    }).encode("utf-8")

    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions",
        data=req_data,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {openai_api_key}"
        }
    )

    try:
        with urllib.request.urlopen(req) as response:
            res_data = json.loads(response.read().decode("utf-8"))
            llm_output = res_data["choices"][0]["message"]["content"]
            findings_data = json.loads(llm_output)
            findings = findings_data.get("findings", [])
    except Exception as e:
        print(f"Error calling LLM: {e}")
        return

    gh_token = os.environ.get("GH_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY", "user/repo")

    for finding in findings:
        title = f"Bug in {finding.get('file')}:{finding.get('line')} - {finding.get('severity')} severity"
        body = f"""**File:** {finding.get('file')}
**Line:** {finding.get('line')}
**Severity:** {finding.get('severity')}

**Scenario:**
{finding.get('scenario')}

**Suggested Fix:**
{finding.get('fix')}

**Effort Estimate:**
{finding.get('effort_estimate')}
"""
        print(f"Finding: {title}")
        print(body)
        print("-" * 40)

        if gh_token:
            issue_req_data = json.dumps({
                "title": title,
                "body": body,
                "labels": ["bug", "agent-found"]
            }).encode("utf-8")

            issue_req = urllib.request.Request(
                f"https://api.github.com/repos/{repo}/issues",
                data=issue_req_data,
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {gh_token}",
                    "Accept": "application/vnd.github.v3+json"
                }
            )

            try:
                with urllib.request.urlopen(issue_req) as issue_res:
                    print(f"Successfully filed issue for {finding.get('file')}:{finding.get('line')}")
            except Exception as e:
                print(f"Error filing issue: {e}")

if __name__ == "__main__":
    main()
