pub struct SecretFilter;

impl SecretFilter {
    pub fn clean(content: &str) -> String {
        let mut sanitized = String::new();
        let keys = ["api-key", "api_key", "apikey", "password", "token", "secret", "private-key", "private_key", "privatekey"];
        
        for line in content.lines() {
            let mut processed_line = line.to_string();
            let lower_line = line.to_lowercase();
            
            // 1. Check for key/value secrets
            for key in &keys {
                if let Some(key_idx) = lower_line.find(key) {
                    let after_key = &line[key_idx + key.len()..];
                    let after_key_lower = &lower_line[key_idx + key.len()..];
                    if let Some(sep_idx) = after_key_lower.find(|c| c == ':' || c == '=') {
                        let between = after_key_lower[..sep_idx].trim();
                        if between.is_empty() {
                            let value_part = &after_key[sep_idx + 1..];
                            let trimmed_val = value_part.trim();
                            
                            // Find matching quotes
                            if let Some(q_start) = trimmed_val.find(|c| c == '\'' || c == '"') {
                                if let Some(&quote_byte) = trimmed_val.as_bytes().get(q_start) {
                                    let quote_char = quote_byte as char;
                                    if let Some(q_end) = trimmed_val[q_start + 1..].find(quote_char) {
                                        let rest = &trimmed_val[q_start + 1 + q_end + 1..];
                                        
                                        // Extract prefix up to quote start
                                        if let Some(line_offset) = line.find(trimmed_val) {
                                            let prefix = &line[..line_offset + q_start];
                                            
                                            processed_line = format!("{}\"[REDACTED]\"{}", prefix, rest);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // 2. Check for bearer token
            if let Some(bearer_idx) = lower_line.find("bearer ") {
                let prefix = &line[..bearer_idx + 7];
                processed_line = format!("{}[REDACTED]", prefix);
            }
            
            if !sanitized.is_empty() {
                sanitized.push('\n');
            }
            sanitized.push_str(&processed_line);
        }
        
        if content.ends_with('\n') && !sanitized.ends_with('\n') {
            sanitized.push('\n');
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
