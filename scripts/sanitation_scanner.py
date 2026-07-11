#!/usr/bin/env python3

import os
import subprocess
import json
import re
from pathlib import Path
from collections import defaultdict

# Paths
REPO_ROOT = Path(__file__).parent.parent
MYTHRAX_CORE = REPO_ROOT / "mythrax-core"
SCRIPTS_DIR = REPO_ROOT / "scripts"
TODO_MD_PATH = REPO_ROOT / "TODO.md"

def run_git_history():
    result = subprocess.run(
        ["git", "log", "-n", "6", "--format=%H %s"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False
    )
    return result.stdout.strip().split("\n")

def run_rust_tooling(commit=None):
    if commit:
        subprocess.run(["git", "checkout", "-f", commit.split()[0]], cwd=REPO_ROOT, check=True, capture_output=True)

    # Ensure clippy.toml exists during history traversal
    clippy_toml = MYTHRAX_CORE / "clippy.toml"
    if not clippy_toml.exists():
        with open(clippy_toml, "w") as f:
            f.write("cognitive-complexity-threshold = 15\n")

    result = subprocess.run(
        ["cargo", "clippy", "--message-format=json", "--", "-W", "clippy::cognitive_complexity"],
        cwd=MYTHRAX_CORE,
        capture_output=True,
        text=True,
        check=False
    )

    findings = {
        "dead_code": 0,
        "unused_imports": 0,
        "unreachable": 0,
        "complex_functions": 0,
        "clippy_warnings": 0,
    }

    for line in result.stdout.splitlines():
        if not line.strip():
            continue
        try:
            msg = json.loads(line)
            if msg.get("reason") == "compiler-message" and msg.get("message"):
                msg_data = msg["message"]
                level = msg_data.get("level")
                if level in ["warning", "error"]:
                    findings["clippy_warnings"] += 1

                code = msg_data.get("code")
                if code:
                    code_id = code.get("code")
                    if code_id == "dead_code":
                        findings["dead_code"] += 1
                    elif code_id == "unused_imports":
                        findings["unused_imports"] += 1
                    elif code_id == "unreachable_code":
                        findings["unreachable"] += 1
                    elif code_id == "clippy::cognitive_complexity":
                        findings["complex_functions"] += 1
        except json.JSONDecodeError:
            pass

    return findings

def scan_manual_debt(commit=None):
    findings = {
        "todo": 0,
        "fixme": 0,
        "hack": 0,
        "temp": 0,
        "allow_dead_code": 0,
        "unwrap": 0,
        "expect": 0,
        "question_mark": 0,
        "match": 0,
        "orphaned_items": 0,
        "magic_numbers": 0,
        "magic_strings": 0,
        "struct_duplication": 0,
    }

    file_debt = defaultdict(int)

    todo_content = ""
    if TODO_MD_PATH.exists():
        with open(TODO_MD_PATH, "r") as f:
            todo_content = f.read().lower()

    struct_defs = defaultdict(list)

    magic_num_re = re.compile(r'\b(?<![a-zA-Z_])(?!0\b|1\b|2\b)\d{2,}(?:\.\d+)?\b')
    magic_str_re = re.compile(r'(?<!print!\()(?<!println!\()(?<!format!\()(?<!panic!\()"(.*?)"')
    struct_re = re.compile(r'(?:pub\s+)?(?:struct|enum)\s+(\w+)[^{]*\{([^}]+)\}')

    files_to_scan = []

    # Collect Rust files
    if MYTHRAX_CORE.exists() and (MYTHRAX_CORE / "src").exists():
        for root, _, files in os.walk(MYTHRAX_CORE / "src"):
            for file in files:
                if file.endswith(".rs"):
                    files_to_scan.append((Path(root) / file, True))

    # Collect script files
    if SCRIPTS_DIR.exists():
        for root, _, files in os.walk(SCRIPTS_DIR):
            for file in files:
                if file.endswith((".py", ".sh", ".bash")):
                    files_to_scan.append((Path(root) / file, False))

    for file_path, is_rust in files_to_scan:
        if not file_path.exists():
            continue

        with open(file_path, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read()

        if is_rust:
            # Parse structs for duplication
            for match in struct_re.finditer(content):
                name, fields = match.groups()
                # normalize fields
                fields_norm = re.sub(r'\s+', '', fields)
                struct_defs[fields_norm].append(name)

        lines = content.splitlines()
        for line in lines:
            line_stripped = line.strip()
            # Support # comments for bash/python
            if not line_stripped or (is_rust and line_stripped.startswith('//')) or (not is_rust and line_stripped.startswith('#')):
                pass

            line_lower = line.lower()

            # Debt comments
            is_debt = False
            if "todo" in line_lower:
                findings["todo"] += 1
                file_debt[str(file_path)] += 1
                is_debt = True
            if "fixme" in line_lower:
                findings["fixme"] += 1
                file_debt[str(file_path)] += 1
                is_debt = True
            if "hack" in line_lower:
                findings["hack"] += 1
                file_debt[str(file_path)] += 1
                is_debt = True
            if "temp" in line_lower:
                findings["temp"] += 1
                file_debt[str(file_path)] += 1
                is_debt = True

            if is_debt:
                comment_text = ""
                comment_prefix = "//" if is_rust else "#"
                if comment_prefix in line:
                    comment_text = line.split(comment_prefix, 1)[1].strip().lower()
                    comment_text = re.sub(r'^(todo|fixme|hack|temp)s?:?\s*', '', comment_text)

                if comment_text and todo_content and comment_text not in todo_content:
                    findings["orphaned_items"] += 1
                    file_debt[str(file_path)] += 1

            if is_rust:
                if "#[allow(dead_code)]" in line:
                    findings["allow_dead_code"] += 1
                    file_debt[str(file_path)] += 5

                if ".unwrap(" in line:
                    findings["unwrap"] += 1
                    file_debt[str(file_path)] += 1
                if ".expect(" in line:
                    findings["expect"] += 1
                    file_debt[str(file_path)] += 1
                if "?" in line:
                    findings["question_mark"] += 1
                if "match " in line:
                    findings["match"] += 1

            # Magic values (crude heuristic)
            if not line_stripped.startswith('const ') and not line_stripped.startswith('static ') and not (not is_rust and re.match(r'^[A-Z_]+=', line_stripped)):
                nums = magic_num_re.findall(line_stripped)
                if nums:
                    findings["magic_numbers"] += len(nums)
                    file_debt[str(file_path)] += len(nums)

                strs = magic_str_re.findall(line_stripped)
                for s in strs:
                    if len(s) > 5 and not s.startswith('{') and not s.endswith('}'):
                        findings["magic_strings"] += 1
                        file_debt[str(file_path)] += 1

    for fields, names in struct_defs.items():
        if len(names) > 1:
            findings["struct_duplication"] += len(names) - 1

    return findings, file_debt

def main():
    print("Gathering current state metrics...")
    current_clippy = run_rust_tooling()
    current_manual, current_file_debt = scan_manual_debt()

    history = run_git_history()
    historical_data = []

    original_branch_res = subprocess.run(["git", "branch", "--show-current"], cwd=REPO_ROOT, capture_output=True, text=True)
    original_branch = original_branch_res.stdout.strip()
    if not original_branch:
        original_branch_res = subprocess.run(["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, capture_output=True, text=True)
        original_branch = original_branch_res.stdout.strip()

    print(f"Original branch: {original_branch}")

    try:
        # Check out previous commits
        for commit in history[1:]:
            if not commit.strip(): continue
            print(f"Checking out {commit}...")
            clippy = run_rust_tooling(commit)
            manual, file_debt = scan_manual_debt(commit)
            historical_data.append({
                "commit": commit,
                "clippy": clippy,
                "manual": manual,
                "file_debt": file_debt
            })
    finally:
        print(f"Restoring {original_branch}...")
        subprocess.run(["git", "checkout", "-f", original_branch], cwd=REPO_ROOT, check=True, capture_output=True)

    # Calculate file debt increases
    increasing_debt_files = []
    if historical_data:
        prev_file_debt = historical_data[0]["file_debt"]
        for f, debt in current_file_debt.items():
            prev_debt = prev_file_debt.get(f, 0)
            if debt > prev_debt:
                increasing_debt_files.append((f, prev_debt, debt))

    report = ["# Technical Debt Sanitation Scorecard"]
    report.append(f"## Current Run Metrics")
    report.append(f"- Clippy Warnings: {current_clippy['clippy_warnings']}")
    report.append(f"- Complex Functions (Clippy): {current_clippy['complex_functions']}")
    report.append(f"- Allow(dead_code) Count: {current_manual['allow_dead_code']}")
    report.append(f"- TODO Comments: {current_manual['todo']}")
    report.append(f"- FIXME Comments: {current_manual['fixme']}")
    report.append(f"- HACK/TEMP Comments: {current_manual['hack'] + current_manual['temp']}")
    report.append(f"- Orphaned Debt Items: {current_manual['orphaned_items']}")
    report.append(f"- Error Handling: {current_manual['unwrap']} unwraps, {current_manual['expect']} expects, {current_manual['question_mark']} ?, {current_manual['match']} matches")
    report.append(f"- Magic Numbers: {current_manual['magic_numbers']}")
    report.append(f"- Magic Strings: {current_manual['magic_strings']}")
    report.append(f"- Struct/Enum Duplications: {current_manual['struct_duplication']}")

    if increasing_debt_files:
        report.append("\n## Files with Increasing Debt")
        for f, old_d, new_d in increasing_debt_files:
            report.append(f"- `{Path(f).name}`: {old_d} -> {new_d}")

    report.append("\n## Trajectory (Last 5 Commits)")
    for data in historical_data:
        c = data["commit"]
        clippy = data["clippy"]
        manual = data["manual"]
        report.append(f"### {c}")
        report.append(f"- Clippy Warnings: {clippy['clippy_warnings']}")
        report.append(f"- Complex Functions: {clippy['complex_functions']}")
        report.append(f"- allow(dead_code): {manual['allow_dead_code']}")
        report.append(f"- TODOs: {manual['todo']}")
        report.append(f"- Orphaned Debt Items: {manual['orphaned_items']}")

    report_content = "\n".join(report)

    # Save scorecard
    scorecard_path = REPO_ROOT / "sanitation_scorecard.md"
    with open(scorecard_path, "w") as f:
        f.write(report_content)

    print(f"\nSaved report to {scorecard_path}")

    # GitHub Action PR Comment Logic
    if os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_EVENT_NAME") in ["push", "pull_request"]:
        print("Mock GitHub Actions: Appending findings as PR comments.")

if __name__ == "__main__":
    main()
