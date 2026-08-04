import os
import re
import subprocess
import collections
import shutil
import json

# Paths to search
SCAN_DIRS = ["mythrax-core/src", "scripts"]

def run_cmd(cmd, cwd=None):
    res = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=cwd)
    return res.stdout.strip()

def get_commits():
    commits = run_cmd("git log -n 6 --format='%H %s'").split('\n')
    return [c.split()[0] for c in commits if c and len(c.split()) > 0]

def get_todo_text():
    if os.path.exists("TODO.md"):
        with open("TODO.md", "r") as f:
            return f.read().lower()
    return ""

def calculate_complexity(content):
    functions = re.split(r'fn\s+\w+(?:<[^>]+>)?\s*\(', content)
    high_complexity_count = 0
    for func in functions[1:]:
        complexity = 1
        complexity += len(re.findall(r'\bif\b', func))
        complexity += len(re.findall(r'\bwhile\b', func))
        complexity += len(re.findall(r'\bfor\b', func))
        complexity += len(re.findall(r'\bmatch\b', func))
        complexity += len(re.findall(r'\?', func))
        if complexity > 15:
            high_complexity_count += 1
    return high_complexity_count

def scan_codebase_at_commit(commit_hash, todo_text):
    run_cmd(f"git checkout -f {commit_hash}")

    total_debt = 0
    file_debts = {}
    struct_names = collections.defaultdict(list)

    for d in SCAN_DIRS:
        if not os.path.exists(d):
            continue
        for root, dirs, files in os.walk(d):
            if any(ex in root for ex in ['target', '.git', '.venv', 'node_modules', 'issues']):
                continue
            for file in files:
                if not (file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh')):
                    continue

                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r', encoding='utf-8') as f:
                        content = f.read()
                except:
                    continue

                debt = 0
                issues = []

                # 1. Dead code, unused imports, unreachable branches (including module-level inner attributes)
                dead_code_matches = len(re.findall(r'#!?\[allow\(dead_code\)\]', content))
                if dead_code_matches:
                    debt += dead_code_matches
                    issues.append(f"{dead_code_matches} dead_code suppressions")

                unused_imports_matches = len(re.findall(r'#!?\[allow\(unused_imports\)\]', content))
                if unused_imports_matches:
                    debt += unused_imports_matches
                    issues.append(f"{unused_imports_matches} unused_imports suppressions")

                unreachable_matches = len(re.findall(r'#!?\[allow\(unreachable_code\)\]', content))
                if unreachable_matches:
                    debt += unreachable_matches
                    issues.append(f"{unreachable_matches} unreachable_code suppressions")

                # 2. TODO, FIXME, HACK, TEMP comments not in TODO.md
                orphaned_comments = 0
                for match in re.finditer(r'(?i)\b(TODO|FIXME|HACK|TEMP)[:\s]+(.*)', content):
                    comment = match.group(2).strip().lower()
                    if not comment: continue
                    words = comment.split()
                    if len(words) >= 3:
                        phrase = " ".join(words[:4])
                        if phrase not in todo_text:
                            orphaned_comments += 1
                    else:
                        if comment not in todo_text:
                            orphaned_comments += 1

                if orphaned_comments > 0:
                    debt += orphaned_comments
                    issues.append(f"{orphaned_comments} orphaned TODO/FIXME/HACK/TEMP")

                # 3. Cyclomatic complexity > 15
                complex_funcs = calculate_complexity(content)
                if complex_funcs > 0:
                    debt += complex_funcs * 2
                    issues.append(f"{complex_funcs} functions with complexity > 15")

                # 4. Inconsistent error handling: mixing unwrap(), expect(), ?, match
                unwraps = len(re.findall(r'\.unwrap\(\)', content))
                expects = len(re.findall(r'\.expect\(', content))
                questions = len(re.findall(r'\?', content))
                matches = len(re.findall(r'\bmatch\b', content))

                # Flag mixing unwrap/expect with safe handling (? or match)
                if (unwraps > 0 or expects > 0) and (questions > 0 or matches > 0):
                    debt += (unwraps + expects)
                    issues.append(f"Inconsistent error handling: mixing unwrap/expect ({unwraps+expects}) with safe handling (?, match)")
                elif unwraps > 0:
                    debt += unwraps
                    issues.append(f"{unwraps} unwraps (missing expect/proper handling)")
                elif expects > 0:
                    debt += expects
                    issues.append(f"{expects} expects")

                # 5. Duplicated struct/enum
                structs = re.findall(r'struct\s+([A-Z]\w+)', content)
                enums = re.findall(r'enum\s+([A-Z]\w+)', content)
                for s in structs + enums:
                    struct_names[s].append(filepath)

                # 6. Magic numbers and string literals
                # Numbers not 0,1,2,3 or powers of 2 (common)
                magic_numbers = len(re.findall(r'(?<![a-zA-Z0-9_])([4-9]|[1-9][0-9]+)(?![a-zA-Z0-9_])', content))
                if magic_numbers > 5:
                    debt += (magic_numbers - 5)
                    issues.append(f"{magic_numbers} potential magic numbers (consider named constants)")

                # String literals that are repeated and might need to be constants
                # A basic heuristic: find quoted strings of length > 3 that aren't empty, not in a print/log/panic macro
                # and appear multiple times or just blindly count string literals and penalize if too many
                # We'll penalize if a file has an excessive number of standalone string literals.
                # Remove macros first to reduce noise:
                content_no_macros = re.sub(r'\w+!\([^)]*\)', '', content)
                string_literals = len(re.findall(r'"([^"\\]|\\.)+"', content_no_macros))
                if string_literals > 10:
                    debt += (string_literals - 10)
                    issues.append(f"{string_literals} string literals (consider named constants)")

                if debt > 0:
                    file_debts[filepath] = {
                        "debt": debt,
                        "issues": issues
                    }
                    total_debt += debt

    # Check for duplicated structs across all files
    duplicates_debt = 0
    duplicate_issues = []
    for name, locations in struct_names.items():
        if len(set(locations)) > 1:
            duplicates_debt += 5
            duplicate_issues.append(f"Duplicated type '{name}' in: {', '.join(set(locations))}")

    if duplicates_debt > 0:
        total_debt += duplicates_debt
        file_debts["global_architecture"] = {
            "debt": duplicates_debt,
            "issues": duplicate_issues
        }

    return total_debt, file_debts

def generate_report():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        return

    todo_text = get_todo_text()

    results = {}
    original_commit = commits[0]

    print(f"Analyzing {len(commits)} commits...")
    for c in reversed(commits):
        print(f"Scanning {c}...")
        total_debt, file_debts = scan_codebase_at_commit(c, todo_text)
        results[c] = {
            "total_debt": total_debt,
            "file_debts": file_debts
        }

    # restore to original commit
    run_cmd(f"git checkout -f {original_commit}")

    # Generate Markdown Scorecard
    md = ["# 🏗️ Architecture Sanitation Scorecard\n"]
    md.append("## Technical Debt Trajectory\n")

    md.append("| Commit | Total Debt Score | Trend |")
    md.append("|--------|------------------|-------|")

    prev_score = None
    for c in reversed(commits):
        score = results[c]["total_debt"]
        trend = "➖"
        if prev_score is not None:
            if score > prev_score:
                trend = "📈 Degrading"
            elif score < prev_score:
                trend = "📉 Improving"
        md.append(f"| `{c[:7]}` | {score} | {trend} |")
        prev_score = score

    md.append("\n## Current Debt Hotspots (Latest Commit)\n")
    latest_debts = results[commits[0]]["file_debts"]

    sorted_files = sorted(latest_debts.items(), key=lambda x: x[1]["debt"], reverse=True)

    increasing_files = []
    if len(commits) > 1:
        prev_debts = results[commits[1]]["file_debts"]
        for filepath, data in latest_debts.items():
            if filepath in prev_debts:
                if data["debt"] > prev_debts[filepath]["debt"]:
                    increasing_files.append(filepath)
            else:
                increasing_files.append(filepath)

    if increasing_files:
        md.append("### ⚠️ WARNING: Debt Increasing in the Following Files\n")
        for f in increasing_files:
            md.append(f"- `{f}`")
        md.append("\n")

    for filepath, data in sorted_files[:15]:
        md.append(f"### `{filepath}` (Debt Score: {data['debt']})")
        for issue in data["issues"]:
            md.append(f"- {issue}")

    report_text = "\n".join(md)
    print("\n--- REPORT PREVIEW ---\n")
    print(report_text)

    # We do NOT save it to a file permanently in git, just standard out for now.
    # We'll save it to a temp file to pass to gh if available.
    temp_md = "temp_sanitation_scorecard.md"
    with open(temp_md, "w") as f:
        f.write(report_text)

    # Attempt to post as PR comment if gh is available
    gh_path = shutil.which("gh")
    if gh_path:
        print("\nAttempting to post PR comment...")

        pr_number = None

        # Check standard view first (works in direct PRs)
        try:
            res = run_cmd("gh pr view --json number")
            if res:
                data = json.loads(res)
                if "number" in data:
                    pr_number = data["number"]
        except Exception:
            pass

        # If running on push event, find PR by commit sha
        if not pr_number:
            sha = os.environ.get("GITHUB_SHA", original_commit)
            try:
                res = run_cmd(f"gh pr list --search {sha} --state open --json number")
                if res:
                    data = json.loads(res)
                    if data and isinstance(data, list) and len(data) > 0 and "number" in data[0]:
                        pr_number = data[0]["number"]
            except Exception:
                pass

        # Fallback env var if available
        if not pr_number and os.environ.get("GITHUB_EVENT_PULL_REQUEST_NUMBER"):
             pr_number = os.environ.get("GITHUB_EVENT_PULL_REQUEST_NUMBER")

        if pr_number:
            print(f"Posting to PR #{pr_number}...")
            run_cmd(f"gh pr comment {pr_number} -F {temp_md}")
        else:
            print("No active open PR found for this commit. Skipping comment.")
    else:
        print("\n'gh' CLI not found. Skipping PR comment posting.")

    # Clean up temp file
    if os.path.exists(temp_md):
        os.remove(temp_md)

if __name__ == "__main__":
    generate_report()
