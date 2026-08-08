import os
import re
import sys
import json
import shutil
import subprocess
import atexit

def get_tracked_todos(repo_root):
    todo_file = os.path.join(repo_root, "TODO.md")
    if not os.path.exists(todo_file):
        return []
    with open(todo_file, "r", encoding="utf-8") as f:
        content = f.read()
    return content.lower()

def run_clippy(core_dir):
    clippy_toml = os.path.join(core_dir, "clippy.toml")
    try:
        with open(clippy_toml, "w") as f:
            f.write("cognitive-complexity-threshold = 15\n")
    except Exception as e:
        print(f"Could not write clippy.toml: {e}")
        return ""

    cmd = ["cargo", "clippy", "--no-default-features", "--message-format=json"]
    try:
        result = subprocess.run(cmd, cwd=core_dir, capture_output=True, text=True)
        return result.stdout
    except Exception as e:
        print(f"Error running clippy: {e}")
        return ""
    finally:
        if os.path.exists(clippy_toml):
            os.remove(clippy_toml)

def scan_files(repo_root):
    core_dir = os.path.join(repo_root, "mythrax-core", "src")
    scripts_dir = os.path.join(repo_root, "scripts")

    findings = []
    tracked_todos_text = get_tracked_todos(repo_root)

    debt_markers = [r'\bTODO\b', r'\bFIXME\b', r'\bHACK\b', r'\bTEMP\b']
    debt_regex = re.compile('|'.join(debt_markers), re.IGNORECASE)

    suppression_regex = re.compile(r'#\[allow\(dead_code\)\]')

    error_patterns = [
        r'\.unwrap\(\)',
        r'\.expect\(',
        r'\?',
        r'\bmatch\s+'
    ]
    error_regex = re.compile('|'.join(error_patterns))

    magic_number_regex = re.compile(r'(?<![A-Za-z0-9_])\d{3,}(?![A-Za-z0-9_])')
    string_literal_regex = re.compile(r'"[^"]{4,}"')

    struct_enum_regex = re.compile(r'\b(?:struct|enum)\s+([A-Za-z0-9_]+)')
    defined_types = set()
    duplicated_types = []

    total_debt_score = 0
    file_scores = {}

    directories_to_scan = []
    if os.path.exists(core_dir): directories_to_scan.append(core_dir)
    if os.path.exists(scripts_dir): directories_to_scan.append(scripts_dir)

    for scan_dir in directories_to_scan:
        for root, dirs, files in os.walk(scan_dir):
            if any(exclude in root for exclude in ['target', '.git', 'node_modules', '.venv']):
                continue
            for file in files:
                if not (file.endswith('.rs') or file.endswith('.sh') or file.endswith('.py')):
                    continue
                filepath = os.path.join(root, file)
                rel_path = os.path.relpath(filepath, repo_root)
                file_score = 0
                file_errors = set()

                with open(filepath, "r", encoding="utf-8") as f:
                    try:
                        lines = f.readlines()
                    except UnicodeDecodeError:
                        continue

                for i, line in enumerate(lines):
                    line_num = i + 1

                    match = debt_regex.search(line)
                    if match:
                        marker = match.group(0)
                        is_orphaned = True
                        if marker.lower() == 'todo':
                            todo_text = line[match.end():].strip().lower()
                            if todo_text and len(todo_text) > 10 and todo_text[:10] in tracked_todos_text:
                                is_orphaned = False
                        findings.append({"type": "debt_marker", "file": rel_path, "line": line_num, "message": f"{marker} comment", "orphaned": is_orphaned})
                        file_score += (2 if is_orphaned else 1)

                    if suppression_regex.search(line):
                        findings.append({"type": "dead_code_suppression", "file": rel_path, "line": line_num, "message": "#[allow(dead_code)]"})
                        file_score += 3

                    if file.endswith('.rs') and not line.strip().startswith('//'):
                        for m in magic_number_regex.finditer(line):
                            if not "test" in rel_path.lower():
                                findings.append({"type": "magic_number", "file": rel_path, "line": line_num, "message": f"Magic number: {m.group(0)}"})
                                file_score += 0.5

                        for m in string_literal_regex.finditer(line):
                            if not "test" in rel_path.lower():
                                findings.append({"type": "string_literal", "file": rel_path, "line": line_num, "message": f"String literal: {m.group(0)}"})
                                file_score += 0.5

                        for m in error_regex.finditer(line):
                            file_errors.add(m.group(0).strip())

                        type_match = struct_enum_regex.search(line)
                        if type_match:
                            t_name = type_match.group(1)
                            if t_name in defined_types:
                                duplicated_types.append({"type": "duplicated_type", "file": rel_path, "line": line_num, "message": f"Duplicated struct/enum name: {t_name}"})
                                file_score += 4
                            else:
                                defined_types.add(t_name)

                if len(file_errors) > 2:
                    findings.append({"type": "inconsistent_errors", "file": rel_path, "line": 0, "message": f"Inconsistent error handling: {', '.join(file_errors)}"})
                    file_score += 3

                if file_score > 0:
                    file_scores[rel_path] = file_score
                    total_debt_score += file_score

    findings.extend(duplicated_types)
    return {"findings": findings, "total_score": total_debt_score, "file_scores": file_scores}

def analyze_clippy(clippy_json):
    findings = []
    score = 0
    file_scores = {}

    for line in clippy_json.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue

        if msg.get("reason") == "compiler-message" and msg.get("message"):
            diag = msg["message"]
            code = diag.get("code")
            if not code: continue
            code_id = code.get("code", "")

            is_debt = False
            debt_type = ""
            weight = 0

            if "cognitive_complexity" in code_id:
                is_debt = True
                debt_type = "high_complexity"
                weight = 5
            elif "dead_code" in code_id:
                is_debt = True
                debt_type = "dead_code"
                weight = 3
            elif "unused_imports" in code_id or "unused_variables" in code_id:
                is_debt = True
                debt_type = "unused_code"
                weight = 2
            elif "unreachable_code" in code_id or "unreachable_patterns" in code_id:
                is_debt = True
                debt_type = "unreachable_code"
                weight = 3

            if is_debt:
                spans = diag.get("spans", [])
                primary_span = next((s for s in spans if s.get("is_primary")), spans[0] if spans else None)
                if primary_span:
                    file_name = primary_span.get("file_name")
                    line_num = primary_span.get("line_start")
                    findings.append({"type": debt_type, "file": file_name, "line": line_num, "message": diag.get("message")})
                    score += weight
                    file_scores[file_name] = file_scores.get(file_name, 0) + weight

    return {"findings": findings, "total_score": score, "file_scores": file_scores}

def get_commits():
    try:
        res = subprocess.run(["git", "log", "--format=%H", "-n", "6"], capture_output=True, text=True, check=True)
        commits = res.stdout.strip().split("\n")
        return [c for c in commits if c]
    except subprocess.CalledProcessError:
        return []

def scan_commit(commit, repo_root):
    subprocess.run(["git", "checkout", "-f", commit], cwd=repo_root, capture_output=True)

    core_dir = os.path.join(repo_root, "mythrax-core")
    if not os.path.exists(os.path.join(core_dir, "Cargo.toml")):
        return 0, {}

    file_scan_results = scan_files(repo_root)
    clippy_json = run_clippy(core_dir)
    clippy_results = analyze_clippy(clippy_json)

    total_score = file_scan_results["total_score"] + clippy_results["total_score"]

    combined_file_scores = file_scan_results["file_scores"].copy()
    for f, s in clippy_results["file_scores"].items():
        if not f.startswith('mythrax-core/'):
            f = os.path.join('mythrax-core', f)
        combined_file_scores[f] = combined_file_scores.get(f, 0) + s

    return total_score, combined_file_scores

def restore_branch(original_branch, repo_root):
    subprocess.run(["git", "checkout", "-f", original_branch], cwd=repo_root, capture_output=True)

def main():
    repo_root = os.getcwd()
    original_branch = subprocess.run(["git", "rev-parse", "--abbrev-ref", "HEAD"], capture_output=True, text=True).stdout.strip()
    if original_branch == "HEAD":
        original_branch = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()

    atexit.register(restore_branch, original_branch, repo_root)

    commits = get_commits()
    if not commits:
        print("No commits found.")
        sys.exit(1)

    history = []
    for commit in commits:
        score, f_scores = scan_commit(commit, repo_root)
        history.append({"commit": commit[:7], "score": score, "files": f_scores})

    history.reverse()

    if not history:
        print("No history scanned.")
        sys.exit(1)

    current_score = history[-1]["score"]
    current_files = history[-1]["files"]
    prev_files = history[-2]["files"] if len(history) > 1 else {}

    report = ["# Architect Sanitation Scorecard\n"]
    report.append(f"**Current Debt Score:** {current_score}\n")

    report.append("## Trajectory\n")
    report.append("| Commit | Debt Score |\n|---|---|\n")
    for h in history:
        report.append(f"| {h['commit']} | {h['score']} |\n")

    report.append("\n## Flagged Files (Increasing Debt)\n")
    flagged = False
    for f, score in current_files.items():
        prev_score = prev_files.get(f, 0)
        if score > prev_score:
            report.append(f"- **{f}**: {prev_score} -> {score}\n")
            flagged = True
    if not flagged:
        report.append("No files with increasing debt.\n")

    report_text = "".join(report)
    scorecard_path = os.path.join(repo_root, "scorecard.md")

    print(report_text)

    # check if we are in github actions
    if os.environ.get("GITHUB_ACTIONS") == "true" and shutil.which("gh"):
        ref_name = os.environ.get("GITHUB_REF_NAME")

        with open(scorecard_path, "w") as f:
            f.write(report_text)

        try:
            # gh can figure out the PR associated with a branch natively
            # GITHUB_REF_NAME is usually the branch name for push events
            if ref_name:
                subprocess.run(["gh", "pr", "comment", ref_name, "-F", scorecard_path])
        finally:
            if os.path.exists(scorecard_path):
                os.remove(scorecard_path)

if __name__ == "__main__":
    main()
