use anyhow::Result;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub content: String,
}

pub fn extract_symbols(content: &str, file_ext: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    // Regexes
    let rust_re =
        Regex::new(r"^(?:pub\s+)?(struct|fn|enum|trait|impl)\s+([a-zA-Z0-9_<>]+)").unwrap();
    let ts_re =
        Regex::new(r"^(?:export\s+)?(class|function|interface|type)\s+([a-zA-Z0-9_]+)").unwrap();
    let py_re = Regex::new(r"^(class|def)\s+([a-zA-Z0-9_]+)").unwrap();

    let re = match file_ext {
        "rs" => &rust_re,
        "ts" | "tsx" | "js" | "jsx" => &ts_re,
        "py" => &py_re,
        _ => return symbols,
    };

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(cap) = re.captures(trimmed) {
            let kind = cap.get(1).unwrap().as_str().to_string();
            let name = cap.get(2).unwrap().as_str().to_string();

            // Extract block: collect lines until the next line that matches a symbol
            let mut block_lines = Vec::new();
            block_lines.push(*line);

            for next_line in lines.iter().skip(i + 1) {
                let next_trimmed = next_line.trim();
                if re.is_match(next_trimmed) {
                    break;
                }
                block_lines.push(*next_line);
            }

            let block_content = block_lines.join("\n");
            symbols.push(Symbol {
                name,
                kind,
                content: block_content,
            });
        }
    }

    symbols
}

pub async fn page_code_block(
    backend: &crate::db::SurrealBackend,
    content: &str,
    file_ext: &str,
) -> Result<String> {
    let symbols = extract_symbols(content, file_ext);
    if symbols.is_empty() {
        return Ok(content.to_string());
    }

    let mut page_map = Vec::new();
    let mut paged_content = content.to_string();

    for sym in symbols {
        let page_id = format!(
            "page_{}_{}",
            sym.kind.to_lowercase(),
            sym.name.to_lowercase().replace('<', "_").replace('>', "_")
        );

        // Save/Archive symbol to SurrealDB symbol_archive table
        let sql = "
            UPSERT type::record('symbol_archive', $page_id) CONTENT {
                page_id: $page_id,
                symbol_name: $symbol_name,
                kind: $kind,
                content: $content,
                timestamp: time::now()
            };
        ";
        backend
            .db
            .query(sql)
            .bind(("page_id", page_id.clone()))
            .bind(("symbol_name", sym.name.clone()))
            .bind(("kind", sym.kind.clone()))
            .bind(("content", sym.content.clone()))
            .await?
            .check()?;

        // Replace symbol content in the text with the page ID reference
        if paged_content.contains(&sym.content) {
            paged_content = paged_content.replace(
                &sym.content,
                &format!("[Paged Symbol: Reference {}]", page_id),
            );
        }

        page_map.push(format!(
            "Symbol: {} ({}) -> Page ID: {}",
            sym.name, sym.kind, page_id
        ));
    }

    if !page_map.is_empty() {
        let map_str = format!(
            "\n\n=== Symbol Page Map ===\n{}\n=======================\n\n",
            page_map.join("\n")
        );
        paged_content.push_str(&map_str);
    }

    Ok(paged_content)
}

pub async fn intercept_and_restore_symbols(
    backend: &crate::db::SurrealBackend,
    text: &str,
) -> String {
    let mut restored = text.to_string();

    // Find all occurrences of "page_[a-zA-Z0-9_]+"
    let re = Regex::new(r"page_[a-zA-Z0-9_]+").unwrap();
    let mut page_ids: Vec<String> = re.find_iter(text).map(|m| m.as_str().to_string()).collect();

    // Deduplicate page IDs
    page_ids.sort();
    page_ids.dedup();

    for pid in page_ids {
        // Query symbol_archive for this page_id
        let sql = "SELECT VALUE content FROM type::record('symbol_archive', $page_id);";
        if let Ok(mut response) = backend.db.query(sql).bind(("page_id", pid.clone())).await {
            if let Ok(Some(symbol_content)) = response.take::<Option<String>>(0) {
                // Swap it back!
                let placeholder = format!("[Paged Symbol: Reference {}]", pid);
                if restored.contains(&placeholder) {
                    restored = restored.replace(&placeholder, &symbol_content);
                } else {
                    restored =
                        restored.replace(&pid, &format!("{}:\n```\n{}\n```", pid, symbol_content));
                }
            }
        }
    }

    restored
}
