import subprocess
import os
import re
import sys
import json
from pathlib import Path
from dataclasses import dataclass
from collections import defaultdict
import traceback

@dataclass
class DebtMetrics:
    dead_code: int = 0
    todos: int = 0
    complexity: int = 0
    inconsistent_errors: int = 0
    magic_numbers: int = 0
    duplicated_structs: int = 0

    @property
    def total(self) -> int:
        return self.dead_code + self.todos + self.complexity + self.inconsistent_errors + self.magic_numbers + self.duplicated_structs

def get_tracked_todos():
    todos = set()
    try:
        with open('TODO.md', 'r') as f:
            content = f.read()
            matches = re.findall(r'- \[ \]\s*(.*)', content)
            for match in matches:
                todos.add(match.lower())
    except FileNotFoundError:
        pass
    return todos

def analyze_complexity(content: str) -> int:
    complexity_debt = 0
    funcs = re.finditer(r'fn\s+(\w+)(?:<[^>]+>)?\s*\(.*?\)\s*(?:->.*?)?\{', content, re.DOTALL)
    for func in funcs:
        start = func.end()
        braces = 1
        i = start
        while i < len(content) and braces > 0:
            if content[i] == '{':
                braces += 1
            elif content[i] == '}':
                braces -= 1
            i += 1
        body = content[start:i]

        body_clean = re.sub(r'//.*', '', body)
        body_clean = re.sub(r'/\*.*?\*/', '', body_clean, flags=re.DOTALL)
        body_clean = re.sub(r'".*?"', '""', body_clean)

        complexity = 1
        complexity += len(re.findall(r'\bif\b', body_clean))
        complexity += len(re.findall(r'\bwhile\b', body_clean))
        complexity += len(re.findall(r'\bfor\b', body_clean))
        complexity += len(re.findall(r'\bmatch\b', body_clean))
        complexity += body_clean.count('?')
        complexity += body_clean.count('&&')
        complexity += body_clean.count('||')

        if complexity > 15:
            complexity_debt += (complexity - 15)

    return complexity_debt

def run_clippy_for_dead_code():
    # Use clippy to find dead code, unused imports, unreachable branches, and convert to debt
    file_debt = defaultdict(int)
    try:
        # Run cargo clippy --message-format=json from within mythrax-core
        result = subprocess.run(
            ['cargo', 'clippy', '--message-format=json', '--', '-W', 'dead_code', '-W', 'unused_imports', '-W', 'unreachable_code'],
            cwd='mythrax-core',
            capture_output=True,
            text=True
        )

        for line in result.stdout.split('\n'):
            if not line:
                continue
            try:
                msg = json.loads(line)
                if msg.get('reason') == 'compiler-message':
                    code = msg.get('message', {}).get('code', {})
                    if code and code.get('code') in ['dead_code', 'unused_imports', 'unreachable_code']:
                        spans = msg.get('message', {}).get('spans', [])
                        if spans:
                            # Extract path
                            # Note: clippy paths are relative to the crate root (e.g. mythrax-core/src/...) or workspace root?
                            # Usually relative to workspace root which is `mythrax-core` in this case
                            file_name = spans[0].get('file_name', '')
                            if file_name:
                                # Convert to full path relative to repo root
                                full_path = str(Path('mythrax-core') / file_name)
                                file_debt[full_path] += 1
            except json.JSONDecodeError:
                pass
    except Exception as e:
        print(f"Failed to run clippy: {e}")

    return file_debt

def scan_file(path: Path, tracked_todos: set, all_structs: dict) -> DebtMetrics:
    metrics = DebtMetrics()
    try:
        content = path.read_text()
    except Exception:
        return metrics

    content_clean = re.sub(r'//.*', '', content)
    content_clean = re.sub(r'/\*.*?\*/', '', content_clean, flags=re.DOTALL)
    content_clean = re.sub(r'".*?"', '""', content_clean)

    # Note: actual dead code is added later via clippy results, but we also count #[allow(...)] tags as debt
    metrics.dead_code += len(re.findall(r'#\[allow\(dead_code\)\]', content))
    metrics.dead_code += len(re.findall(r'#\[allow\(unused_imports\)\]', content))
    metrics.dead_code += len(re.findall(r'#\[allow\(unreachable_code\)\]', content))

    inline_todos = re.findall(r'//\s*(TODO|FIXME|HACK|TEMP):?\s*(.*)', content, re.IGNORECASE)
    for tag, desc in inline_todos:
        desc_lower = desc.strip().lower()
        is_tracked = False
        for tracked in tracked_todos:
            if desc_lower in tracked or tracked in desc_lower:
                is_tracked = True
                break
        if not is_tracked:
            metrics.todos += 1

    metrics.inconsistent_errors = len(re.findall(r'\.unwrap\(\)|\.expect\(', content_clean))

    lines = content.split('\n')
    for line in lines:
        stripped = line.strip()
        if stripped.startswith('//') or stripped.startswith('pub const') or stripped.startswith('const '):
            continue
        if '"/Users/' in line or '"/tmp/' in line or 'Bearer ' in line:
            metrics.magic_numbers += 1

        if not re.search(r'test|assert', stripped.lower()):
            # Numbers > 1
            numbers = re.findall(r'\b(?:[2-9]|[1-9]\d+)\b', stripped)
            metrics.magic_numbers += len(numbers)

    if path.suffix == '.rs':
        metrics.complexity = analyze_complexity(content)
        structs = re.findall(r'(?:pub\s+)?(?:struct|enum)\s+(\w+)', content_clean)
        for s in structs:
            all_structs[s].append(str(path))

    return metrics

def run_scan():
    dirs_to_scan = [Path('mythrax-core'), Path('scripts')]
    tracked_todos = get_tracked_todos()
    total_metrics = DebtMetrics()
    file_metrics = {}
    all_structs = defaultdict(list)

    for base_dir in dirs_to_scan:
        if not base_dir.exists():
            continue
        for root, _, files in os.walk(base_dir):
            if 'target' in root or '.git' in root:
                continue
            for file in files:
                if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                    path = Path(root) / file
                    metrics = scan_file(path, tracked_todos, all_structs)
                    file_metrics[str(path)] = metrics

    for s, paths in all_structs.items():
        if len(paths) > 1:
            for p in paths:
                file_metrics[p].duplicated_structs += 1

    # Run clippy for actual unannotated dead code debt
    clippy_debt = run_clippy_for_dead_code()
    for file, debt in clippy_debt.items():
        if file not in file_metrics:
            file_metrics[file] = DebtMetrics()
        file_metrics[file].dead_code += debt

    for metrics in file_metrics.values():
        total_metrics.dead_code += metrics.dead_code
        total_metrics.todos += metrics.todos
        total_metrics.complexity += metrics.complexity
        total_metrics.inconsistent_errors += metrics.inconsistent_errors
        total_metrics.magic_numbers += metrics.magic_numbers
        total_metrics.duplicated_structs += metrics.duplicated_structs

    return total_metrics, file_metrics

def get_commit_history(num_commits=5):
    try:
        output = subprocess.check_output(['git', 'log', f'-{num_commits}', '--format=%H'], text=True)
        return output.strip().split('\n')
    except subprocess.CalledProcessError:
        return []

def run_historical_scan(commits):
    history = {}
    file_history = {}

    current_branch = ""
    try:
        current_branch = subprocess.check_output(['git', 'rev-parse', '--abbrev-ref', 'HEAD'], text=True).strip()
    except:
        pass

    current_commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()

    try:
        for commit in reversed(commits):
            print(f"Scanning commit {commit}...")
            # More robust git cleanup
            subprocess.run(['git', 'clean', '-fd'])
            subprocess.run(['git', 'reset', '--hard'])

            subprocess.run(['git', 'checkout', '-f', '-q', commit])
            total, files = run_scan()
            history[commit] = total
            file_history[commit] = files
    except Exception as e:
        print(f"Error during historical scan: {e}")
        traceback.print_exc()
    finally:
        subprocess.run(['git', 'clean', '-fd'])
        subprocess.run(['git', 'reset', '--hard'])
        try:
            if current_branch and current_branch != 'HEAD':
                subprocess.run(['git', 'checkout', '-f', '-q', current_branch])
            else:
                subprocess.run(['git', 'checkout', '-f', '-q', current_commit])
        except:
            subprocess.run(['git', 'checkout', '-f', '-q', current_commit])

    return history, file_history

def generate_report():
    print("Gathering commit history...")
    commits = get_commit_history(6)
    if not commits:
        print("No commit history found.")
        return

    history, file_history = run_historical_scan(commits)

    current_commit = commits[0]
    prev_commit = commits[1] if len(commits) > 1 else None

    if current_commit not in history:
        current_total, current_files = run_scan()
        history[current_commit] = current_total
        file_history[current_commit] = current_files

    current_files = file_history[current_commit]
    prev_files = file_history.get(prev_commit, {})

    report = ["# Chief Architect Sanitation Scorecard\n"]
    report.append("## Debt Trajectory (Last 5 Commits)\n")
    report.append("| Commit | Total Debt | Dead Code | Orphaned TODOs | Inconsistent Errors | Magic Strings/Numbers | Complexity | Duplicates |")
    report.append("|--------|------------|-----------|----------------|---------------------|-----------------------|------------|------------|")

    prev_total_val = None
    for commit in reversed(commits):
        if commit not in history:
            continue
        metrics = history[commit]
        trend = ""
        if prev_total_val is not None:
            if metrics.total < prev_total_val:
                trend = " 📉 (Improving)"
            elif metrics.total > prev_total_val:
                trend = " 📈 (Degrading)"
            else:
                trend = " ➖"
        report.append(f"| `{commit[:7]}` | {metrics.total}{trend} | {metrics.dead_code} | {metrics.todos} | {metrics.inconsistent_errors} | {metrics.magic_numbers} | {metrics.complexity} | {metrics.duplicated_structs} |")
        prev_total_val = metrics.total

    report.append("\n## Current Debt by File (Degrading Files Flagged)\n")
    report.append("| File | Total Debt | Dead Code | Orphaned TODOs | Inconsistent Errors | Magic Strings/Numbers | Complexity | Duplicates |")
    report.append("|------|------------|-----------|----------------|---------------------|-----------------------|------------|------------|")

    sorted_files = sorted(current_files.items(), key=lambda x: x[1].total, reverse=True)
    for file, metrics in sorted_files:
        if metrics.total > 0:
            trend = ""
            if prev_commit and file in prev_files:
                prev_metrics = prev_files[file]
                if metrics.total > prev_metrics.total:
                    trend = " 🚨 DEGRADING"
            elif prev_commit and file not in prev_files:
                trend = " 🚨 DEGRADING (New)"

            report.append(f"| `{file}`{trend} | {metrics.total} | {metrics.dead_code} | {metrics.todos} | {metrics.inconsistent_errors} | {metrics.magic_numbers} | {metrics.complexity} | {metrics.duplicated_structs} |")

    report_text = "\n".join(report)
    print("\n" + report_text)

    event_name = os.environ.get('GITHUB_EVENT_NAME', '')
    if event_name in ['push', 'pull_request']:
        branch_name = os.environ.get('GITHUB_HEAD_REF', '') or os.environ.get('GITHUB_REF_NAME', '')
        if branch_name:
            print(f"Event {event_name} detected on branch {branch_name}, attempting to post to PR...")
            try:
                pr_json = subprocess.check_output(['gh', 'pr', 'list', '--head', branch_name, '--json', 'number'], text=True)
                prs = json.loads(pr_json)
                if prs:
                    pr_number = prs[0]['number']
                    with open('report.md', 'w') as f:
                        f.write(report_text)
                    subprocess.run(['gh', 'pr', 'comment', str(pr_number), '--body-file', 'report.md'], check=True)
                    os.remove('report.md')
                    print("Successfully posted scorecard to PR.")
                else:
                    print(f"No open PR found for branch {branch_name}.")
            except Exception as e:
                print(f"Failed to post to PR: {e}")

if __name__ == '__main__':
    generate_report()
