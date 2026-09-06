import os
import json
import urllib.request
import subprocess

def get_files_to_analyze():
    is_pr = os.environ.get('GITHUB_EVENT_NAME') == 'pull_request'
    if is_pr:
        base_ref = os.environ.get('GITHUB_BASE_REF')
        try:
            subprocess.run(['git', 'fetch', 'origin', base_ref], check=False)
            output = subprocess.check_output(['git', 'diff', '--name-only', f'origin/{base_ref}', 'HEAD']).decode('utf-8')
            return [f for f in output.split('\n') if f.strip() and os.path.exists(f)]
        except Exception:
            return []
    else:
        files = []
        for d in ['mythrax-core/src', 'mythrax-core/tests', '.agents']:
            if not os.path.exists(d):
                continue
            for root, _, filenames in os.walk(d):
                for f in filenames:
                    if f.endswith('.rs') or f.endswith('.md') or f.endswith('.py'):
                        files.append(os.path.join(root, f))
        return files

def chunk_content(content, chunk_size=8000):
    return [content[i:i+chunk_size] for i in range(0, len(content), chunk_size)]

def analyze_chunk_with_llm(filename, chunk):
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        print("OPENAI_API_KEY missing, bypassing LLM logic gracefully.")
        return []

    prompt = f"""
Analyze the following code snippet from `{filename}` for logic bugs, edge case vulnerabilities, and correctness issues.
Focus on:
- Off-by-one errors (loops, indexes, ranges)
- Panic paths (.unwrap(), .expect())
- Integer overflow/underflow
- Race conditions or async execution order assumptions
- Logic branches with incorrect results (silent failures)
- Test coverage gaps (public functions with no tests)

Return a JSON object with a single key `findings`. Its value must be a list of objects, each containing:
- `file`: string
- `line`: string or number
- `severity`: string (High, Medium, Low)
- `scenario`: string
- `suggested_fix`: string

Code:
{chunk}
"""
    try:
        req = urllib.request.Request(
            'https://api.openai.com/v1/chat/completions',
            headers={
                'Content-Type': 'application/json',
                'Authorization': f'Bearer {api_key}'
            },
            data=json.dumps({
                "model": "gpt-4-turbo",
                "response_format": { "type": "json_object" },
                "messages": [
                    {"role": "system", "content": "You are a CTO performing a stringent code review. You assume the code is wrong until proven otherwise."},
                    {"role": "user", "content": prompt}
                ]
            }).encode('utf-8')
        )

        with urllib.request.urlopen(req) as response:
            res_body = json.loads(response.read().decode('utf-8'))
            content = res_body['choices'][0]['message']['content']
            parsed = json.loads(content)
            return parsed.get('findings', [])
    except Exception as e:
        print(f"Error calling OpenAI API: {e}")
        return []

def create_github_issue(finding):
    token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    if not token or not repo:
        print("Missing GITHUB_TOKEN or GITHUB_REPOSITORY, cannot create issue.")
        print(json.dumps(finding, indent=2))
        return

    title = f"Bug found in {finding.get('file', 'unknown')}:{finding.get('line', 'unknown')}"
    body = f"**File:** {finding.get('file')}\n**Line:** {finding.get('line')}\n**Severity:** {finding.get('severity')}\n**Scenario:** {finding.get('scenario')}\n**Suggested Fix:** {finding.get('suggested_fix')}"

    url = f"https://api.github.com/repos/{repo}/issues"
    try:
        req = urllib.request.Request(
            url,
            headers={
                'Authorization': f'Bearer {token}',
                'Accept': 'application/vnd.github.v3+json',
                'Content-Type': 'application/json'
            },
            data=json.dumps({
                "title": title,
                "body": body,
                "labels": ["bug", "agent-found"]
            }).encode('utf-8')
        )
        with urllib.request.urlopen(req) as response:
            print(f"Created issue for {finding.get('file')}:{finding.get('line')}")
    except Exception as e:
        print(f"Error creating GitHub issue: {e}")

def main():
    files = get_files_to_analyze()
    for f in files:
        if not os.path.exists(f):
            continue
        try:
            with open(f, 'r', encoding='utf-8') as file_obj:
                content = file_obj.read()
        except Exception:
            continue

        chunks = chunk_content(content)
        for chunk in chunks:
            findings = analyze_chunk_with_llm(f, chunk)
            for finding in findings:
                create_github_issue(finding)

if __name__ == '__main__':
    main()
