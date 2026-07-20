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
