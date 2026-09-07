import os
import sys
import subprocess
import urllib.request
import json
import re

def get_modified_files():
    event_name = os.environ.get('GITHUB_EVENT_NAME')
    if event_name == 'pull_request':
        base_sha = os.environ.get('GITHUB_BASE_SHA')
        head_sha = os.environ.get('GITHUB_HEAD_SHA')
        if base_sha and head_sha:
            try:
                # Need to fetch first if we're in a shallow clone
                subprocess.run(['git', 'fetch', '--unshallow'], stderr=subprocess.DEVNULL)
                subprocess.run(['git', 'fetch', '--all'], stderr=subprocess.DEVNULL)
                output = subprocess.check_output(['git', 'diff', '--name-only', f'{base_sha}..{head_sha}']).decode('utf-8')
                return [f for f in output.split('\n') if f.endswith('.rs') or f.endswith('.py') or f.endswith('.md')]
            except subprocess.CalledProcessError:
                pass
    return None # All files

def find_test_coverage_gaps(files_to_analyze=None):
    print("Analyzing test coverage gaps...")
    public_funcs_no_test = []

    pub_fns = []

    # 1. Extract pub fn
    if files_to_analyze is not None:
        # Only check modified files for PR
        for filepath in files_to_analyze:
            if filepath.endswith('.rs') and os.path.exists(filepath):
                try:
                    with open(filepath, 'r', encoding='utf-8') as file:
                        content = file.read()
                        matches = re.finditer(r'pub\s+(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*\(', content)
                        for match in matches:
                            fn_name = match.group(1)
                            pub_fns.append({'name': fn_name, 'file': filepath})
                except Exception:
                    pass
    else:
        # Full scan
        for root, _, files in os.walk('mythrax-core/src'):
            for f in files:
                if f.endswith('.rs'):
                    filepath = os.path.join(root, f)
                    try:
                        with open(filepath, 'r', encoding='utf-8') as file:
                            content = file.read()
                            matches = re.finditer(r'pub\s+(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*\(', content)
                            for match in matches:
                                fn_name = match.group(1)
                                pub_fns.append({'name': fn_name, 'file': filepath})
                    except Exception:
                        pass

    # 2. Check for tests
    test_files_content = ""
    for root, _, files in os.walk('mythrax-core/tests'):
        for f in files:
            if f.endswith('.rs'):
                try:
                    with open(os.path.join(root, f), 'r', encoding='utf-8') as file:
                        test_files_content += file.read() + "\n"
                except Exception:
                    pass

    src_test_content = ""
    for root, _, files in os.walk('mythrax-core/src'):
        for f in files:
            if f.endswith('.rs'):
                try:
                    with open(os.path.join(root, f), 'r', encoding='utf-8') as file:
                         content = file.read()
                         # Very basic heuristic: extract text inside #[cfg(test)] modules
                         matches = re.finditer(r'#\[cfg\(test\)\]\s*mod\s+[^{]+\{([^}]+)\}', content, re.DOTALL)
                         for match in matches:
                             src_test_content += match.group(1) + "\n"
                except Exception:
                    pass

    all_test_content = test_files_content + src_test_content

    for fn_info in pub_fns:
        fn_name = fn_info['name']
        # If the function name doesn't appear anywhere in test code, we assume it's untested
        # We look for `fn_name(` or `fn_name::` or just `fn_name` as a whole word
        pattern = r'\b' + re.escape(fn_name) + r'\b'
        if not re.search(pattern, all_test_content):
            public_funcs_no_test.append({
                'file': fn_info['file'],
                'line': 0, # Don't have exact line, 0 is fine
                'finding': f"Public function `{fn_name}` has no corresponding test coverage in `tests/` or `#[cfg(test)]` modules.",
                'severity': 'Medium',
                'fix': f"Add tests for `{fn_name}` in the corresponding eval or test suite.",
                'rule': 'Test Coverage Gap',
                'scenario': f"Function `{fn_name}` might be called in unexpected ways. Without tests, there is a risk of regressions when modifying its implementation."
            })

    return public_funcs_no_test

def analyze_file(filepath):
    print(f"Analyzing {filepath}")
    findings = []

    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            lines = f.readlines()

        for i, line in enumerate(lines):
            line_num = i + 1

            # Simple heuristic checks (CTO rules)
            # Panic paths
            if '.unwrap()' in line or '.expect(' in line:
                findings.append({
                    'file': filepath,
                    'line': line_num,
                    'finding': "Panic path detected (.unwrap() or .expect()). This can crash the system on invalid external data.",
                    'severity': 'High',
                    'fix': "Return a Result and propagate the error using `?` or handle it explicitly with a `match`.",
                    'rule': 'Panic Path',
                    'scenario': "External input fails to parse or resource is missing, causing the process to panic and die unexpectedly."
                })

    except Exception as e:
        print(f"Error reading {filepath}: {e}")

    return findings

def issue_exists(repo, token, title):
    """Check if an issue with the given title already exists (open or closed)."""
    # Use search API to find issues with exact title
    # To handle spaces in title, we can just search for the specific finding rule and file
    # For a robust approach, we just fetch issues and match title
    query = urllib.parse.quote(f'repo:{repo} "{title}" in:title')
    url = f"https://api.github.com/search/issues?q={query}"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json"
    }

    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            return result.get('total_count', 0) > 0
    except Exception as e:
        print(f"Error checking existing issues: {e}")
        return False

def file_github_issue(finding):
    repo = os.environ.get('GITHUB_REPOSITORY')
    token = os.environ.get('GITHUB_TOKEN')

    scenario = finding.get('scenario', 'A realistic runtime input could trigger this path, causing a crash or incorrect behavior.')
    title = f"Bug: {finding['rule']} in {finding['file']}:{finding['line']}"

    if not repo or not token:
        print("Missing GITHUB_REPOSITORY or GITHUB_TOKEN, cannot file issue.")
        # Local fallback for tests
        os.makedirs("issues", exist_ok=True)
        with open(f"issues/issue_{finding['file'].replace('/', '_')}_{finding['line']}.md", 'w') as f:
            f.write(f"# {title}\n\n")
            f.write(f"**Finding:** {finding['finding']}\n")
            f.write(f"**Severity:** {finding['severity']}\n")
            f.write(f"**Suggested Fix:** {finding['fix']}\n")
            f.write(f"**Minimal Reproducible Scenario:** {scenario}\n")
        return

    # Deduplication
    if issue_exists(repo, token, title):
        print(f"Issue already exists, skipping: {title}")
        return

    url = f"https://api.github.com/repos/{repo}/issues"
    headers = {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json"
    }

    body = f"""**Red Team Architecture Brief**

**Finding:** {finding['finding']}
**File:** `{finding['file']}:{finding['line']}`
**Severity:** {finding['severity']}

**Attack Scenario / Blast Radius:**
{scenario}

**Recommended Structural Change:**
{finding['fix']}

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
"""
    data = {
        "title": title,
        "body": body,
        "labels": ["bug", "agent-found", "architecture-review", "adversarial"]
    }

    req = urllib.request.Request(url, data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            print(f"Filed issue: {response.getcode()}")
    except urllib.error.URLError as e:
        print(f"Failed to file issue: {e}")

def get_llm_findings(files_content):
    """
    Use an LLM (if API key available) to analyze code for logic bugs, edge cases,
    and correctness issues as requested by the Adversarial CTO persona.
    """
    api_key = os.environ.get('OPENAI_API_KEY')
    if not api_key:
        return []

    # Format the prompt
    prompt = """
You are a CTO who has been burned by production incidents traced to subtle bugs that code review missed.
You are methodical and assume the code is wrong until proven otherwise.
Analyze the provided code for logic bugs, edge case vulnerabilities, and correctness issues:
• Off-by-one errors in any loop, index, or range operation
• Panic paths: any .unwrap() or .expect() that could be triggered by realistic runtime inputs
• Integer overflow or underflow risks in arithmetic
• Race conditions or incorrect assumptions about execution order
• Logic branches where the code silently succeeds but produces an incorrect result

Return a JSON object with a single key 'findings' which holds a JSON array of results. Each result should have:
- 'file': filename
- 'line': approximate line number
- 'finding': description of the bug
- 'severity': High, Medium, Low
- 'fix': suggested fix
- 'rule': category of the bug
- 'scenario': a minimal reproducible scenario that demonstrates how this bug could be triggered in a realistic environment
    """

    data = {
        "model": "gpt-4-turbo",
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": json.dumps(files_content)}
        ],
        "response_format": {"type": "json_object"}
    }

    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json"
    }

    req = urllib.request.Request("https://api.openai.com/v1/chat/completions", data=json.dumps(data).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            result = json.loads(response.read().decode('utf-8'))
            content = result['choices'][0]['message']['content']
            parsed = json.loads(content)
            return parsed.get('findings', [])
    except Exception as e:
        print(f"Error calling LLM: {e}")
        return []

def chunk_files(files_to_analyze, chunk_size=3):
    """Split files into chunks to fit context window."""
    for i in range(0, len(files_to_analyze), chunk_size):
        yield files_to_analyze[i:i + chunk_size]

def main():
    if not os.environ.get('OPENAI_API_KEY') and not os.environ.get('GITHUB_TOKEN'):
        print("Warning: API keys missing. Proceeding with basic static analysis only.")

    files_to_analyze = get_modified_files()
    if files_to_analyze is None:
        # Full scan
        files_to_analyze = []
        # Scan mythrax-core
        for root, _, files in os.walk('mythrax-core/src'):
            for f in files:
                if f.endswith('.rs'):
                    files_to_analyze.append(os.path.join(root, f))

        # Scan .agents directory for AI scaffolding
        if os.path.exists('.agents'):
            for root, _, files in os.walk('.agents'):
                for f in files:
                    if f.endswith('.rs') or f.endswith('.py') or f.endswith('.md'):
                        files_to_analyze.append(os.path.join(root, f))

    # Always include core context files
    for core_file in ['ARCHITECTURE.md', 'mock_audit_report.md']:
        if core_file not in files_to_analyze and os.path.exists(core_file):
            files_to_analyze.append(core_file)

    all_findings = []

    # 1. Static Analysis
    for f in files_to_analyze:
        if os.path.exists(f):
            findings = analyze_file(f)
            all_findings.extend(findings)

    # 2. Test Coverage Gaps
    # Only test coverage for the files being analyzed (PR modifications) or all files (scheduled)
    coverage_findings = find_test_coverage_gaps(get_modified_files())
    all_findings.extend(coverage_findings)

    # 3. LLM Analysis
    if os.environ.get('OPENAI_API_KEY'):
        print("Running LLM analysis on files...")
        for file_chunk in chunk_files(files_to_analyze):
            files_content = {}
            for f in file_chunk:
                if os.path.exists(f):
                    try:
                        with open(f, 'r', encoding='utf-8') as file:
                            files_content[f] = file.read()
                    except Exception as e:
                        print(f"Error reading {f} for LLM: {e}")

            if files_content:
                 llm_findings = get_llm_findings(files_content)
                 all_findings.extend(llm_findings)

    for finding in all_findings:
        file_github_issue(finding)

    # Prevent fatal exit if LLM fails, return code 0
    sys.exit(0)

if __name__ == '__main__':
    main()
