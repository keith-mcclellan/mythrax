import os
import re

def fix_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    # We want to match .search( ... ) where there are exactly 10 comma-separated arguments.
    # To handle nesting (like Some("all") or &format!("...")), we can use a custom parser or
    # a regex that matches balanced parentheses or just count commas at the top level.
    
    # Let's write a simple parser to find all calls to ".search(" and check how many arguments they have.
    new_content = []
    idx = 0
    n = len(content)
    
    while idx < n:
        # Look for ".search("
        found = content.find(".search(", idx)
        if found == -1:
            new_content.append(content[idx:])
            break
        
        new_content.append(content[idx:found])
        
        # Parse the arguments inside search(...)
        arg_start = found + len(".search(")
        paren_count = 1
        curr = arg_start
        args_list = []
        current_arg = []
        
        in_quote = False
        quote_char = None
        
        while curr < n and paren_count > 0:
            c = content[curr]
            if c in ['"', "'"] and (curr == arg_start or content[curr-1] != '\\'):
                if in_quote:
                    if c == quote_char:
                        in_quote = False
                else:
                    in_quote = True
                    quote_char = c
                current_arg.append(c)
            elif c == '(' and not in_quote:
                paren_count += 1
                current_arg.append(c)
            elif c == ')' and not in_quote:
                paren_count -= 1
                if paren_count > 0:
                    current_arg.append(c)
            elif c == ',' and paren_count == 1 and not in_quote:
                args_list.append("".join(current_arg))
                current_arg = []
            else:
                current_arg.append(c)
            curr += 1
            
        last_arg = "".join(current_arg)
        if last_arg.strip():
            args_list.append(last_arg)
            
        # Clean comments from arguments and count them
        cleaned_args = []
        for arg in args_list:
            # Strip block comments
            arg_no_block = re.sub(r'/\*.*?\*/', '', arg, flags=re.DOTALL)
            # Strip line comments
            arg_lines = []
            for line in arg_no_block.split('\n'):
                if '//' in line:
                    line = line.split('//')[0]
                arg_lines.append(line)
            cleaned_arg = " ".join(arg_lines).strip()
            if cleaned_arg:
                cleaned_args.append(cleaned_arg)
                
        # Handle trailing comma where the last cleaned arg might be empty
        if cleaned_args and not cleaned_args[-1]:
            cleaned_args.pop()
        
        print(f"DEBUG: {filepath} found .search with {len(cleaned_args)} args: {cleaned_args}")
        
        # If we successfully closed the parenthesis and have exactly 10 arguments
        if paren_count == 0 and len(cleaned_args) == 10:
            new_call = f".search(\n        " + ",\n        ".join(cleaned_args) + ",\n        None,\n        true,\n    )"
            new_content.append(new_call)
        else:
            # Keep original
            new_content.append(content[found:curr])
            
        idx = curr
        
    updated = "".join(new_content)
    if updated != content:
        with open(filepath, 'w') as f:
            f.write(updated)
        print(f"Fixed {filepath}")

def main():
    # Fix src/db/backend.rs
    fix_file("mythrax-core/src/db/backend.rs")
    # Fix src/vault/watcher.rs
    fix_file("mythrax-core/src/vault/watcher.rs")
    
    # Fix all files in mythrax-core/tests/
    for root, _, files in os.walk("mythrax-core/tests"):
        for file in files:
            if file.endswith(".rs"):
                fix_file(os.path.join(root, file))

if __name__ == '__main__':
    main()
