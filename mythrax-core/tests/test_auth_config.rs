use mythrax_core::auth::{load_token, verify_token_constant_time};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_permissions_enforced() {
    let dir = tempdir().unwrap();
    let token_path = dir.path().join("token");

    // 1. Create file and write token
    {
        let mut file = File::create(&token_path).unwrap();
        file.write_all(b"my-secure-token\n").unwrap();
    }

    // Set permission to 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let loaded = load_token(&token_path).unwrap();
        assert_eq!(loaded, "my-secure-token");

        // Set permission to 0644 (wider)
        std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let result = load_token(&token_path);
        assert!(
            result.is_err(),
            "Expected error for wider permissions (0644)"
        );
    }
}

#[test]
fn test_constant_time_token_check() {
    assert!(verify_token_constant_time("secure-token", "secure-token"));
    assert!(!verify_token_constant_time("secure-token", "wrong-token"));
    assert!(!verify_token_constant_time(
        "secure-token",
        "longer-secure-token"
    ));
    assert!(!verify_token_constant_time(
        "longer-secure-token",
        "secure-token"
    ));
}

#[test]
fn test_no_secret_token_fallback() {
    let dir = tempdir().unwrap();
    let token_path = dir.path().join("non_existent_token_file");

    let result = load_token(&token_path);
    assert!(
        result.is_err(),
        "Expected error when token file does not exist"
    );
}
