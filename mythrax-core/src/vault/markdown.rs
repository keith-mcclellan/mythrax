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

/// Sanitizes a markdown body by removing any leading duplicate title lines, H1-H4 headers matching the node title,
/// or lines consisting strictly of repeated exact title tokens (e.g., "title title title...").
pub fn sanitize_body_title_repetitions(body: &str, title_candidates: &[&str]) -> String {
    let mut lines = body.lines();
    let mut clean_lines = Vec::new();
    let mut in_leading_header_zone = true;

    // Build exact normalized candidate strings
    let mut norm_candidates = Vec::new();
    for cand in title_candidates {
        let trimmed_cand = cand.trim().trim_start_matches('#').trim();
        if !trimmed_cand.is_empty() {
            norm_candidates.push(trimmed_cand.to_lowercase());
            let short_cand = trimmed_cand.rsplit('/').next().unwrap_or(trimmed_cand);
            if !short_cand.is_empty() {
                norm_candidates.push(short_cand.to_lowercase());
            }
        }
    }

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        if in_leading_header_zone {
            if trimmed.is_empty() {
                continue;
            }

            // Strip any #, ##, ###, #### header prefixes
            let stripped_header = trimmed.trim_start_matches('#').trim();
            let stripped_lower = stripped_header.to_lowercase();

            let mut is_title_rep = false;

            for cand in &norm_candidates {
                // Exact line match to full candidate or short candidate
                if stripped_lower == *cand {
                    is_title_rep = true;
                    break;
                }

                // Exact repetition match: line consists purely of 2+ repetitions of exact candidate
                let cand_words: Vec<&str> = cand.split_whitespace().collect();
                let line_words: Vec<&str> = stripped_lower.split_whitespace().collect();

                if !cand_words.is_empty() && line_words.len() >= cand_words.len() * 2 {
                    let is_exact_repetition = line_words
                        .chunks(cand_words.len())
                        .all(|chunk| chunk == cand_words);
                    if is_exact_repetition {
                        is_title_rep = true;
                        break;
                    }
                }
            }

            if is_title_rep {
                continue;
            }

            // The moment we hit the first non-title-repetition line, we exit the leading zone permanently!
            in_leading_header_zone = false;
        }

        clean_lines.push(line);
    }

    clean_lines.join("\n")
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
                if !plain_text.is_empty() && !plain_text.ends_with(' ') {
                    plain_text.push(' ');
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                if !plain_text.is_empty() && !plain_text.ends_with(' ') {
                    plain_text.push(' ');
                }
            }
            Event::Text(text) => {
                append_text(&mut plain_text, &text);
            }
            Event::Code(code) => {
                append_text(&mut plain_text, &code);
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
        assert!(plain.contains("fn main"));
        assert!(plain.contains("println"));
        assert!(!plain.contains("<div>"));
    }

    #[test]
    fn test_sanitize_body_title_repetitions() {
        let title = "general/cto_review_critique_part1";
        let short_title = "cto_review_critique_part1";
        let body = "cto_review_critique_part1 cto_review_critique_part1 cto_review_critique_part1 cto_review_critique_part1\n\n# cto_review_critique_part1\n\n# general/cto_review_critique_part1\n\nCTO Adversarial Critique: Mythrax v2.6.0 Code Review\nReviewer : Adversarial CTO Reviewer";
        
        let cleaned = sanitize_body_title_repetitions(body, &[title, short_title]);
        assert_eq!(
            cleaned,
            "CTO Adversarial Critique: Mythrax v2.6.0 Code Review\nReviewer : Adversarial CTO Reviewer"
        );
    }
}

