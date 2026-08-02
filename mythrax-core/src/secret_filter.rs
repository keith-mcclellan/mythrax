use std::sync::OnceLock;
use regex::Regex;

static AWS_KEY_REGEX: OnceLock<Regex> = OnceLock::new();
static GITHUB_TOKEN_REGEX: OnceLock<Regex> = OnceLock::new();
static JWT_REGEX: OnceLock<Regex> = OnceLock::new();
static PEM_REGEX: OnceLock<Regex> = OnceLock::new();
static ENV_SECRET_REGEX: OnceLock<Regex> = OnceLock::new();

pub struct SecretFilter;

impl SecretFilter {
    pub fn clean(content: &str) -> String {
        let aws_re = AWS_KEY_REGEX.get_or_init(|| Regex::new(r"AKIA[0-9A-Z]{16}").unwrap());
        let gh_re = GITHUB_TOKEN_REGEX.get_or_init(|| Regex::new(r"(ghp_[a-zA-Z0-9]{36}|gho_[a-zA-Z0-9]{36}|github_pat_[a-zA-Z0-9_]{22,})").unwrap());
        let jwt_re = JWT_REGEX.get_or_init(|| Regex::new(r"eyJ[a-zA-Z0-9_-]+\.eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+").unwrap());
        let pem_re = PEM_REGEX.get_or_init(|| Regex::new(r"-----BEGIN [A-Z ]+PRIVATE KEY-----[\s\S]*?-----END [A-Z ]+PRIVATE KEY-----").unwrap());
        let env_re = ENV_SECRET_REGEX.get_or_init(|| Regex::new(r"(?i)(export\s+)?(AWS_SECRET_ACCESS_KEY|API_KEY|SECRET_KEY|PASSWORD|TOKEN|AUTH_TOKEN)\s*=\s*([^\s'\x22]+)").unwrap());

        let res = aws_re.replace_all(content, "[REDACTED_AWS_KEY]");
        let res = gh_re.replace_all(&res, "[REDACTED_GITHUB_TOKEN]");
        let res = jwt_re.replace_all(&res, "[REDACTED_JWT]");
        let res = pem_re.replace_all(&res, "[REDACTED_PEM_KEY]");
        let res = env_re.replace_all(&res, "${1}${2}=[REDACTED]");

        let mut sanitized = String::new();
        let keys = [
            "api-key",
            "api_key",
            "apikey",
            "password",
            "token",
            "secret",
            "private-key",
            "private_key",
            "privatekey",
        ];

        for line in res.lines() {
            let mut processed_line = line.to_string();
            let lower_line = line.to_lowercase();

            for key in &keys {
                if let Some(key_idx) = lower_line.find(key) {
                    let after_key = &line[key_idx + key.len()..];
                    let after_key_lower = &lower_line[key_idx + key.len()..];
                    if let Some(sep_idx) = after_key_lower.find(|c| c == ':' || c == '=') {
                        let between = after_key_lower[..sep_idx].trim();
                        if between.is_empty() {
                            let value_part = &after_key[sep_idx + 1..];
                            let trimmed_val = value_part.trim();

                            if let Some(q_start) = trimmed_val.find(|c| c == '\'' || c == '"') {
                                if let Some(&quote_byte) = trimmed_val.as_bytes().get(q_start) {
                                    let quote_char = quote_byte as char;
                                    if let Some(q_end) = trimmed_val[q_start + 1..].find(quote_char)
                                    {
                                        let rest = &trimmed_val[q_start + 1 + q_end + 1..];
                                        if let Some(line_offset) = line.find(trimmed_val) {
                                            let prefix = &line[..line_offset + q_start];
                                            processed_line =
                                                format!("{}\"[REDACTED]\"{}", prefix, rest);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

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
        let content =
            "My config:\napi_key: 'sk-12345'\nsecret = \"supersecretpassword\"\nnormal_field: 42";
        let cleaned = SecretFilter::clean(content);
        assert!(cleaned.contains("[REDACTED]"));
        assert!(!cleaned.contains("sk-12345"));
        assert!(cleaned.contains("normal_field: 42"));
    }

    #[test]
    fn test_secret_filter_extended_patterns() {
        let aws_content = "AWS_KEY=AKIAIOSFODNN7EXAMPLE";
        let cleaned_aws = SecretFilter::clean(aws_content);
        assert!(cleaned_aws.contains("[REDACTED_AWS_KEY]"));

        let gh_content = "git token is ghp_1234567890abcdefghijklmnopqrstuvwxyz12 in code";
        let cleaned_gh = SecretFilter::clean(gh_content);
        assert!(cleaned_gh.contains("[REDACTED_GITHUB_TOKEN]"));
        assert!(!cleaned_gh.contains("ghp_1234567890"));
    }
}
