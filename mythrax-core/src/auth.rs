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
