import subprocess
import json
import os
import re
import shutil

EXCLUDE_DIRS = ['target', '.git', '.venv', 'node_modules', 'issues', 'tests', 'bench_data']

def run_cmd(cmd):
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    return result.stdout.strip()

def get_commits():
    out = run_cmd("git log -n 6 --format='%H'")
    commits = out.splitlines()[::-1]
    return commits

def scan_commit(commit):
    run_cmd(f"git checkout -f {commit}")

    with open("mythrax-core/clippy.toml", "w") as f:
        f.write('cognitive-complexity-threshold = 15\n')

    clippy_cmd = "cargo clippy --no-default-features --manifest-path mythrax-core/Cargo.toml --message-format=json -- -A warnings -W clippy::cognitive_complexity -W dead_code -W unused_imports -W unreachable_code"
    clippy_out = subprocess.run(clippy_cmd, shell=True, capture_output=True, text=True).stdout

    clippy_warnings = 0
    cognitive_complexities = 0

    for line in clippy_out.splitlines():
        try:
            msg = json.loads(line)
            if msg.get("reason") == "compiler-message":
                message = msg["message"]["message"]
                if "cognitive complexity" in message.lower():
                    cognitive_complexities += 1
                elif msg["message"]["level"] in ["warning", "error"]:
                    clippy_warnings += 1
        except:
            pass

    rust_files = []
    for root, dirs, files in os.walk('.'):
        dirs[:] = [d for d in dirs if d not in EXCLUDE_DIRS]
        for file in files:
            if file.endswith('.rs') or file.endswith('.py') or file.endswith('.sh'):
                rust_files.append(os.path.join(root, file))

    debt = {
        'dead_code_allows': 0,
        'orphaned_todos': 0,
        'unwraps': 0,
        'expects': 0,
        'clippy_warnings': clippy_warnings,
        'cognitive_complexities': cognitive_complexities,
        'magic_numbers': 0,
        'duplicated_types': 0
    }

    todo_md_content = ""
    try:
        with open("TODO.md", "r", encoding="utf-8") as f:
            todo_md_content = f.read().lower()
    except:
        pass

    struct_definitions = []

    for filepath in rust_files:
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                content = f.read()
                debt['dead_code_allows'] += len(re.findall(r'#\[allow\(dead_code\)\]', content))

                # Find all TODO, FIXME, HACK, TEMP comments
                comments = re.findall(r'//\s*(?:TODO|FIXME|HACK|TEMP)[:\s]*(.*)', content, re.IGNORECASE)
                for comment in comments:
                    comment = comment.strip().lower()
                    if comment and comment not in todo_md_content:
                        debt['orphaned_todos'] += 1

                debt['unwraps'] += len(re.findall(r'\.unwrap\(\)', content))
                debt['expects'] += len(re.findall(r'\.expect\(', content))

                # Naive check for magic numbers (excluding 0, 1) assigned directly
                # e.g. let x = 42;
                debt['magic_numbers'] += len(re.findall(r'=\s*[2-9][0-9]*\s*;', content))

                # Collect structs to check for duplication
                structs = re.findall(r'struct\s+([A-Za-z0-9_]+)\s*\{', content)
                struct_definitions.extend(structs)
        except:
            pass

    # Check for duplicated struct names
    counts = {}
    for struct in struct_definitions:
        counts[struct] = counts.get(struct, 0) + 1

    for count in counts.values():
        if count > 1:
            debt['duplicated_types'] += count - 1

    return debt

def main():
    original_branch = run_cmd("git rev-parse --abbrev-ref HEAD")
    if original_branch == "HEAD":
        original_branch = run_cmd("git rev-parse HEAD")

    commits = get_commits()
    results = {}

    # We must make sure the script is committed before iterating
    # so we don't lose it during checkout

    for commit in commits:
        results[commit] = scan_commit(commit)

    run_cmd(f"git checkout -f {original_branch}")

    scorecard = "## Architecture Sanitation Scorecard\n\n"
    scorecard += "| Commit | Clippy Warns | Cog. Cmplx | Dead Code | Orphan TODOs | Unwraps | Expects | Magic Nums | Dup. Types |\n"
    scorecard += "|---|---|---|---|---|---|---|---|---|\n"

    for commit, debt in results.items():
        scorecard += f"| {commit[:7]} | {debt['clippy_warnings']} | {debt['cognitive_complexities']} | {debt['dead_code_allows']} | {debt['orphaned_todos']} | {debt['unwraps']} | {debt['expects']} | {debt['magic_numbers']} | {debt['duplicated_types']} |\n"

    print(scorecard)

    with open('scorecard.md', 'w') as f:
        f.write(scorecard)

    if shutil.which("gh") and os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        run_cmd("gh pr comment -F scorecard.md")

    # Clean up artifacts
    if os.path.exists('scorecard.md'):
        os.remove('scorecard.md')
    if os.path.exists('mythrax-core/clippy.toml'):
        os.remove('mythrax-core/clippy.toml')

if __name__ == "__main__":
    main()
