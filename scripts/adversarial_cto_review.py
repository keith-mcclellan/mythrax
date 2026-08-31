import os
import re
import urllib.request
import urllib.parse
import json
import uuid

def load_config():
    with open(".agents/agents/adversarial_cto.json", "r") as f:
        return json.load(f)

def get_system_prompt(config):
    return config["config"]["customAgent"]["systemPromptSections"][0]["content"]

def get_files_in_dir(directory, extensions):
    matched_files = []
    for root, _, files in os.walk(directory):
        for file in files:
            if any(file.endswith(ext) for ext in extensions):
                matched_files.append(os.path.join(root, file))
    return matched_files

def find_untested_functions():
    pub_fns = []
    for root, _, files in os.walk("mythrax-core/src"):
        for file in files:
            if not file.endswith(".rs"): continue
            filepath = os.path.join(root, file)
            with open(filepath, 'r', encoding='utf-8') as f:
                lines = f.readlines()
                for i, line in enumerate(lines):
                    if re.search(r'\bpub fn \w+', line):
                        fn_name = re.search(r'\bpub fn (\w+)', line).group(1)
                        pub_fns.append({
                            "file": filepath,
                            "line": i + 1,
                            "name": fn_name,
                            "scenario": f"Untested public function {fn_name}.",
                            "severity": "medium",
                            "fix": f"Add test coverage for {fn_name}.",
                            "effort": "Medium"
                        })

    test_files = get_files_in_dir("mythrax-core/src", [".rs"])
    test_files.extend(get_files_in_dir("evals", [".rs", ".py"]))

    test_content = ""
    for tf in test_files:
        with open(tf, 'r', encoding='utf-8') as f:
            content = f.read()
            if "#[test]" in content or "test" in tf.lower():
                test_content += content + "\n"

    untested = []
    for fn in pub_fns:
        if not re.search(r'\b' + re.escape(fn['name']) + r'\b', test_content):
            untested.append(fn)

    return untested

def call_llm_for_chunk(system_prompt, chunk_name, content):
    api_key = os.getenv("OPENAI_API_KEY")
    if not api_key:
        print(f"Mocking LLM call for {chunk_name} (No API key)")
        return []

    prompt = f"""Review the following codebase chunk: {chunk_name}

Output your findings as a JSON object with a "findings" key containing an array of objects. Each object must have the following keys:
- "file": The file path
- "line": The line number
- "scenario": A minimal reproducible scenario describing the bug
- "severity": The severity (e.g., "high", "medium", "low")
- "fix": A suggested fix
- "effort": Estimated effort to fix (e.g., "low", "medium", "high")

Only report bugs that match the criteria. For panics, only report .unwrap() or .expect() that could be triggered by realistic runtime inputs.
If no bugs are found, output {{"findings": []}}.

Codebase Chunk:
{content}
"""
    data = json.dumps({
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": prompt}
        ],
        "response_format": {"type": "json_object"}
    }).encode('utf-8')

    req = urllib.request.Request("https://api.openai.com/v1/chat/completions", data=data, headers={
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    })

    try:
        response = urllib.request.urlopen(req)
        result = json.loads(response.read().decode('utf-8'))
        msg = result['choices'][0]['message']['content']
        parsed = json.loads(msg)
        return parsed.get("findings", [])
    except Exception as e:
        print(f"LLM API Error for {chunk_name}: {e}")
        return []

def create_issue(finding):
    token = os.getenv("GH_TOKEN")

    issue_content = f"""
## File: {finding.get('file', 'Unknown')}
## Line: {finding.get('line', 'Unknown')}

**Severity:** {finding.get('severity', 'Unknown')}
**Scenario:** {finding.get('scenario', 'Unknown')}
**Effort:** {finding.get('effort', 'Unknown')}

**Suggested Fix:** {finding.get('fix', 'Unknown')}
"""

    print(f"\n--- Finding ---\n{issue_content}\n---------------")

    if token:
        repo = os.getenv("GITHUB_REPOSITORY", "user/repo")
        url = f"https://api.github.com/repos/{repo}/issues"
        data = json.dumps({
            "title": f"[Adversarial CTO] Issue in {finding.get('file', 'Unknown')}",
            "body": issue_content,
            "labels": ["bug", "agent-found"]
        }).encode('utf-8')
        req = urllib.request.Request(url, data=data, headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github.v3+json"
        })
        try:
            urllib.request.urlopen(req)
            print("Issue filed via GitHub API.")
        except Exception as e:
            print(f"Failed to file issue: {e}")
    else:
        os.makedirs("issues", exist_ok=True)
        issue_filename = f"issues/{uuid.uuid4().hex}.md"
        with open(issue_filename, "w") as f:
            f.write(issue_content)
        print(f"Issue saved locally to {issue_filename}")

def main():
    print("Running Adversarial CTO script...")
    config = load_config()
    system_prompt = get_system_prompt(config)
    print(f"System Prompt Loaded: {system_prompt[:50]}...")

    all_findings = []

    untested_fns = find_untested_functions()
    print(f"Found {len(untested_fns)} untested functions statically.")
    all_findings.extend(untested_fns)

    content_chunks = []
    current_chunk = ""
    for directory in ["mythrax-core", "evals", ".agents"]:
        if not os.path.isdir(directory):
            continue
        for root, _, files in os.walk(directory):
            for file in files:
                if file.endswith((".rs", ".py")):
                    filepath = os.path.join(root, file)
                    with open(filepath, 'r', encoding='utf-8') as f:
                        file_content = f"--- {filepath} ---\n{f.read()}\n"
                        if len(current_chunk) + len(file_content) > 60000:
                            content_chunks.append(current_chunk)
                            current_chunk = file_content
                        else:
                            current_chunk += file_content
    if current_chunk:
        content_chunks.append(current_chunk)

    print(f"Created {len(content_chunks)} chunks for LLM review.")
    for i, chunk in enumerate(content_chunks):
        print(f"Processing chunk {i+1}/{len(content_chunks)}...")
        findings = call_llm_for_chunk(system_prompt, f"Chunk {i+1}", chunk)
        all_findings.extend(findings)

    print(f"\nTotal findings to report: {len(all_findings)}")

    for f in all_findings:
        create_issue(f)

if __name__ == "__main__":
    main()
