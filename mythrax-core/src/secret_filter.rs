use regex::Regex;
use std::sync::OnceLock;

pub struct SecretFilter;

impl SecretFilter {
    pub fn clean(content: &str) -> String {
        static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
        let regexes = PATTERNS.get_or_init(|| {
            vec![
                // Redact things like: api_key: "abc-123", token = 'secret', password: 123
                Regex::new(r#"(?i)(api[-_]?key|password|token|secret|private[-_]?key)\s*[:=]\s*['"].*?['"]"#).unwrap(),
                // Redact bearer tokens
                Regex::new(r#"(?i)bearer\s+[A-Za-z0-9\-\._~\+\/]+=*"#).unwrap(),
            ]
        });

        let mut sanitized = content.to_string();
        for re in regexes {
            sanitized = re.replace_all(&sanitized, "$1: \"[REDACTED]\"").to_string();
        }
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_filter() {
        let content = "My config:\napi_key: 'sk-12345'\nsecret = \"supersecretpassword\"\nnormal_field: 42";
        let cleaned = SecretFilter::clean(content);
        assert!(cleaned.contains("[REDACTED]"));
        assert!(!cleaned.contains("sk-12345"));
        assert!(cleaned.contains("normal_field: 42"));
    }
}
