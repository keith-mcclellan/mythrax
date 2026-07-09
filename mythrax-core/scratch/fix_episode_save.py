import os
import re

def fix_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    # Find all occurrences of "EpisodeSave {" or "crate::contracts::EpisodeSave {"
    # We want to insert "created_at: None," inside the braces, unless "created_at" is already there.
    
    # We can search for the pattern and find matching closing brace, or just do a simple replacement
    # where we check the block. Let's do a robust search:
    
    patterns = [r'\bEpisodeSave\s*\{', r'\bcontracts::EpisodeSave\s*\{']
    
    new_content = content
    for pattern in patterns:
        for match in list(re.finditer(pattern, new_content))[::-1]:
            start = match.end()
            # find matching closing brace to inspect the content of the initializer
            brace_count = 1
            idx = start
            n = len(new_content)
            while idx < n and brace_count > 0:
                if new_content[idx] == '{':
                    brace_count += 1
                elif new_content[idx] == '}':
                    brace_count -= 1
                idx += 1
            
            initializer_text = new_content[start:idx-1]
            if 'created_at' not in initializer_text:
                # Insert created_at: None,
                new_content = new_content[:start] + "\n        created_at: None," + new_content[start:]
                
    if new_content != content:
        with open(filepath, 'w') as f:
            f.write(new_content)
        print(f"Fixed EpisodeSave literals in {filepath}")

def main():
    for root, _, files in os.walk("mythrax-core/src"):
        for file in files:
            if file.endswith(".rs"):
                fix_file(os.path.join(root, file))

if __name__ == '__main__':
    main()
