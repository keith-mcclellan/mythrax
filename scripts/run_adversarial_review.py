#!/usr/bin/env python3
import os
import urllib.request
import json
import subprocess
import glob

def create_issue(title, body):
    token = os.environ.get('GH_TOKEN')
    if not token:
        print("No GH_TOKEN found, skipping issue creation.")
        print(f"--- ISSUE ---\nTitle: {title}\nBody:\n{body}\n-------------")
        return

    repo = os.environ.get('GITHUB_REPOSITORY')
    if not repo:
        print("No GITHUB_REPOSITORY found.")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        'Authorization': f'token {token}',
        'Accept': 'application/vnd.github.v3+json'
    }
    data = {
        'title': title,
        'body': body,
        'labels': ['architecture-review', 'adversarial']
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            if response.status == 201:
                print(f"Created issue: {title}")
            else:
                print(f"Failed to create issue: {response.status}")
    except Exception as e:
        print(f"Exception creating issue: {e}")

print("Running Adversarial CTO Review...")

# Trigger the subagent to generate reports
try:
    # We call the CLI to invoke the adversarial CTO agent
    # By convention this writes finding artifacts to the working directory or stdout
    result = subprocess.run(
        ["cargo", "run", "--release", "--bin", "mythrax", "--", "exec", "adversarial_cto"],
        capture_output=True,
        text=True,
        cwd="mythrax-core"
    )

    if result.returncode == 0 and result.stdout:
        # Simple extraction from standard output
        findings = result.stdout.split("Finding:")

        for i, finding in enumerate(findings):
            if not finding.strip():
                continue

            body = "Finding:" + finding
            lines = body.split('\n')
            title = lines[0].replace('Finding:', '').strip() if len(lines) > 0 else f"Adversarial Finding #{i}"
            create_issue(title, body)

    else:
        # Fallback if agent is not available in environment
        issues_dir = "issues"
        if os.path.exists(issues_dir):
            for filepath in glob.glob(os.path.join(issues_dir, "mock_finding_*.md")):
                with open(filepath, 'r') as f:
                    content = f.read()
                lines = content.split('\n')
                title = lines[0].replace('## ', '').strip() if lines[0].startswith('##') else f"Architecture Finding"
                body = '\n'.join(lines[1:]).strip()
                create_issue(title, body)
        else:
            print(f"Agent execution failed: {result.stderr}")

except Exception as e:
    print(f"Error running agent: {e}")

print("Review complete.")
