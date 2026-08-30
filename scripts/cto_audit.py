import os
import re
import urllib.request
import json
import subprocess
import glob
from openai import OpenAI

def analyze_code_chunk(file_path, content, client):
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        return []

    prompt = f"""
    You are a CTO who has been burned by production incidents traced to subtle bugs that code review missed.
    Analyze this code from {file_path} for logic bugs, edge case vulnerabilities, and correctness issues:
    - Off-by-one errors in loops, index, or range operations
    - Panic paths (.unwrap() or .expect())
    - Integer overflow/underflow
    - Race conditions or incorrect async assumptions
    - Logic branches where code silently fails/succeeds incorrectly
    - Missing test coverage for public functions

    Return a JSON array of objects, each containing:
    - title: A descriptive bug title
    - body: A detailed bug description with file/line number, reproducible scenario, severity, and suggested fix.

    Here is the code:
    ```
    {content}
    ```
    """

    try:
        response = client.chat.completions.create(
            model="gpt-4o",
            response_format={ "type": "json_object" },
            messages=[
                {"role": "system", "content": "You are a senior code reviewer CTO. You output JSON with an 'issues' key containing a list of bug objects."},
                {"role": "user", "content": prompt}
            ]
        )

        content = response.choices[0].message.content
        data = json.loads(content)
        if "issues" in data:
             return data["issues"]
        elif isinstance(data, list):
             return data
        else:
             return []
    except Exception as e:
        print(f"Error querying OpenAI: {e}")
        return []

def issue_exists(title, token, repo):
    url = f"https://api.github.com/repos/{repo}/issues?state=all&per_page=100"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28"
    }

    try:
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req) as response:
            if response.status == 200:
                issues = json.loads(response.read().decode('utf-8'))
                for issue in issues:
                    if issue['title'] == title:
                        return True
            return False
    except Exception as e:
         print(f"Error checking existing issues: {e}")
         return False

def file_issue(title, body):
    token = os.environ.get('GH_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')

    if not token or not repo:
        print(f"Missing GH_TOKEN or GITHUB_REPOSITORY. Cannot file issue: {title}")
        return

    if issue_exists(title, token, repo):
        print(f"Issue already exists: {title}")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found"]
    }

    try:
        req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Successfully filed issue: {title}")
            else:
                print(f"Failed to file issue. Status: {response.status}")
    except Exception as e:
         print(f"Error making API request to file issue: {e}")

def get_files_to_analyze():
    important_files = []

    # Check for a PR diff
    try:
         result = subprocess.run(['git', 'diff', '--name-only', 'origin/main...HEAD'], capture_output=True, text=True)
         if result.returncode == 0 and result.stdout:
             changed = result.stdout.strip().split('\n')
             important_files.extend([f for f in changed if (f.endswith('.rs') or f.endswith('.py')) and os.path.exists(f)])
    except:
         pass

    # If no PR diff, or just a scheduled run, analyze core components
    if not important_files:
         for ext in ('*.rs', '*.py'):
             important_files.extend(glob.glob(f'mythrax-core/src/**/{ext}', recursive=True))
             important_files.extend(glob.glob(f'evals/**/{ext}', recursive=True))
             important_files.extend(glob.glob(f'.agents/**/*.md', recursive=True))

    return list(set(important_files))

def main():
    print("Starting CTO Bug Audit...")
    api_key = os.environ.get('OPENAI_API_KEY')
    issues_found = []

    if api_key:
        client = OpenAI(api_key=api_key)
        files = get_files_to_analyze()

        for f in files:
            try:
                with open(f, 'r') as fp:
                    content = fp.read()

                    if len(content) > 30000:
                         continue

                    print(f"Analyzing {f} via LLM...")
                    llm_issues = analyze_code_chunk(f, content, client)
                    if llm_issues:
                        issues_found.extend(llm_issues)
            except Exception as e:
                print(f"Failed to read or analyze {f}: {e}")

    for issue in issues_found:
        file_issue(issue['title'], issue['body'])
        print(f"Title: {issue['title']}\nBody: {issue['body']}\n\n")

if __name__ == '__main__':
    main()
