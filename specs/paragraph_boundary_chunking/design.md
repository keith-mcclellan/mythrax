# Design: Paragraph-Boundary and Code-Object aware Chunking

## Overview
We will implement a hierarchical, greedy chunking algorithm inside `chunk_parsed_content`. 

## Execution Flow
1. **Quick Check**: If `content.len() <= limit`, return `vec![content.to_string()]` immediately.
2. **Normalize Newlines**: Convert all occurrences of `\r\n` to `\n` to simplify delimiter processing.
3. **Split by Delimiter**:
   - Split input by double newlines (`\n\n`) to get a list of high-level blocks (paragraphs/functions).
4. **Greedy Assembly with Fallback**:
   - Keep a `current_chunk: String`.
   - Iterate through each paragraph:
     - Skip empty paragraphs.
     - Calculate the size if this paragraph were added to the `current_chunk` (separated by `\n\n`).
     - If it fits (`needed_len <= limit`), append it to `current_chunk` joined by `\n\n`.
     - If it does not fit:
       - Flush `current_chunk` if it is not empty.
       - Check if the paragraph itself exceeds the limit:
         - **If no**: `current_chunk = paragraph.to_string()`.
         - **If yes**: Trigger the **Line Fallback**:
           - Split the paragraph by single newlines (`\n`).
           - Iterate through each line:
             - Calculate the size if this line were added to `current_chunk` (separated by `\n`).
             - If it fits, append it joined by `\n`.
             - If it does not fit:
               - Flush `current_chunk` if not empty.
               - Check if the line itself exceeds the limit:
                 - **If no**: `current_chunk = line.to_string()`.
                 - **If yes**: Trigger the **Character Fallback**:
                   - Split the line by character slices of exactly `limit` size.
                   - Push all full slices to `chunks`.
                   - `current_chunk = remaining_slice.to_string()`.
5. **Final Flush**: If `current_chunk` is not empty, push it to `chunks`.

## Code Example Draft
```rust
pub fn chunk_parsed_content(content: &str, limit: usize) -> Vec<String> {
    if content.len() <= limit {
        return vec![content.to_string()];
    }

    let normalized = content.replace("\r\n", "\n");
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for paragraph in normalized.split("\n\n") {
        if paragraph.is_empty() {
            continue;
        }

        let needed_len = if current_chunk.is_empty() {
            paragraph.len()
        } else {
            current_chunk.len() + 2 + paragraph.len()
        };

        if needed_len <= limit {
            if !current_chunk.is_empty() {
                current_chunk.push_str("\n\n");
            }
            current_chunk.push_str(paragraph);
        } else {
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.clone());
                current_chunk.clear();
            }

            if paragraph.len() > limit {
                // Fallback to lines
                for line in paragraph.split('\n') {
                    let needed_len = if current_chunk.is_empty() {
                        line.len()
                    } else {
                        current_chunk.len() + 1 + line.len()
                    };

                    if needed_len <= limit {
                        if !current_chunk.is_empty() {
                            current_chunk.push('\n');
                        }
                        current_chunk.push_str(line);
                    } else {
                        if !current_chunk.is_empty() {
                            chunks.push(current_chunk.clone());
                            current_chunk.clear();
                        }

                        if line.len() > limit {
                            // Fallback to character split
                            let mut remaining = line;
                            while remaining.len() > limit {
                                let (part, rest) = remaining.split_at(limit);
                                chunks.push(part.to_string());
                                remaining = rest;
                            }
                            current_chunk = remaining.to_string();
                        } else {
                            current_chunk = line.to_string();
                        }
                    }
                }
            } else {
                current_chunk = paragraph.to_string();
            }
        }
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}
```

## Traceability & Safety Boundaries
- Every character is preserved; no content is discarded (other than empty lines or empty paragraphs which don't carry text content).
- Maximum chunk length is strictly guaranteed to be `<= limit`.
- Coherence is maximized by splitting at the highest possible structural level.
