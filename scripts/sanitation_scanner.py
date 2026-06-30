#!/usr/bin/env python3
import os
import re
import subprocess

def get_commits():
    res = subprocess.run(["git", "log", "--oneline", "-n", "6"], capture_output=True, text=True)
    return [line.split()[0] for line in res.stdout.strip().split('\n')]

def read_todo_md(commit):
    res = subprocess.run(["git", "show", f"{commit}:TODO.md"], capture_output=True, text=True, errors="ignore")
    return res.stdout.lower()

def get_files(commit):
    res = subprocess.run(["git", "ls-tree", "-r", commit, "--name-only"], capture_output=True, text=True)
    files = res.stdout.strip().split('\n')
    return [f for f in files if (f.startswith("mythrax-core/src/") and f.endswith(".rs")) or (f.startswith("scripts/") and f.endswith(".py")) or (f.startswith("scripts/") and f.endswith(".sh"))]

def get_file_content(commit, fpath):
    res = subprocess.run(["git", "show", f"{commit}:{fpath}"], capture_output=True, text=True, errors="ignore")
    return res.stdout

def strip_comments_and_strings(code):
    # Replace block comments
    code = re.sub(r'/\*.*?\*/', '', code, flags=re.DOTALL)
    # Replace line comments
    code = re.sub(r'//.*', '', code)
    # Replace strings
    code = re.sub(r'"(?:\\.|[^"\\])*"', '""', code)
    code = re.sub(r"r#*\"(?:.*?)\"#*", '""', code, flags=re.DOTALL)
    return code

def calculate_complexity(code):
    complexity = 1
    complexity += len(re.findall(r'\bif\b', code))
    complexity += len(re.findall(r'\bwhile\b', code))
    complexity += len(re.findall(r'\bfor\b', code))
    complexity += len(re.findall(r'\bmatch\b', code))
    complexity += len(re.findall(r'\?', code))
    complexity += len(re.findall(r'&&', code))
    complexity += len(re.findall(r'\|\|', code))
    return complexity

def extract_functions(content):
    functions = []
    lines = content.split('\n')
    in_function = False
    brace_depth = 0
    current_func = []

    for line in lines:
        if not in_function:
            if re.search(r'\bfn\s+[a-zA-Z0-9_]+', line):
                in_function = True
                brace_depth = line.count('{') - line.count('}')
                current_func.append(line)
                if brace_depth <= 0 and '{' in line:
                    functions.append('\n'.join(current_func))
                    in_function = False
                    current_func = []
        else:
            current_func.append(line)
            brace_depth += line.count('{') - line.count('}')
            if brace_depth <= 0:
                functions.append('\n'.join(current_func))
                in_function = False
                current_func = []
    return functions

def analyze_commit(commit):
    todo_content = read_todo_md(commit)
    files = get_files(commit)

    total_metrics = {
        "dead_code": 0,
        "unused_imports": 0,
        "unreachable_branches": 0,
        "orphaned_todos": 0,
        "complex_functions": 0,
        "unwraps": 0,
        "expects": 0,
        "qm_operator": 0, # ?
        "matches": 0,
        "magic_numbers": 0,
        "magic_strings": 0,
        "duplicated_structs": 0,
        "duplicated_enums": 0,
    }

    file_metrics = {}
    struct_map = {}
    enum_map = {}

    for fpath in files:
        content = get_file_content(commit, fpath)
        stripped_content = strip_comments_and_strings(content)

        f_dead_code = len(re.findall(r'#\[allow\(dead_code\)\]', stripped_content))
        f_unused_imports = len(re.findall(r'#\[allow\(unused_imports\)\]', stripped_content))
        f_unreachable = len(re.findall(r'#\[allow\(unreachable_(code|patterns)\)\]', stripped_content))

        f_unwraps = len(re.findall(r'\.unwrap\(\)', stripped_content))
        f_expects = len(re.findall(r'\.expect\(', stripped_content))
        f_qm = len(re.findall(r'\?', stripped_content))
        f_matches = len(re.findall(r'\bmatch\b', stripped_content))

        f_orphaned_todos = 0
        for match in re.finditer(r'(?://|#)\s*(TODO|FIXME|HACK|TEMP):?\s*(.*)', content, re.IGNORECASE):
            comment_text = match.group(2).strip().lower()
            if comment_text and comment_text not in todo_content:
                f_orphaned_todos += 1

        f_complex_functions = 0
        functions = extract_functions(stripped_content)
        for func in functions:
            if calculate_complexity(func) > 15:
                f_complex_functions += 1

        f_magic_numbers = 0
        f_magic_strings = 0

        for line in stripped_content.split('\n'):
            if 'const ' not in line and 'static ' not in line:
                numbers = re.findall(r'\b\d+\b', line)
                for num in numbers:
                    try:
                        val = int(num)
                        if val > 1 and val not in [1000, 1024, 2048]:
                            f_magic_numbers += 1
                    except ValueError:
                        pass

        # Magic strings logic: strings that are not empty and not in const/static
        # (This is approximate, operating on original content for string literals)
        for line in content.split('\n'):
            if 'const ' not in line and 'static ' not in line and 'println!' not in line and 'format!' not in line:
                strings = re.findall(r'"([^"\\]*)"', line)
                for s in strings:
                    if len(s) > 2: # Ignore short strings like "", " ", "\n"
                        f_magic_strings += 1

        structs = re.findall(r'struct\s+[a-zA-Z0-9_]+\s*\{([^}]*)\}', stripped_content)
        for struct_body in structs:
            fields = re.sub(r'\s+', '', struct_body)
            if fields in struct_map:
                struct_map[fields] += 1
            else:
                struct_map[fields] = 1

        enums = re.findall(r'enum\s+[a-zA-Z0-9_]+\s*\{([^}]*)\}', stripped_content)
        for enum_body in enums:
            fields = re.sub(r'\s+', '', enum_body)
            if fields in enum_map:
                enum_map[fields] += 1
            else:
                enum_map[fields] = 1

        # Total debt for density calculation
        f_total_debt = f_dead_code + f_unused_imports + f_unreachable + f_unwraps + f_expects + f_orphaned_todos + f_complex_functions + f_magic_numbers + f_magic_strings

        file_metrics[fpath] = f_total_debt

        total_metrics["dead_code"] += f_dead_code
        total_metrics["unused_imports"] += f_unused_imports
        total_metrics["unreachable_branches"] += f_unreachable
        total_metrics["unwraps"] += f_unwraps
        total_metrics["expects"] += f_expects
        total_metrics["qm_operator"] += f_qm
        total_metrics["matches"] += f_matches
        total_metrics["orphaned_todos"] += f_orphaned_todos
        total_metrics["complex_functions"] += f_complex_functions
        total_metrics["magic_numbers"] += f_magic_numbers
        total_metrics["magic_strings"] += f_magic_strings

    for fields, count in struct_map.items():
        if count > 1:
            total_metrics["duplicated_structs"] += count - 1

    for fields, count in enum_map.items():
        if count > 1:
            total_metrics["duplicated_enums"] += count - 1

    return total_metrics, file_metrics

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        return

    results = {}

    for commit in commits:
        results[commit] = analyze_commit(commit)

    with open("sanitation_scorecard.md", "w") as f:
        f.write("# Sanitation Scorecard\n\n")
        f.write("| Commit | Debt Ann. | Orphaned TODOs | Complex Functions | Error Mix (unwrap/expect/?/match) | Magic (Num/Str) | Duplicated Structs/Enums |\n")
        f.write("|--------|-----------|----------------|-------------------|-----------------------------------|-----------------|--------------------------|\n")

        for commit in commits:
            t, _ = results[commit]
            debt_ann = f"Dead: {t['dead_code']}, Unused: {t['unused_imports']}, Unreach: {t['unreachable_branches']}"
            err_mix = f"{t['unwraps']} / {t['expects']} / {t['qm_operator']} / {t['matches']}"
            magic = f"{t['magic_numbers']} / {t['magic_strings']}"
            dups = f"{t['duplicated_structs']} / {t['duplicated_enums']}"

            f.write(f"| {commit} | {debt_ann} | {t['orphaned_todos']} | {t['complex_functions']} | {err_mix} | {magic} | {dups} |\n")

        f.write("\n## Trajectory Analysis\n")

        current_totals, current_files = results[commits[0]]
        oldest_totals, oldest_files = results[commits[-1]]

        f.write("Comparing current commit to the oldest of the last 6:\n")
        for key in current_totals.keys():
            diff = current_totals[key] - oldest_totals[key]
            if diff > 0:
                f.write(f"- 🔴 **{key}**: Degrading (+{diff})\n")
            elif diff < 0:
                f.write(f"- 🟢 **{key}**: Improving ({diff})\n")
            else:
                f.write(f"- ⚪ **{key}**: Unchanged\n")

        f.write("\n## Files with Increasing Debt Density\n")
        f.write("| File | Oldest Debt | Current Debt | Difference |\n")
        f.write("|------|-------------|--------------|------------|\n")

        increasing_files = []
        for fpath, current_debt in current_files.items():
            oldest_debt = oldest_files.get(fpath, 0)
            if current_debt > oldest_debt:
                increasing_files.append((fpath, oldest_debt, current_debt))

        if increasing_files:
            for fpath, old, curr in sorted(increasing_files, key=lambda x: x[2]-x[1], reverse=True):
                f.write(f"| {fpath} | {old} | {curr} | +{curr-old} |\n")
        else:
            f.write("| (None) | | | |\n")

    print("Sanitation scorecard generated at sanitation_scorecard.md")

if __name__ == "__main__":
    main()
