use winnow::prelude::*;
use winnow::ascii::digit1;
use winnow::token::take;
use winnow::combinator::{alt, opt};
use winnow::stream::Stream;
use winnow::error::ModalResult;

fn parse_digits(input: &mut &str) -> ModalResult<u32> {
    let s = digit1.parse_next(input)?;
    s.parse::<u32>().map_err(|_| winnow::error::ErrMode::Backtrack(winnow::error::ContextError::new()))
}

// Parses Unix timestamp (exactly 10 digits)
pub fn parse_unix_timestamp(input: &mut &str) -> ModalResult<i64> {
    let raw = take(10usize).parse_next(input)?;
    if let Some(c) = input.chars().next() {
        if c.is_ascii_digit() {
            return Err(winnow::error::ErrMode::Backtrack(winnow::error::ContextError::new()));
        }
    }
    raw.parse::<i64>().map_err(|_| winnow::error::ErrMode::Backtrack(winnow::error::ContextError::new()))
}

// Parses YYYY-MM-DD or YYYY/MM/DD
fn parse_ymd(input: &mut &str) -> ModalResult<(i32, u32, u32)> {
    let year = parse_digits(input)?;
    let _ = alt(('-', '/')).parse_next(input)?;
    let month = parse_digits(input)?;
    let _ = alt(('-', '/')).parse_next(input)?;
    let day = parse_digits(input)?;
    Ok((year as i32, month, day))
}

// Parses HH:MM:SS
fn parse_hms(input: &mut &str) -> ModalResult<(u32, u32, u32)> {
    let hour = parse_digits(input)?;
    let _ = ':'.parse_next(input)?;
    let minute = parse_digits(input)?;
    let _ = ':'.parse_next(input)?;
    let second = parse_digits(input)?;
    Ok((hour, minute, second))
}

// Parses flexible ISO-8601/human date-time into NaiveDateTime with zero allocation
pub fn parse_flexible_date(input: &mut &str) -> ModalResult<chrono::NaiveDateTime> {
    let checkpoint = input.checkpoint();
    
    let res = (|| -> ModalResult<chrono::NaiveDateTime> {
        let (year, month, day) = parse_ymd.parse_next(input)?;
        
        // Optional separator 'T' or space
        let has_time = opt(alt(('T', ' '))).parse_next(input)?.is_some();
        
        let (hour, minute, second) = if has_time {
            parse_hms.parse_next(input)?
        } else {
            (0, 0, 0)
        };
        
        // Optional timezone suffix (e.g., 'Z' or '+00:00' or '-05:00')
        let _ = opt(alt((
            "Z".map(|_| ()),
            ("Z", parse_digits, ':', parse_digits).map(|_| ()),
            ('+', parse_digits, opt((':', parse_digits))).map(|_| ()),
            ('-', parse_digits, opt((':', parse_digits))).map(|_| ()),
        ))).parse_next(input)?;

        let date = chrono::NaiveDate::from_ymd_opt(year, month, day)
            .ok_or_else(|| winnow::error::ErrMode::Backtrack(winnow::error::ContextError::new()))?;
        let time = chrono::NaiveTime::from_hms_opt(hour, minute, second)
            .ok_or_else(|| winnow::error::ErrMode::Backtrack(winnow::error::ContextError::new()))?;
        
        Ok(chrono::NaiveDateTime::new(date, time))
    })();

    if res.is_err() {
        input.reset(&checkpoint);
    }
    res
}

// Robust character-by-character state-machine scanner for wiki-links
pub fn parse_wiki_link(mut input: &str) -> Option<(&str, Option<&str>)> {
    if !input.starts_with("[[") { return None; }
    input = &input[2..];
    
    let mut target_end = None;
    let mut label_start = None;
    
    let mut chars = input.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        match ch {
            '\\' => {
                // Skip the next character to allow escaping (e.g. [[Page\|Name]])
                let _ = chars.next();
            }
            '|' if target_end.is_none() => {
                target_end = Some(idx);
                label_start = Some(idx + 1);
            }
            ']' => {
                if let Some((_, ']')) = chars.peek() {
                    let target = match target_end {
                        Some(end) => input[..end].trim(),
                        None => input[..idx].trim(),
                    };
                    let label = label_start.map(|start| input[start..idx].trim());
                    return Some((target, label));
                }
            }
            _ => {}
        }
    }
    None
}

// Scans entire text for double-bracket wiki-links and returns all unique target pages
pub fn extract_wiki_links(text: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut start = 0;
    while let Some(open_idx) = text[start..].find("[[") {
        let open_pos = start + open_idx;
        if let Some((target, _)) = parse_wiki_link(&text[open_pos..]) {
            let cleaned_target = target.replace("\\|", "|");
            if !cleaned_target.is_empty() && !links.contains(&cleaned_target) {
                links.push(cleaned_target);
            }
            // Move start index past the parsed target (we look for next double brackets)
            start = open_pos + 2;
        } else {
            start = open_pos + 2;
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unix_timestamp() {
        let mut input = "1719918231";
        assert_eq!(parse_unix_timestamp(&mut input), Ok(1719918231));
        
        let mut input = "17199182310"; // 11 digits
        assert!(parse_unix_timestamp(&mut input).is_err());
    }

    #[test]
    fn test_parse_flexible_date() {
        let mut input = "2026-07-08T11:22:20Z";
        let dt = parse_flexible_date(&mut input).unwrap();
        assert_eq!(dt.date().to_string(), "2026-07-08");
        assert_eq!(dt.time().to_string(), "11:22:20");

        let mut input = "2026/07/08 11:22:20";
        let dt = parse_flexible_date(&mut input).unwrap();
        assert_eq!(dt.date().to_string(), "2026-07-08");
        assert_eq!(dt.time().to_string(), "11:22:20");

        let mut input = "2026-07-08";
        let dt = parse_flexible_date(&mut input).unwrap();
        assert_eq!(dt.date().to_string(), "2026-07-08");
        assert_eq!(dt.time().to_string(), "00:00:00");
    }

    #[test]
    fn test_extract_wiki_links() {
        let text = "Hello [[Target Page|My Label]] and [[Another Page]] and [[Escaped\\|Page]].";
        let links = extract_wiki_links(text);
        assert_eq!(links, vec!["Target Page", "Another Page", "Escaped|Page"]);
    }
}
