use std::path::Path;
use anyhow::{anyhow, Result};
use subtle::ConstantTimeEq;

/// Load the authentication token from the specified path.
/// Verifies that the file permissions are strictly 0600 on Unix/macOS.
/// Returns an explicit error if the token file is not found or permissions are insecure.
pub fn load_token<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(anyhow!("Token file not found: {:?}", path));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(path)?;
        let permissions = metadata.permissions();
        let mode = permissions.mode() & 0o777; // Mask extra permission bits
        if mode != 0o600 {
            return Err(anyhow!("Insecure permissions on token file: {:o} (must be 0600)", mode));
        }
    }

    let token = std::fs::read_to_string(path)?;
    let trimmed = token.trim().to_string();
    if trimmed.is_empty() {
        return Err(anyhow!("Token file is empty"));
    }
    Ok(trimmed)
}

/// Constant-time comparison to validate tokens and prevent timing attacks.
pub fn verify_token_constant_time(provided: &str, expected: &str) -> bool {
    let provided_bytes = provided.as_bytes();
    let expected_bytes = expected.as_bytes();

    if provided_bytes.len() != expected_bytes.len() {
        // Perform a constant-time comparison on the expected token to simulate the timing.
        let _ = expected_bytes.ct_eq(expected_bytes);
        false
    } else {
        provided_bytes.ct_eq(expected_bytes).unwrap_u8() == 1
    }
}

/// Retrieve the token from file, or generate a new one with strict 0600 permissions if missing or invalid.
pub fn get_or_create_token<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    if path.exists() {
        if let Ok(token) = load_token(path) {
            if !token.is_empty() {
                return Ok(token);
            }
        }
    }
    
    // Generate new token
    let new_token = uuid::Uuid::new_v4().to_string();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    // Write with 0600 permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(new_token.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, &new_token)?;
    }
    
    Ok(new_token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_get_or_create_token_generates_valid_uuid() {
        let temp_dir = std::env::temp_dir();
        let token_path = temp_dir.join(format!("test_token_v4_{}.txt", uuid::Uuid::new_v4()));
        
        // Ensure clean state
        let _ = fs::remove_file(&token_path);

        let token = get_or_create_token(&token_path).expect("Failed to get or create token");
        
        // Basic UUID v4 validation: 36 chars, contains hyphens, version 4
        assert_eq!(token.len(), 36);
        assert!(token.contains('-'));
        assert!(token.chars().nth(14).unwrap() == '4');
        
        // Cleanup
        let _ = fs::remove_file(&token_path);
    }

    #[test]
    fn test_token_file_permissions() {
        let temp_dir = std::env::temp_dir();
        let token_path = temp_dir.join(format!("test_token_perms_{}.txt", uuid::Uuid::new_v4()));
        
        // Ensure clean state
        let _ = fs::remove_file(&token_path);

        let _ = get_or_create_token(&token_path).expect("Failed to get or create token");

        // Check permissions
        let metadata = fs::metadata(&token_path).expect("Failed to read metadata");
        let permissions = metadata.permissions();
        
        // On Unix, check that only owner has read/write (0o600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = permissions.mode();
            // Mask out file type bits to get permission bits
            assert_eq!(mode & 0o777, 0o600, "Token file should have 0600 permissions");
        }

        // Cleanup
        let _ = fs::remove_file(&token_path);
    }

    #[test]
    fn test_successive_calls_load_existing_token() {
        let temp_dir = std::env::temp_dir();
        let token_path = temp_dir.join(format!("test_token_persist_{}.txt", uuid::Uuid::new_v4()));
        
        // Ensure clean state
        let _ = fs::remove_file(&token_path);

        // First call generates a token
        let token1 = get_or_create_token(&token_path).expect("Failed to get or create token");
        
        // Second call should return the exact same token
        let token2 = get_or_create_token(&token_path).expect("Failed to get or create token");
        
        assert_eq!(token1, token2, "Successive calls should return the same existing token");

        // Cleanup
        let _ = fs::remove_file(&token_path);
    }
}
