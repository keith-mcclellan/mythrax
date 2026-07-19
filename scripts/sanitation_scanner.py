import subprocess
import json
import re
import sys
import os
import shutil
from collections import defaultdict

def run_clippy(cwd):
    cmd = ["cargo", "clippy", "--message-format=json", "--", "-D", "warnings"]
    try:
        result = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, check=False)
        return result.stdout.splitlines()
    except Exception as e:
        print(f"Error running clippy: {e}")
        return []

def scan_files(base_dir):
    files = []
    for root, dirs, filenames in os.walk(os.path.join(base_dir, "mythrax-core")):
        if "target" in dirs:
            dirs.remove("target")

        for filename in filenames:
            if filename.endswith(".rs"):
                files.append(os.path.join(root, filename))

    for root, _, filenames in os.walk(os.path.join(base_dir, "scripts")):
        for filename in filenames:
            if filename.endswith(".sh") or filename.endswith(".py"):
                files.append(os.path.join(root, filename))
    return files

def get_todos_from_md(base_dir):
    todo_list = set()
    try:
        with open(os.path.join(base_dir, "TODO.md"), 'r') as f:
            for line in f:
                if line.strip().startswith("- [ ]"):
                    todo_list.add(line.strip().lower())
    except:
        pass
    return todo_list

def find_comments_and_patterns(filepath, tracked_todos, is_rust=True):
    # Regex for TODO, FIXME, HACK, TEMP in both Rust (//) and scripts (#)
    if is_rust:
        todo_re = re.compile(r'//\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', re.IGNORECASE)
    else:
        todo_re = re.compile(r'#\s*(TODO|FIXME|HACK|TEMP)[:\s]*(.*)', re.IGNORECASE)

    dead_code_re = re.compile(r'#\[allow\s*\(\s*dead_code\s*\)\s*\]')
    unwrap_re = re.compile(r'\.unwrap\(\)')
    expect_re = re.compile(r'\.expect\(')
    question_re = re.compile(r'\?')
    match_re = re.compile(r'\bmatch\b')
    struct_re = re.compile(r'\bstruct\s+([A-Z][a-zA-Z0-9_]*)\b')
    enum_re = re.compile(r'\benum\s+([A-Z][a-zA-Z0-9_]*)\b')
    magic_num_re = re.compile(r'\b([2-9]\d{2,})\b')
    string_literal_re = re.compile(r'\"([^\"]{5,})\"')

    findings = []
    error_patterns = {'unwrap': False, 'expect': False, '?': False, 'match': False}
    structs = set()
    enums = set()

    try:
        with open(filepath, 'r') as f:
            for i, line in enumerate(f):
                todo_match = todo_re.search(line)
                if todo_match:
                    tag = todo_match.group(1).upper()
                    comment_text = todo_match.group(2).strip().lower()

                    is_orphaned = True
                    # Check if the words in the comment text appear in any tracked todo
                    if comment_text:
                        comment_words = set(re.findall(r'\w+', comment_text))
                        for t in tracked_todos:
                            if comment_words and comment_words.issubset(set(re.findall(r'\w+', t))):
                                is_orphaned = False
                                break

                    if is_orphaned:
                        findings.append((i+1, f"Orphaned {tag} comment found"))
                    else:
                        findings.append((i+1, f"Tracked {tag} comment found"))

                if is_rust:
                    if dead_code_re.search(line):
                        findings.append((i+1, "Dead code suppression #[allow(dead_code)] found"))

                    if unwrap_re.search(line): error_patterns['unwrap'] = True
                    if expect_re.search(line): error_patterns['expect'] = True
                    if question_re.search(line): error_patterns['?'] = True
                    if match_re.search(line): error_patterns['match'] = True

                    s_match = struct_re.search(line)
                    if s_match: structs.add(s_match.group(1))

                    e_match = enum_re.search(line)
                    if e_match: enums.add(e_match.group(1))

                    stripped_line = line.strip()
                    if not stripped_line.startswith("//"):
                        if magic_num_re.search(line) and not "0x" in line:
                            findings.append((i+1, f"Potential magic number found: {magic_num_re.search(line).group(1)}"))

                        str_match = string_literal_re.search(line)
                        if str_match and not "assert" in line and not "test" in line and not "#[" in line:
                            val = str_match.group(1)
                            if len(val) > 10 and not val.startswith("http") and not val.endswith(".rs"):
                                findings.append((i+1, f"Potential magic string literal found: '{val}'"))

            if is_rust and (error_patterns['unwrap'] or error_patterns['expect']) and (error_patterns['?'] or error_patterns['match']):
                 findings.append((0, "Inconsistent error handling patterns (mixing unwrap/expect with ?/match)"))

    except Exception as e:
        pass
    return findings, structs, enums


def run_full_scan(base_dir):
    clippy_output = run_clippy(os.path.join(base_dir, "mythrax-core"))
    debt = defaultdict(list)

    for line in clippy_output:
        if not line.strip(): continue
        try:
            msg = json.loads(line)
            if msg.get("reason") == "compiler-message":
                msg_data = msg.get("message", {})
                code = msg_data.get("code", {})
                if code:
                    code_id = code.get("code")
                    if code_id in ["dead_code", "unused_imports", "unreachable_code", "clippy::cognitive_complexity", "clippy::too_many_lines"]:
                        spans = msg_data.get("spans", [])
                        if spans:
                            span = spans[0]
                            file_name = span.get("file_name")
                            line_num = span.get("line_start")
                            message_text = msg_data.get("message")

                            full_path = os.path.join("mythrax-core", file_name)
                            debt[full_path].append(f"Line {line_num}: {message_text}")
        except json.JSONDecodeError:
            pass

    files = scan_files(base_dir)
    tracked_todos = get_todos_from_md(base_dir)

    all_structs = defaultdict(list)
    all_enums = defaultdict(list)

    for filepath in files:
        is_rust = filepath.endswith(".rs")
        comments, structs, enums = find_comments_and_patterns(filepath, tracked_todos, is_rust=is_rust)

        # Make paths relative to base_dir for consistent output
        rel_filepath = os.path.relpath(filepath, base_dir)

        for line_num, desc in comments:
             if line_num == 0:
                 debt[rel_filepath].append(f"File level: {desc}")
             else:
                 debt[rel_filepath].append(f"Line {line_num}: {desc}")

        for s in structs:
            all_structs[s].append(rel_filepath)
        for e in enums:
            all_enums[e].append(rel_filepath)

    for s, filepaths in all_structs.items():
        if len(filepaths) > 1:
            for filepath in filepaths:
                debt[filepath].append(f"File level: Struct '{s}' is duplicated across files: {', '.join(filepaths)}")

    for e, filepaths in all_enums.items():
        if len(filepaths) > 1:
            for filepath in filepaths:
                debt[filepath].append(f"File level: Enum '{e}' is duplicated across files: {', '.join(filepaths)}")

    total_debt = sum(len(issues) for issues in debt.values())
    return debt, total_debt

def main():
    current_dir = os.getcwd()

    # Run scan on current HEAD
    current_debt, current_total = run_full_scan(current_dir)

    # Check historical debt trajectory across the last 5 commits
    trajectory = "Unknown (Could not calculate trajectory)"
    try:
        # Create a temporary directory to clone the repo at HEAD~5
        temp_dir = os.path.join(current_dir, ".temp_scanner_repo")
        if os.path.exists(temp_dir):
            shutil.rmtree(temp_dir)

        subprocess.run(["git", "clone", "--no-checkout", current_dir, temp_dir], capture_output=True)
        subprocess.run(["git", "-C", temp_dir, "checkout", "HEAD~5"], capture_output=True)

        _, prev_total = run_full_scan(temp_dir)

        if current_total < prev_total:
            trajectory = f"Improving 📉 (Total debt reduced from {prev_total} to {current_total})"
        elif current_total > prev_total:
            trajectory = f"Degrading 📈 (Total debt increased from {prev_total} to {current_total})"
        else:
            trajectory = f"Stable ➖ (Total debt remained {current_total})"

        shutil.rmtree(temp_dir)
    except Exception as e:
        print(f"Failed to calculate trajectory: {e}")
        pass

    scorecard = "## 🧹 Sanitation Scorecard\n\n"
    scorecard += f"**Total Debt Items:** {current_total}\n\n"

    scorecard += "### Trajectory\n"
    scorecard += f"Compared to last 5 commits: {trajectory}\n\n"

    scorecard += "### Files with Debt\n"
    for file, issues in sorted(current_debt.items(), key=lambda item: len(item[1]), reverse=True):
        scorecard += f"#### `{file}` ({len(issues)} items)\n"
        for issue in issues:
            scorecard += f"- {issue}\n"

    print(scorecard)

    with open("sanitation_scorecard.md", "w") as f:
        f.write(scorecard)

if __name__ == "__main__":
    main()
