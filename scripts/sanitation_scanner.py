import os
import glob
import subprocess
import re
from collections import defaultdict

def run_cmd(cmd):
    try:
        return subprocess.check_output(cmd, shell=True, text=True, stderr=subprocess.DEVNULL)
    except subprocess.CalledProcessError:
        return ""

def get_last_commits(n=6):
    res = run_cmd(f"git rev-list HEAD -n {n}")
    return res.strip().split("\n") if res else []

def get_file_content_at_commit(commit, filepath):
    if not commit:
        try:
            with open(filepath, "r", encoding="utf-8") as f:
                return f.read()
        except Exception:
            return ""
    res = run_cmd(f"git show {commit}:{filepath}")
    return res

def scan_tree(commit=None):
    if commit:
        res = run_cmd(f"git ls-tree -r --name-only {commit}")
        all_files = res.strip().split("\n") if res else []
        files = [f for f in all_files if (f.startswith("mythrax-core/") and f.endswith(".rs")) or (f.startswith("scripts/") and os.path.isfile(f))]
        return files
    else:
        files = []
        files.extend(glob.glob("mythrax-core/**/*.rs", recursive=True))
        for f in glob.glob("scripts/*", recursive=True):
            if os.path.isfile(f):
                files.append(f)
        return files

def analyze_commit(commit=None):
    files = scan_tree(commit)
    metrics = {
        "debt_items": 0,
        "files_scanned": len(files),
        "debt_density_per_file": defaultdict(int),
        "details": defaultdict(list)
    }

    todo_md = get_file_content_at_commit(commit, "TODO.md").lower()

    # Avoid matching inside words like temporary or template, and ensure it's a standalone marker
    todo_pattern = re.compile(r'//\s*(TODO|FIXME|HACK|TEMP)\b([:\s]+.*)?', re.IGNORECASE)
    branching_keywords = ['if ', 'else if ', 'match ', 'for ', 'while ', 'loop ', 'and_then', 'or_else']

    struct_enum_names = defaultdict(list)
    magic_number_pattern = re.compile(r'(?<![a-zA-Z0-9_])([2-9]|\d{2,})(?![a-zA-Z0-9_])')

    for filepath in files:
        content = get_file_content_at_commit(commit, filepath)
        if not content:
            continue

        lines = content.split('\n')

        # Complexity tracking
        in_function = False
        func_complexity = 0
        func_name = ""
        func_start_line = 0

        # Error handling tracking per file
        error_handling_methods = set()

        in_test_module = False

        for i, line in enumerate(lines):
            line_stripped = line.strip()

            # Function complexity estimation
            if " fn " in line_stripped or line_stripped.startswith("fn "):
                if in_function and func_complexity > 15:
                    metrics["debt_items"] += 1
                    metrics["debt_density_per_file"][filepath] += 1
                    metrics["details"][filepath].append(f"Line {func_start_line}: Function '{func_name}' has excessive cyclomatic complexity ({func_complexity} > 15)")

                in_function = True
                func_complexity = 1 # Base complexity
                func_start_line = i + 1
                try:
                    func_name = line_stripped.split("(")[0].split(" fn ")[-1] if " fn " in line_stripped else line_stripped.split("(")[0].split("fn ")[1]
                    func_name = func_name.strip()
                except:
                    func_name = "unknown"

            if in_function:
                for kw in branching_keywords:
                    if kw in line_stripped and not line_stripped.startswith("//"):
                        func_complexity += 1
                if '?' in line_stripped and not line_stripped.startswith("//"):
                     func_complexity += 1

            # Error handling pattern tracking
            if line_stripped == "#[cfg(test)]" or "mod tests" in line_stripped:
                in_test_module = True

            if not line_stripped.startswith("//"):
                if ".unwrap(" in line_stripped:
                    error_handling_methods.add("unwrap")
                if ".expect(" in line_stripped:
                    error_handling_methods.add("expect")
                if "?" in line_stripped:
                    error_handling_methods.add("?")
                if "match " in line_stripped and ("Err(" in content or "Ok(" in content):
                    error_handling_methods.add("match")

            # Struct/Enum duplication detection
            if " struct " in line_stripped or line_stripped.startswith("struct ") or " enum " in line_stripped or line_stripped.startswith("enum "):
                # Extract the actual name by looking after 'struct' or 'enum'
                parts = re.split(r'\b(struct|enum)\b', line_stripped)
                if len(parts) >= 3:
                    name_part = parts[2].strip()
                    name = name_part.split("<")[0].split("{")[0].split("(")[0].strip()
                    if name:
                        struct_enum_names[name].append((filepath, i+1))

            # Magic numbers
            if not in_test_module and not line_stripped.startswith("//"):
                if "const " not in line_stripped and "static " not in line_stripped and "enum " not in line_stripped and "struct " not in line_stripped:
                    # Ignore common lines like loops, indexing, etc where numbers are common, just doing basic heuristic
                    if magic_number_pattern.search(line_stripped) and '"' not in line_stripped: # ignore strings for now
                         # very noisy, we'll only flag if it's very obvious, e.g. let x = 42;
                         if "let " in line_stripped and "=" in line_stripped:
                            metrics["debt_items"] += 1
                            metrics["debt_density_per_file"][filepath] += 1
                            metrics["details"][filepath].append(f"Line {i+1}: Potential magic number in assignment")

            # Dead code suppression
            if "#[allow(dead_code)]" in line_stripped:
                metrics["debt_items"] += 1
                metrics["debt_density_per_file"][filepath] += 1
                metrics["details"][filepath].append(f"Line {i+1}: dead_code suppression")

            # Orphaned TODO/FIXME/HACK/TEMP
            match = todo_pattern.search(line_stripped)
            if match:
                comment_text = (match.group(2) or "").strip().lower()
                # If comment text is empty or not found in TODO.md, it's orphaned
                if not comment_text or (len(comment_text) > 5 and comment_text not in todo_md):
                    metrics["debt_items"] += 1
                    metrics["debt_density_per_file"][filepath] += 1
                    metrics["details"][filepath].append(f"Line {i+1}: Orphaned {match.group(1)} - {comment_text[:30]}")

        # Check last function in file
        if in_function and func_complexity > 15:
            metrics["debt_items"] += 1
            metrics["debt_density_per_file"][filepath] += 1
            metrics["details"][filepath].append(f"Line {func_start_line}: Function '{func_name}' has excessive cyclomatic complexity ({func_complexity} > 15)")

        # Inconsistent error handling check
        if len(error_handling_methods) > 2:
            metrics["debt_items"] += 1
            metrics["debt_density_per_file"][filepath] += 1
            metrics["details"][filepath].append(f"Inconsistent error handling: mixed use of {', '.join(error_handling_methods)}")

    # Check for duplicated struct/enum
    for name, locations in struct_enum_names.items():
        if len(locations) > 1:
            for filepath, line_num in locations:
                metrics["debt_items"] += 1
                metrics["debt_density_per_file"][filepath] += 1
                metrics["details"][filepath].append(f"Line {line_num}: Struct/Enum '{name}' is duplicated in {len(locations)} places")

    return metrics

def generate_scorecard(metrics_current, metrics_history):
    lines = []
    lines.append("# Sanitation Scorecard")
    lines.append("")

    current_debt = metrics_current["debt_items"]

    lines.append("## Current Debt Metrics")
    lines.append(f"- **Total Debt Items:** {current_debt}")
    lines.append(f"- **Files Scanned:** {metrics_current['files_scanned']}")
    if metrics_current['files_scanned'] > 0:
        lines.append(f"- **Avg Debt/File:** {current_debt / metrics_current['files_scanned']:.2f}")
    lines.append("")

    lines.append("## Trajectory (Last 5 Commits)")
    if metrics_history:
        history_str = " -> ".join([str(m["debt_items"]) for m in reversed(metrics_history)])
        lines.append(f"`{history_str} -> {current_debt} (current)`")

        last_debt = metrics_history[0]["debt_items"]
        if current_debt > last_debt:
            lines.append("\n**⚠️ DEGRADING:** Debt has increased compared to the last commit.")
        elif current_debt < last_debt:
            lines.append("\n**✅ IMPROVING:** Debt has decreased compared to the last commit.")
        else:
            lines.append("\n**➖ STABLE:** Debt remains unchanged.")
    else:
        lines.append("Not enough history.")
    lines.append("")

    lines.append("## Increasing Debt Density")
    increasing_files = []
    if metrics_history:
        last_metrics = metrics_history[0]
        for filepath, density in metrics_current["debt_density_per_file"].items():
            last_density = last_metrics["debt_density_per_file"].get(filepath, 0)
            if density > last_density:
                increasing_files.append((filepath, density, last_density))

    if increasing_files:
        lines.append("The following files have increasing debt density:")
        for filepath, current, previous in sorted(increasing_files, key=lambda x: x[1], reverse=True):
            lines.append(f"- `{filepath}`: {previous} -> **{current}**")
    else:
        lines.append("No files show increasing debt density. Great job!")
    lines.append("")

    lines.append("## Detailed Findings (Current Commit)")
    has_details = False
    for filepath, file_details in sorted(metrics_current["details"].items()):
        if file_details:
            has_details = True
            lines.append(f"### {filepath}")
            for detail in file_details:
                lines.append(f"- {detail}")
            lines.append("")
    if not has_details:
         lines.append("No specific debt items found.")

    return "\n".join(lines)

def main():
    commits = get_last_commits(6)

    metrics_history = []

    # The first commit in the list is HEAD (current)
    # The rest are history
    if not commits:
        print("No commits found.")
        return

    current_commit = commits[0]
    history_commits = commits[1:]

    metrics_current = analyze_commit(None) # None means current working directory / tree

    for c in history_commits:
        metrics_history.append(analyze_commit(c))

    scorecard = generate_scorecard(metrics_current, metrics_history)
    print(scorecard)

if __name__ == "__main__":
    main()
