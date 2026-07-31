use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde_yaml::Value;

/// Parses YAML frontmatter (between `---` lines) and extracts metadata.
/// Returns `(Some(Value), remainder)` if valid frontmatter is found,
/// or `(None, original_content)` otherwise.
pub fn parse_frontmatter(content: &str) -> (Option<Value>, String) {
    let content_trimmed = content.trim_start();
    if !content_trimmed.starts_with("---") {
        return (None, content.to_string());
    }

    // Split by "---" at most 3 parts.
    // The first part is empty (since it starts with "---").
    // The second part is the YAML.
    // The third part is the remaining body.
    let parts: Vec<&str> = content_trimmed.splitn(3, "---").collect();
    if parts.len() < 3 {
        return (None, content.to_string());
    }

    let yaml_str = parts[1];
    let body = parts[2].trim().to_string();

    let yaml_val = serde_yaml::from_str(yaml_str).ok();
    (yaml_val, body)
}

/// Strips markdown styling, headers, formatting, raw HTML, links, and code blocks
/// to produce clean, plain text for indexing/embeddings.
pub fn extract_plain_text(markdown: &str) -> String {
    let parser = Parser::new(markdown);
    let mut plain_text = String::new();
    let mut in_code_block = false;

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
            }
            Event::Text(text) => {
                if !in_code_block {
                    append_text(&mut plain_text, &text);
                }
            }
            Event::Code(code) => {
                if !in_code_block {
                    append_text(&mut plain_text, &code);
                }
            }
            Event::SoftBreak | Event::HardBreak
                if !plain_text.is_empty() && !plain_text.ends_with(' ') =>
            {
                plain_text.push(' ');
            }
            _ => {}
        }
    }

    plain_text.trim().to_string()
}

fn append_text(plain_text: &mut String, text: &str) {
    if !plain_text.is_empty() && !plain_text.ends_with(' ') && !text.starts_with(' ') {
        plain_text.push(' ');
    }
    plain_text.push_str(text);
}

/// Formats a WikiNode into standard Arbor quadruplet Markdown format with frontmatter and Wikilinks.
pub fn format_arbor_wiki_node_markdown(node: &crate::contracts::WikiNode, related_links: &[String]) -> String {
    let mut yaml_val = serde_json::Map::new();
    yaml_val.insert("name".to_string(), serde_json::json!(node.name));
    yaml_val.insert("scope".to_string(), serde_json::json!(node.scope));
    if let Some(ref h) = node.hypothesis {
        yaml_val.insert("hypothesis".to_string(), serde_json::json!(h));
    }
    if let Some(ref ci) = node.causal_insight {
        yaml_val.insert("causal_insight".to_string(), serde_json::json!(ci));
    }
    if let Some(ref ev) = node.raw_evidence {
        yaml_val.insert("raw_evidence".to_string(), serde_json::json!(ev));
    }
    if let Some(ref refs) = node.artifact_refs {
        yaml_val.insert("artifact_refs".to_string(), serde_json::json!(refs));
    }

    let yaml_str = serde_yaml::to_string(&yaml_val).unwrap_or_default();
    let mut body = format!("---\n{}\n---\n# {}\n\n", yaml_str.trim(), node.name);

    if let Some(ref h) = node.hypothesis {
        body.push_str(&format!("## Claim ($h_n$)\n{}\n\n", h));
    }

    if let Some(ref ci) = node.causal_insight {
        body.push_str(&format!("## Causal Insight ($\\iota_n$)\n{}\n\n", ci));
    }

    if !node.content.is_empty() {
        body.push_str(&format!("## Synthesis\n{}\n\n", node.content));
    }

    if let Some(ref ev) = node.raw_evidence {
        if !ev.is_empty() {
            body.push_str("## Evidence ($r_n$)\n");
            for e in ev {
                body.push_str(&format!("- {}\n", e));
            }
            body.push('\n');
        }
    }

    if let Some(ref refs) = node.artifact_refs {
        if !refs.is_empty() {
            body.push_str("## Artifact References ($\\mu_n$)\n");
            for r in refs {
                let target = r.strip_suffix(".md").unwrap_or(r);
                body.push_str(&format!("- [[{}|{}]]\n", target, r));
            }
            body.push('\n');
        }
    }

    if !related_links.is_empty() {
        body.push_str("## Graph Relations\n");
        for link in related_links {
            body.push_str(&format!("- {}\n", link));
        }
        body.push('\n');
    }

    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_valid() {
        let content = "---\ntitle: \"Hello\"\nscope: \"test\"\n---\nSome body text here.";
        let (yaml_opt, body) = parse_frontmatter(content);
        assert!(yaml_opt.is_some());
        let yaml = yaml_opt.unwrap();
        assert_eq!(yaml["title"].as_str(), Some("Hello"));
        assert_eq!(yaml["scope"].as_str(), Some("test"));
        assert_eq!(body, "Some body text here.");
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let content = "Some body text here without frontmatter.";
        let (yaml_opt, body) = parse_frontmatter(content);
        assert!(yaml_opt.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_extract_plain_text() {
        let markdown = "# Title\n\nThis is **bold** and *italic* text.\n\nHere is a [link](http://example.com) to a site.\n\nAnd some `inline code` here.\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\nRaw <div>HTML</div> block.";
        let plain = extract_plain_text(markdown);
        assert!(plain.contains("Title"));
        assert!(plain.contains("This is bold and italic text."));
        assert!(plain.contains("Here is a link to a site."));
        assert!(plain.contains("And some inline code here."));
        assert!(!plain.contains("fn main"));
        assert!(!plain.contains("println"));
        assert!(!plain.contains("<div>"));
    }
}
