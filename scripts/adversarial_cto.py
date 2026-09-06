import os
import sys
import json
import subprocess
import urllib.request
import traceback

def chunk_text(text, max_chars=40000):
    """Splits text into chunks to avoid LLM context window limits."""
    chunks = []
    current_chunk = []
    current_length = 0

    for line in text.split('\n'):
        if current_length + len(line) > max_chars:
            chunks.append('\n'.join(current_chunk))
            current_chunk = [line]
            current_length = len(line)
        else:
            current_chunk.append(line)
            current_length += len(line)

    if current_chunk:
        chunks.append('\n'.join(current_chunk))

    return chunks

def call_llm(prompt):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("Warning: OPENAI_API_KEY not found. Bypassing LLM logic (degrading gracefully).")
        return {"findings": []}

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json"
    }

    data = {
        "model": "gpt-4o", # Replace with appropriate model
        "response_format": { "type": "json_object" },
        "messages": [
            {
                "role": "system",
                "content": """You are an Adversarial CTO analyzing code for subtle bugs.
Analyze for:
- Off-by-one errors (loops, indexes, especially AI context window logic in .agents/)
- Panic paths (.unwrap(), .expect()) triggered by external/LLM data
- Integer overflow/underflow in scoring, token counting, or eval metrics
- Race conditions or async execution order assumptions
- Logic branches with silent success but incorrect results
- Test coverage gaps (public functions with no tests)

Return a valid JSON object with a single key 'findings' containing a list of objects.
Each object must have: file, line, title, severity, scenario, fix."""
            },
            {
                "role": "user",
                "content": prompt
            }
        ]
    }

    try:
        req = urllib.request.Request(url, json.dumps(data).encode('utf-8'), headers, method="POST")
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            return json.loads(result['choices'][0]['message']['content'])
    except Exception as e:
        print(f"LLM API Error: {e}")
        return {"findings": []}

def analyze_diff(diff):
    all_findings = []
    chunks = chunk_text(diff)
    for chunk in chunks:
        prompt = f"Analyze the following code diff:\n\n```diff\n{chunk}\n```"
        result = call_llm(prompt)
        all_findings.extend(result.get('findings', []))
    return all_findings

def scan_codebase():
    all_findings = []
    try:
        # Find all relevant files
        cmd = ["find", "mythrax-core/src", ".agents", "evals", "-type", "f", "-name", "*.rs", "-o", "-name", "*.py", "-o", "-name", "*.ts", "-o", "-name", "*.js"]
        files_out = subprocess.check_output(cmd, text=True)
        files = [f for f in files_out.split('\n') if f]

        # Batch process files to avoid too many API calls
        current_batch = ""
        for file in files:
            if not os.path.exists(file): continue
            with open(file, 'r', encoding='utf-8') as f:
                content = f.read()
                file_text = f"--- File: {file} ---\n{content}\n\n"

                if len(current_batch) + len(file_text) > 40000:
                    result = call_llm(f"Analyze the following codebase files:\n\n{current_batch}")
                    all_findings.extend(result.get('findings', []))
                    current_batch = file_text
                else:
                    current_batch += file_text

        if current_batch:
            result = call_llm(f"Analyze the following codebase files:\n\n{current_batch}")
            all_findings.extend(result.get('findings', []))

    except Exception as e:
        print(f"Error scanning codebase: {e}")

    return all_findings

def create_github_issue(token, repo, finding):
    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github.v3+json"
    }

    body = f"**File:** {finding.get('file', 'Unknown')} (Line {finding.get('line', 'Unknown')})\n"
    body += f"**Severity:** {finding.get('severity', 'Unknown')}\n\n"
    body += f"**Scenario:**\n{finding.get('scenario', 'Unknown')}\n\n"
    body += f"**Suggested Fix:**\n{finding.get('fix', 'Unknown')}\n"

    data = {
        "title": f"Bug: {finding.get('title', 'Unknown Issue')} in {os.path.basename(finding.get('file', 'Unknown'))}",
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    try:
        req = urllib.request.Request(url, json.dumps(data).encode('utf-8'), headers, method="POST")
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Created issue: {data['title']}")
            else:
                print(f"Failed to create issue. Status: {response.status}")
    except urllib.error.URLError as e:
        print(f"Failed to create issue (Network Error/No Access): {e}")
    except Exception as e:
        print(f"Exception during issue creation: {e}")

def main():
    print("Adversarial CTO Scan started...")

    repo = os.environ.get("GITHUB_REPOSITORY")
    token = os.environ.get("GITHUB_TOKEN")

    # Check if we are running in a PR by checking for base ref
    base_ref = os.environ.get("GITHUB_BASE_REF")

    findings = []

    if base_ref:
        print(f"PR context detected. Base ref: {base_ref}. Running differential scan.")
        try:
            # fetch base and compare
            subprocess.run(["git", "fetch", "origin", base_ref], check=False)
            diff_out = subprocess.check_output(["git", "diff", "--name-only", f"origin/{base_ref}", "HEAD"], text=True)
            modified_files = [f for f in diff_out.split('\n') if f]

            if not modified_files:
                print("No changes found in PR. Exiting.")
                sys.exit(0)

            diff_content = subprocess.check_output(["git", "diff", f"origin/{base_ref}", "HEAD"], text=True)
            findings = analyze_diff(diff_content)
        except Exception as e:
            print(f"Failed to run diff: {e}")
            findings = []
    else:
        print("Scheduled/Main context detected. Running full codebase scan.")
        findings = scan_codebase()

    print(f"Scan complete. Found {len(findings)} potential issues.")

    if repo and token:
        print("GitHub context found. Creating issues...")
        for finding in findings:
            create_github_issue(token, repo, finding)
    else:
        print("No GitHub token or repository found. Skipping issue creation.")
        # Generate mock issues locally in 'issues/' dir instead
        if findings:
            os.makedirs("issues", exist_ok=True)
            # Remove existing mock issues to keep it clean
            for f in os.listdir("issues"):
                if f.startswith("mock_issue_") and f.endswith(".md"):
                    os.remove(os.path.join("issues", f))

            for i, finding in enumerate(findings):
                with open(f"issues/mock_issue_{i+1}.md", "w") as f:
                    f.write(f"# Bug: {finding.get('title', 'Unknown')} in {os.path.basename(finding.get('file', 'Unknown'))}\n\n")
                    f.write(f"**Labels:** bug, agent-found\n\n")
                    f.write(f"**File:** {finding.get('file', 'Unknown')} (Line {finding.get('line', 'Unknown')})\n")
                    f.write(f"**Severity:** {finding.get('severity', 'Unknown')}\n\n")
                    f.write(f"**Scenario:**\n{finding.get('scenario', 'Unknown')}\n\n")
                    f.write(f"**Suggested Fix:**\n{finding.get('fix', 'Unknown')}\n")
            print(f"Generated {len(findings)} mock issues in 'issues/' directory.")
        else:
            print("No findings to report.")

    print("Adversarial CTO Scan finished.")

if __name__ == "__main__":
    main()
