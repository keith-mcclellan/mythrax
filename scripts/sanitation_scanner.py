#!/usr/bin/env python3
import subprocess
import json
import os
import re

TODO_FILE = "TODO.md"

def get_tracked_todos():
    todos = []
    if os.path.exists(TODO_FILE):
        with open(TODO_FILE, "r", encoding="utf-8") as f:
            for line in f:
                # Basic matching for bullet points or lists in TODO.md
                if "- [" in line or "*" in line or line.strip().startswith("-"):
                    cleaned = re.sub(r'^[-\*]\s*\[?[ xX]?\]?\s*', '', line).strip().lower()
                    if cleaned:
                        todos.append(cleaned)
    return todos

def run_clippy(temp_dir=None):
    manifest_path = "mythrax-core/Cargo.toml"
    if temp_dir:
        manifest_path = os.path.join(temp_dir, "mythrax-core/Cargo.toml")

    cmd = [
        "cargo", "clippy",
        f"--manifest-path={manifest_path}",
        "--message-format=json",
        "--",
        "-W", "clippy::cognitive_complexity",
        "-W", "clippy::unwrap_used",
        "-W", "clippy::expect_used",
        "--force-warn", "dead_code",
        "--force-warn", "unused_imports",
        "--force-warn", "unreachable_code"
    ]

    # We use RUSTFLAGS to inject the clippy.toml configuration dynamically
    env = os.environ.copy()
    # It might take a long time to run clippy on all past commits, we only check the main project dir
    if temp_dir:
        with open(os.path.join(temp_dir, "mythrax-core/clippy.toml"), "w") as f:
            f.write("cognitive-complexity-threshold = 15\n")
    else:
        with open("mythrax-core/clippy.toml", "w") as f:
            f.write("cognitive-complexity-threshold = 15\n")

    result = subprocess.run(cmd, env=env, capture_output=True, text=True)

    if temp_dir:
        os.remove(os.path.join(temp_dir, "mythrax-core/clippy.toml"))
    else:
        os.remove("mythrax-core/clippy.toml")

    return result.stdout

def parse_clippy_output(output):
    file_debt = {}

    def add_debt(filepath, debt_type):
        if not filepath:
            return
        # Normalize relative path
        if "mythrax-core/src" in filepath:
            filepath = filepath[filepath.find("mythrax-core/src"):]
            if filepath not in file_debt:
                file_debt[filepath] = {"dead_code": 0, "complex_funcs": 0, "inconsistent_errors": 0, "orphaned_todos": 0, "dupes": 0, "magic_numbers": 0}
            file_debt[filepath][debt_type] += 1

    for line in output.splitlines():
        try:
            msg = json.loads(line)
        except:
            continue
        if msg.get("reason") == "compiler-message":
            message = msg.get("message", {})
            spans = message.get("spans", [])
            filepath = spans[0].get("file_name") if spans else None

            code = message.get("code", {})
            if code:
                code_id = code.get("code", "")
                if code_id in ["dead_code", "unused_imports", "unreachable_code"]:
                    add_debt(filepath, "dead_code")
                elif code_id in ["clippy::unwrap_used", "clippy::expect_used"]:
                    add_debt(filepath, "inconsistent_errors")
                elif code_id == "clippy::cognitive_complexity":
                    add_debt(filepath, "complex_funcs")
            else:
                rendered = message.get("rendered", "")
                if "clippy::cognitive_complexity" in rendered:
                    add_debt(filepath, "complex_funcs")
                if "clippy::unwrap_used" in rendered or "clippy::expect_used" in rendered:
                    add_debt(filepath, "inconsistent_errors")

    return file_debt

def manual_scan(tracked_todos, file_debt, src_dir="mythrax-core/src"):
    structs_seen = set()

    for root, _, files in os.walk(src_dir):
        for file in files:
            if not file.endswith(".rs"):
                continue
            path = os.path.join(root, file)
            # Normalize path for indexing
            idx_path = path
            if src_dir != "mythrax-core/src":
                idx_path = path.replace(src_dir, "mythrax-core/src")

            # Ensure file is in file_debt
            if idx_path not in file_debt:
                file_debt[idx_path] = {"dead_code": 0, "complex_funcs": 0, "inconsistent_errors": 0, "orphaned_todos": 0, "dupes": 0, "magic_numbers": 0}

            with open(path, "r", encoding="utf-8", errors="ignore") as f:
                lines = f.readlines()
                for line in lines:
                    # TODO checks
                    if any(x in line for x in ["// TODO", "// FIXME", "// HACK", "// TEMP"]):
                        comment_raw = line.split("//", 1)[1].strip()
                        # Extract the actual message after the keyword
                        comment_text = re.sub(r'^(TODO|FIXME|HACK|TEMP)[:\s]*', '', comment_raw, flags=re.IGNORECASE).strip().lower()
                        if len(comment_text) > 5:
                            found = False
                            for tracked in tracked_todos:
                                if comment_text in tracked or tracked in comment_text:
                                    found = True
                                    break
                            if not found:
                                file_debt[idx_path]["orphaned_todos"] += 1

                    # Dupe structs check
                    if line.strip().startswith("pub struct ") or line.strip().startswith("struct "):
                        parts = line.split("struct ")
                        if len(parts) > 1:
                            struct_name = parts[1].split("{")[0].split("(")[0].strip()
                            if struct_name in structs_seen:
                                file_debt[idx_path]["dupes"] += 1
                            else:
                                structs_seen.add(struct_name)
                    if line.strip().startswith("pub enum ") or line.strip().startswith("enum "):
                        parts = line.split("enum ")
                        if len(parts) > 1:
                            enum_name = parts[1].split("{")[0].split("(")[0].strip()
                            if enum_name in structs_seen:
                                file_debt[idx_path]["dupes"] += 1
                            else:
                                structs_seen.add(enum_name)

                    # Magic numbers
                    if "=" in line and not line.strip().startswith("const ") and not line.strip().startswith("let mut") and not "==" in line:
                        parts = line.split("=")
                        if len(parts) == 2:
                            val = parts[1].strip().strip(";").strip()
                            if val.isdigit() and val not in ["0", "1"]:
                                file_debt[idx_path]["magic_numbers"] += 1

    return file_debt

def calculate_debt(tracked_todos, temp_dir=None):
    clippy_out = run_clippy(temp_dir)
    file_debt = parse_clippy_output(clippy_out)
    src_dir = os.path.join(temp_dir, "mythrax-core/src") if temp_dir else "mythrax-core/src"
    file_debt = manual_scan(tracked_todos, file_debt, src_dir=src_dir)

    return file_debt

def analyze_trajectory():
    import tempfile
    import shutil

    original_dir = os.getcwd()
    tracked_todos = get_tracked_todos()

    trajectory = []
    commits_result = subprocess.run(["git", "log", "-n", "6", "--format=%H"], capture_output=True, text=True)
    commits = commits_result.stdout.strip().split("\n")

    # Run clippy on the current directory for HEAD
    current_debt = calculate_debt(tracked_todos)

    with tempfile.TemporaryDirectory() as temp_dir:
        subprocess.run(["git", "clone", ".", temp_dir], capture_output=True)

        # Don't iterate over the current commit again, it's the first in commits
        for commit in commits:
            if not commit: continue

            subprocess.run(["git", "checkout", "-f", commit], cwd=temp_dir, capture_output=True)

            # To speed up clippy, just use a heuristic for past commits instead of running the full clippy check
            # For past commits, we only run manual scan to get a fast trajectory metric, or we do a quick clippy
            fd = calculate_debt(tracked_todos, temp_dir=temp_dir)
            total = sum(sum(metrics.values()) for metrics in fd.values())
            trajectory.append(total)

    return current_debt, trajectory

def main():
    tracked_todos = get_tracked_todos()

    # We will compute trajectory, which is time-consuming but necessary
    current_file_debt, trajectory = analyze_trajectory()

    total_dead_code = sum(d["dead_code"] for d in current_file_debt.values())
    total_complex = sum(d["complex_funcs"] for d in current_file_debt.values())
    total_inconsistent = sum(d["inconsistent_errors"] for d in current_file_debt.values())
    total_orphaned = sum(d["orphaned_todos"] for d in current_file_debt.values())
    total_dupes = sum(d["dupes"] for d in current_file_debt.values())
    total_magic = sum(d["magic_numbers"] for d in current_file_debt.values())

    current_total = total_dead_code + total_complex + total_inconsistent + total_orphaned + total_dupes + total_magic

    # Identify files with high/increasing debt
    # We define increasing debt as a file having > 5 total debt items (heuristically) for the "flagging"
    flagged_files = []
    for filepath, metrics in current_file_debt.items():
        file_total = sum(metrics.values())
        if file_total > 5:
            flagged_files.append((filepath, file_total))

    flagged_files.sort(key=lambda x: x[1], reverse=True)

    with open("scorecard.md", "w") as f:
        f.write("# 🧹 Code Sanitation Scorecard\n\n")

        f.write("## Current Debt Metrics\n")
        f.write(f"- **Dead Code / Unused Imports:** {total_dead_code}\n")
        f.write(f"- **Complex Functions (>15):** {total_complex}\n")
        f.write(f"- **Inconsistent Error Handling (unwrap/expect):** {total_inconsistent}\n")
        f.write(f"- **Orphaned TODOs:** {total_orphaned}\n")
        f.write(f"- **Duplicated Structs/Enums:** {total_dupes}\n")
        f.write(f"- **Magic Numbers (Heuristic):** {total_magic}\n")
        f.write(f"\n**Total Debt Score:** {current_total}\n\n")

        f.write("## Trajectory (Last 5 Commits)\n")
        if len(trajectory) > 1:
            # Trajectory is [current, HEAD-1, HEAD-2...]
            history_str = ", ".join(map(str, trajectory[1:]))
            f.write(f"Previous scores (HEAD~1 to HEAD~5): {history_str}\n\n")
            if current_total > trajectory[1]:
                f.write("🚨 **Warning:** Total debt has **INCREASED** compared to the previous commit!\n")
            elif current_total < trajectory[1]:
                f.write("✅ **Good Job:** Total debt has **DECREASED** compared to the previous commit.\n")
            else:
                f.write("➖ **Neutral:** Total debt is unchanged.\n")
        else:
            f.write("Not enough history for trajectory.\n")

        f.write("\n## 🚩 Files with High Debt Density\n")
        if flagged_files:
            for filepath, score in flagged_files[:10]: # Show top 10
                f.write(f"- `{filepath}`: {score} debt items\n")
        else:
            f.write("No files with high debt density found. Good job!\n")

    print(f"Scorecard generated. Current debt: {current_total}")

if __name__ == "__main__":
    main()
