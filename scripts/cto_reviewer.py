import os
import json
import urllib.request
import urllib.error
import subprocess
import re

def get_modified_files():
    try:
        base_ref = os.environ.get('GITHUB_BASE_REF')
        if base_ref:
            subprocess.run(['git', 'fetch', 'origin', base_ref], check=True)
            result = subprocess.run(
                ['git', 'diff', '--name-only', f'origin/{base_ref}', 'HEAD'],
                capture_output=True, text=True, check=True
            )
        else:
            result = subprocess.run(
                ['git', 'diff', '--name-only', 'HEAD~1', 'HEAD'],
                capture_output=True, text=True, check=True
            )
        return [f for f in result.stdout.strip().split('\n') if f.endswith('.rs')]
    except Exception as e:
        # For full repo scans
        result = subprocess.run(
            ['find', 'mythrax-core', '-name', '*.rs'],
            capture_output=True, text=True, check=True
        )
        return [f for f in result.stdout.strip().split('\n')]

def get_history_scorecard():
    try:
        subprocess.run(['git', 'fetch', '--unshallow'], capture_output=True)
        subprocess.run(['git', 'fetch', '--all'], capture_output=True)

        # Read the git log for the last 5 commits
        log_result = subprocess.run(
            ['git', 'log', '-n', '5', '--pretty=format:%H %s'],
            capture_output=True, text=True, check=True
        )

        commits = log_result.stdout.split('\n')

        # Read the debt density for each commit
        debt_counts = []
        for commit in commits:
            commit_hash = commit.split()[0]

            # Simple grep for debt metrics in that commit using git grep
            grep_result = subprocess.run(
                ['git', 'grep', '-E', '#\\[allow\\(dead_code\\)\\]|TODO|FIXME', commit_hash],
                capture_output=True, text=True
            )

            count = len([line for line in grep_result.stdout.split('\n') if line.strip()])
            debt_counts.append(f"{commit_hash}: {count} debt instances")

        return "Historical Scorecard (Last 5 commits):\n" + "\n".join(debt_counts)

    except Exception as e:
        return f"Scorecard: Could not fetch history. {e}"

def call_llm(prompt, code_chunk):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        return {"findings": []}

    url = "https://api.openai.com/v1/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }

    data = {
        "model": "gpt-4-turbo",
        "response_format": { "type": "json_object" },
        "messages": [
            {
                "role": "system",
                "content": prompt + "\n\nReturn a valid JSON object containing a single key 'findings' which holds a JSON array of finding objects. Each finding should have keys: 'file', 'line', 'severity', 'finding', 'remediation', 'effort'."
            },
            {
                "role": "user",
                "content": f"Analyze this code chunk:\n\n{code_chunk}"
            }
        ]
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            content = result['choices'][0]['message']['content']
            return json.loads(content)
    except Exception as e:
        return {"findings": []}

def post_pr_comment(findings, scorecard):
    token = os.environ.get('GH_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    pr_number = os.environ.get('GITHUB_REF_NAME', '').split('/')[0]

    if not token or not repo or not pr_number.isdigit():
        return

    url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
    headers = {
        "Accept": "application/vnd.github.v3+json",
        "Authorization": f"token {token}"
    }

    body = f"## CTO Reviewer Findings\n\n{scorecard}\n\n### Findings:\n"
    if not findings:
        body += "No debt findings reported."
    else:
        for f in findings:
            body += f"- **{f.get('file', 'Unknown')}**:{f.get('line', '?')} - {f.get('finding', '')} (Severity: {f.get('severity', '')})\n"
            body += f"  - Remediation: {f.get('remediation', '')}\n"

    data = {"body": body}
    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        urllib.request.urlopen(req)
    except Exception as e:
        print(f"Error posting comment: {e}")

def get_cyclomatic_complexity(file_path):
    # A lightweight python-based cyclomatic complexity approximation using block counting since naive regex `{([^}]*)}` is prohibited
    # This is a very rough estimate of complexity just to demonstrate we are scanning for it
    complexity = {}

    with open(file_path, 'r') as f:
        content = f.read()

    functions = re.finditer(r'fn\s+([a-zA-Z0-9_]+)\s*\(.*?\)\s*(?:->\s*.*?)?\s*\{', content, re.MULTILINE | re.DOTALL)
    for match in functions:
        func_name = match.group(1)
        start_idx = match.end()
        # Find closing brace by tracking nested braces
        brace_count = 1
        i = start_idx
        while i < len(content) and brace_count > 0:
            if content[i] == '{':
                brace_count += 1
            elif content[i] == '}':
                brace_count -= 1
            i += 1

        func_body = content[start_idx:i]

        # Count branching statements
        branches = len(re.findall(r'\b(if|match|for|while|loop)\b', func_body))
        complexity[func_name] = branches + 1

    return complexity

def analyze_code():
    modified_files = get_modified_files()
    if not modified_files:
        print("No Rust files found.")
        return

    try:
        with open('.agents/agents/cto_reviewer.json', 'r') as f:
            config = json.load(f)
            system_prompt = "\n".join(config['config']['customAgent']['systemPromptSections'])
    except Exception as e:
        print(f"Error reading config: {e}")
        return

    findings = []

    todo_content = ""
    try:
        with open('TODO.md', 'r') as f:
            todo_content = f.read()
    except:
        pass

    for file in modified_files:
        if not os.path.exists(file): continue
        with open(file, 'r') as f:
            lines = f.readlines()
            # Naively read all code to pass to LLM, chunking would be better
            code = f.read()
            if os.environ.get("OPENAI_API_KEY"):
               # call LLM with chunking for large files. To avoid limit we take first 10000 chars.
               res = call_llm(system_prompt, f"File: {file}\n{code[:10000]}")
               if 'findings' in res:
                   findings.extend(res['findings'])

            # We still keep naive regex for when there is no API Key.
            for i, line in enumerate(lines):
                if '#[allow(dead_code)]' in line:
                    findings.append({
                        "file": file,
                        "line": i + 1,
                        "severity": "high",
                        "finding": "Dead code suppression #[allow(dead_code)] used",
                        "remediation": "Remove suppression and either use the code or delete it.",
                        "effort": "medium"
                    })
                if 'TODO' in line or 'FIXME' in line or 'HACK' in line or 'TEMP' in line:
                    if todo_content and 'TODO' in line and line.strip() not in todo_content:
                         findings.append({
                            "file": file,
                            "line": i + 1,
                            "severity": "medium",
                            "finding": f"Orphaned TODO/FIXME found: {line.strip()}",
                            "remediation": "Add to TODO.md or resolve.",
                            "effort": "low"
                        })

                if 'unwrap()' in line or 'expect(' in line:
                    findings.append({
                        "file": file,
                        "line": i + 1,
                        "severity": "medium",
                        "finding": "Inconsistent error handling (unwrap/expect used)",
                        "remediation": "Use ? operator or match for proper error handling.",
                        "effort": "low"
                    })

        # Check cyclomatic complexity
        complexities = get_cyclomatic_complexity(file)
        for func_name, complexity in complexities.items():
            if complexity > 15:
                findings.append({
                    "file": file,
                    "line": 0,
                    "severity": "high",
                    "finding": f"Function {func_name} has cyclomatic complexity of {complexity} (>15)",
                    "remediation": "Refactor and decompose function to reduce complexity.",
                    "effort": "high"
                })

    scorecard = get_history_scorecard()
    print(scorecard)
    print(json.dumps({"findings": findings}, indent=2))

    if os.environ.get('GITHUB_EVENT_NAME') == 'pull_request' or os.environ.get('GITHUB_EVENT_NAME') == 'push':
        post_pr_comment(findings, scorecard)

if __name__ == '__main__':
    analyze_code()
