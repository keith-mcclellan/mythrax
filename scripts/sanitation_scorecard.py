#!/usr/bin/env python3
import subprocess
import re
import os
import sys
import urllib.request
import json

def get_commits():
    try:
        output = subprocess.check_output(["git", "rev-list", "--first-parent", "HEAD", "-n", "6"], text=True)
        commits = output.strip().split("\n")
        commits.reverse() # oldest to newest
        return [c for c in commits if c]
    except subprocess.CalledProcessError:
        return []

def get_file_content_at_commit(commit, filepath):
    try:
        return subprocess.check_output(["git", "show", f"{commit}:{filepath}"], text=True, stderr=subprocess.DEVNULL)
    except subprocess.CalledProcessError:
        return None

def get_tracked_files_at_commit(commit):
    try:
        output = subprocess.check_output(["git", "ls-tree", "-r", "--name-only", commit], text=True)
        return [f for f in output.strip().split("\n") if f]
    except subprocess.CalledProcessError:
        return []

def get_todo_md_content(commit):
    content = get_file_content_at_commit(commit, "TODO.md")
    return content if content else ""

def analyze_file(content, filename, todo_md_content):
    score = 0
    metrics = {
        "dead_code_allows": 0,
        "orphaned_todos": 0,
        "complexity_violations": 0,
        "inconsistent_errors": 0,
        "magic_numbers": 0,
        "magic_strings": 0,
        "duplicated_structs": 0
    }

    # 1. Dead code (handle multi-allows by just counting occurrences of the keywords inside #allow)
    allows = re.findall(r'#\[allow\((.*?)\)\]', content)
    for allow_list in allows:
        for keyword in ["dead_code", "unused_imports", "unreachable_code", "unused_variables"]:
            if keyword in allow_list:
                metrics["dead_code_allows"] += 1
                score += 1

    # 2. TODO/FIXME/HACK/TEMP orphaned
    todo_md_lines = [line.strip().lower() for line in todo_md_content.split('\n') if line.strip()]
    todos = re.findall(r'(?:#|//)\s*(TODO|FIXME|HACK|TEMP):?\s*(.*)', content, re.IGNORECASE)
    for tag, text in todos:
        text_lower = text.strip().lower()
        if not text_lower:
            metrics["orphaned_todos"] += 1
            score += 1
            continue

        found = False
        for line in todo_md_lines:
            if text_lower in line or line in text_lower:
                found = True
                break
        if not found:
            metrics["orphaned_todos"] += 1
            score += 1

    # 3. Cyclomatic complexity (Rust mainly) - improved block tracking instead of split('}')[0]
    if filename.endswith(".rs"):
        # Match function declarations and take the content until the next fn definition
        # It's an approximation, but better than taking the first block
        functions = re.split(r'\bfn\s+', content)
        for func in functions[1:]: # skip preamble before first fn
            # Simple heuristic to find end of fn: usually followed by another fn, or EOF
            body = func
            complexity = 1 # base
            complexity += len(re.findall(r'\bif\b', body))
            complexity += len(re.findall(r'\bfor\b', body))
            complexity += len(re.findall(r'\bwhile\b', body))
            complexity += len(re.findall(r'\bmatch\b', body))
            complexity += len(re.findall(r'\?', body))
            complexity += len(re.findall(r'&&', body))
            complexity += len(re.findall(r'\|\|', body))
            if complexity > 15:
                metrics["complexity_violations"] += 1
                score += 1

    # 4. Inconsistent error handling
    if filename.endswith(".rs"):
        has_unwrap = bool(re.search(r'\.unwrap\(\)', content))
        has_expect = bool(re.search(r'\.expect\(', content))
        has_question = bool(re.search(r'\?', content))
        if (has_unwrap or has_expect) and has_question:
            metrics["inconsistent_errors"] = 1
            score += 5 # heavily penalize inconsistent error handling

    # 5. Magic numbers (very heuristic)
    magic_num_matches = len(re.findall(r'(?:==|!=|>|<|>=|<=|\+=|-=|\*=|/=|let\s+\w+\s*=)\s*(\d{2,})', content))
    metrics["magic_numbers"] = magic_num_matches
    score += magic_num_matches

    # 6. Magic strings (heuristic: string literal checked in equality)
    if filename.endswith(".rs"):
        magic_string_matches = len(re.findall(r'(?:==|!=)\s*"([^"]{2,})"', content))
        metrics["magic_strings"] = magic_string_matches
        score += magic_string_matches

    return score, metrics

def extract_structs(content, filename):
    structs = {}
    if not filename.endswith(".rs"): return structs
    matches = re.finditer(r'struct\s+(\w+)\s*\{([^}]+)\}', content)
    for m in matches:
        name = m.group(1)
        fields_str = m.group(2)
        fields = [f.split(':')[0].strip() for f in fields_str.split(',') if ':' in f]
        if fields:
            structs[f"{filename}::{name}"] = set(fields)

    matches_enum = re.finditer(r'enum\s+(\w+)\s*\{([^}]+)\}', content)
    for m in matches_enum:
        name = m.group(1)
        fields_str = m.group(2)
        fields = [f.split('(')[0].strip() for f in fields_str.split(',') if f.strip()]
        if fields:
            structs[f"{filename}::{name}"] = set(fields)

    return structs

def main():
    commits = get_commits()
    if not commits:
        print("No commits found.")
        return

    history = []

    for commit in commits:
        files = get_tracked_files_at_commit(commit)
        todo_md = get_todo_md_content(commit)

        commit_score = 0
        file_scores = {}
        all_structs = {}

        for f in files:
            if (f.startswith("mythrax-core/") and f.endswith(".rs")) or (f.startswith("scripts/") and (f.endswith(".py") or f.endswith(".sh"))):
                content = get_file_content_at_commit(commit, f)
                if content:
                    score, metrics = analyze_file(content, f, todo_md)
                    structs = extract_structs(content, f)
                    all_structs.update(structs)

                    file_scores[f] = score
                    commit_score += score

        struct_names = list(all_structs.keys())
        dup_count = 0
        for i in range(len(struct_names)):
            for j in range(i+1, len(struct_names)):
                s1 = all_structs[struct_names[i]]
                s2 = all_structs[struct_names[j]]
                if len(s1) > 0 and s1 == s2:
                    dup_count += 1

        commit_score += dup_count * 5

        history.append({
            "commit": commit,
            "total_score": commit_score,
            "file_scores": file_scores,
            "dup_structs": dup_count
        })

    if not history:
        return

    latest = history[-1]

    scorecard = "## Sanitation Scorecard\n\n"
    scorecard += "**Trajectory (Last {} commits):**\n".format(len(history))
    for h in history:
        scorecard += f"- {h['commit'][:7]}: Debt Score {h['total_score']}\n"

    if len(history) > 1:
        if history[-1]['total_score'] > history[0]['total_score']:
            scorecard += "\n**Status:** 🚨 DEGRADING (Debt is increasing)\n"
        elif history[-1]['total_score'] < history[0]['total_score']:
            scorecard += "\n**Status:** ✅ IMPROVING (Debt is decreasing)\n"
        else:
            scorecard += "\n**Status:** ➖ STABLE\n"

    scorecard += "\n### Files with Increasing Debt Density\n"
    increasing_files = []
    if len(history) > 1:
        first_files = history[0]['file_scores']
        last_files = latest['file_scores']
        for f, score in last_files.items():
            old_score = first_files.get(f, 0)
            if score > old_score:
                increasing_files.append((f, old_score, score))

    if increasing_files:
        for f, old_score, new_score in sorted(increasing_files, key=lambda x: x[2]-x[1], reverse=True):
            scorecard += f"- `{f}`: {old_score} -> {new_score}\n"
    else:
        scorecard += "No files with increasing debt.\n"

    print(scorecard)

    event_name = os.environ.get("GITHUB_EVENT_NAME")
    if event_name == "push":
        try:
            repo = os.environ.get("GITHUB_REPOSITORY")
            token = os.environ.get("GH_TOKEN")
            if not repo or not token:
                print("Missing GITHUB_REPOSITORY or GH_TOKEN env vars")
                return

            commit_sha = commits[-1]

            # Use GitHub REST API via urllib.request instead of gh CLI
            url = f"https://api.github.com/repos/{repo}/commits/{commit_sha}/pulls"
            req = urllib.request.Request(url, headers={
                "Authorization": f"token {token}",
                "Accept": "application/vnd.github.v3+json"
            })

            with urllib.request.urlopen(req) as response:
                prs = json.loads(response.read().decode())

            if prs:
                pr_number = prs[0]["number"]
                comment_url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
                data = json.dumps({"body": scorecard}).encode("utf-8")

                comment_req = urllib.request.Request(comment_url, data=data, headers={
                    "Authorization": f"token {token}",
                    "Accept": "application/vnd.github.v3+json",
                    "Content-Type": "application/json"
                })

                with urllib.request.urlopen(comment_req) as comment_res:
                    print("Successfully posted scorecard to PR")

        except Exception as e:
            print(f"Error commenting on PR: {e}")

if __name__ == "__main__":
    main()
