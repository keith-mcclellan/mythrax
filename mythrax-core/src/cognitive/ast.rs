use crate::contracts::CodeSymbol;
use crate::cognitive::pipeline::slugify_title;
use crate::store::MarkdownStore;
use anyhow::Result;
use regex::Regex;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;

/// Regex patterns for multi-language AST symbol extraction
static RS_SYMBOL_RE: OnceLock<Regex> = OnceLock::new();
static PY_SYMBOL_RE: OnceLock<Regex> = OnceLock::new();
static TS_SYMBOL_RE: OnceLock<Regex> = OnceLock::new();
static GO_SYMBOL_RE: OnceLock<Regex> = OnceLock::new();

pub fn extract_code_ast(file_path: &str, content: &str, scope: &str) -> Vec<CodeSymbol> {
    let path = Path::new(file_path);
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let file_slug = slugify_title(file_path);

    let mut symbols = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    match ext {
        "rs" => extract_rust_symbols(file_path, &file_slug, &lines, scope, &mut symbols),
        "py" => extract_python_symbols(file_path, &file_slug, &lines, scope, &mut symbols),
        "ts" | "js" | "tsx" | "jsx" => extract_ts_symbols(file_path, &file_slug, &lines, scope, &mut symbols),
        "go" => extract_go_symbols(file_path, &file_slug, &lines, scope, &mut symbols),
        _ => {}
    }

    symbols
}

fn extract_rust_symbols(
    file_path: &str,
    file_slug: &str,
    lines: &[&str],
    scope: &str,
    symbols: &mut Vec<CodeSymbol>,
) {
    let re = RS_SYMBOL_RE.get_or_init(|| {
        Regex::new(r"^\s*(?:pub(?:\(crate\))?\s+)?(fn|struct|enum|trait|type|const)\s+([A-Za-z0-9_]+)").unwrap()
    });

    let mut current_doc = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            current_doc.push(trimmed.trim_start_matches("///").trim_start_matches("//!").trim());
            continue;
        }

        if let Some(caps) = re.captures(line) {
            let symbol_type = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let name = caps.get(2).map_or("", |m| m.as_str()).to_string();

            let doc_comment = if current_doc.is_empty() {
                None
            } else {
                let doc = current_doc.join("\n");
                current_doc.clear();
                Some(doc)
            };

            let start_line = idx + 1;
            let end_line = (idx + 15).min(lines.len());
            let signature = line.trim().to_string();

            symbols.push(CodeSymbol {
                id: None,
                name,
                symbol_type,
                file_path: file_path.to_string(),
                file_slug: file_slug.to_string(),
                start_line,
                end_line,
                signature,
                doc_comment,
                call_graph: None,
                scope: scope.to_string(),
                embedding: None,
                created_at: Some(chrono::Utc::now()),
            });
        } else if !trimmed.starts_with("//") {
            current_doc.clear();
        }
    }
}

fn extract_python_symbols(
    file_path: &str,
    file_slug: &str,
    lines: &[&str],
    scope: &str,
    symbols: &mut Vec<CodeSymbol>,
) {
    let re = PY_SYMBOL_RE.get_or_init(|| {
        Regex::new(r"^\s*(def|class)\s+([A-Za-z0-9_]+)").unwrap()
    });

    for (idx, line) in lines.iter().enumerate() {
        if let Some(caps) = re.captures(line) {
            let symbol_type = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let name = caps.get(2).map_or("", |m| m.as_str()).to_string();

            let start_line = idx + 1;
            let end_line = (idx + 15).min(lines.len());
            let signature = line.trim().to_string();

            symbols.push(CodeSymbol {
                id: None,
                name,
                symbol_type,
                file_path: file_path.to_string(),
                file_slug: file_slug.to_string(),
                start_line,
                end_line,
                signature,
                doc_comment: None,
                call_graph: None,
                scope: scope.to_string(),
                embedding: None,
                created_at: Some(chrono::Utc::now()),
            });
        }
    }
}

fn extract_ts_symbols(
    file_path: &str,
    file_slug: &str,
    lines: &[&str],
    scope: &str,
    symbols: &mut Vec<CodeSymbol>,
) {
    let re = TS_SYMBOL_RE.get_or_init(|| {
        Regex::new(r"^\s*(?:export\s+)?(?:async\s+)?(function|class|interface|type|const|enum)\s+([A-Za-z0-9_]+)").unwrap()
    });

    for (idx, line) in lines.iter().enumerate() {
        if let Some(caps) = re.captures(line) {
            let symbol_type = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let name = caps.get(2).map_or("", |m| m.as_str()).to_string();

            let start_line = idx + 1;
            let end_line = (idx + 15).min(lines.len());
            let signature = line.trim().to_string();

            symbols.push(CodeSymbol {
                id: None,
                name,
                symbol_type,
                file_path: file_path.to_string(),
                file_slug: file_slug.to_string(),
                start_line,
                end_line,
                signature,
                doc_comment: None,
                call_graph: None,
                scope: scope.to_string(),
                embedding: None,
                created_at: Some(chrono::Utc::now()),
            });
        }
    }
}

fn extract_go_symbols(
    file_path: &str,
    file_slug: &str,
    lines: &[&str],
    scope: &str,
    symbols: &mut Vec<CodeSymbol>,
) {
    let re = GO_SYMBOL_RE.get_or_init(|| {
        Regex::new(r"^\s*(func|type|const|var)\s+([A-Za-z0-9_]+)").unwrap()
    });

    for (idx, line) in lines.iter().enumerate() {
        if let Some(caps) = re.captures(line) {
            let symbol_type = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let name = caps.get(2).map_or("", |m| m.as_str()).to_string();

            let start_line = idx + 1;
            let end_line = (idx + 15).min(lines.len());
            let signature = line.trim().to_string();

            symbols.push(CodeSymbol {
                id: None,
                name,
                symbol_type,
                file_path: file_path.to_string(),
                file_slug: file_slug.to_string(),
                start_line,
                end_line,
                signature,
                doc_comment: None,
                call_graph: None,
                scope: scope.to_string(),
                embedding: None,
                created_at: Some(chrono::Utc::now()),
            });
        }
    }
}

pub fn save_ast_symbols_to_vault(
    store: &Arc<MarkdownStore>,
    file_slug: &str,
    symbols: &[CodeSymbol],
) -> Result<()> {
    let relative_path = format!("reference/ast/{}.json", file_slug);
    let json_str = serde_json::to_string_pretty(symbols)?;
    store.write_file(&relative_path, &json_str)?;
    Ok(())
}
