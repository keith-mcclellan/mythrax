use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::mcp_routes::truncate_summary;
use mythrax_core::secret_filter::SecretFilter;

static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_utf8_boundary_truncation() {
    // 200 characters of Chinese, each character is 3 bytes (total 600 bytes).
    // Let's create a string with 205 Chinese characters, so slicing at 200 bytes would fall in the middle of a character.
    let chinese_char = "中";
    let input = chinese_char.repeat(205);
    
    // Call truncate_summary
    let truncated = truncate_summary(&input);
    
    // It should not panic, and since it is > 200 chars, it should be truncated to exactly 200 chars plus "..."
    // Let's count characters in the truncated string
    let char_count = truncated.chars().count();
    // 200 characters plus 3 characters for "..." = 203 characters
    assert_eq!(char_count, 203);
    assert!(truncated.ends_with("..."));
}

#[test]
fn test_secret_filter_no_panic_on_mismatch() {
    // 1. Unmatched quotes
    let unmatched = "password = \"secret";
    let cleaned_unmatched = SecretFilter::clean(unmatched);
    assert_eq!(cleaned_unmatched, "password = \"secret");

    // 2. Secret with multi-byte characters
    let multibyte = "password = \"🔑secret\"";
    let cleaned_multibyte = SecretFilter::clean(multibyte);
    assert!(cleaned_multibyte.contains("[REDACTED]"));
    assert!(!cleaned_multibyte.contains("🔑secret"));
}

#[tokio::test]
async fn test_embed_batch_error_propagation() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    // Temporarily clear MYTHRAX_TEST_MOCK if it's set, to force an error (no embedder loaded)
    let original_mock = std::env::var("MYTHRAX_TEST_MOCK");
    unsafe {
        std::env::remove_var("MYTHRAX_TEST_MOCK");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let result = backend.embed_batch(&["test".to_string()]).await;

    // Restore MYTHRAX_TEST_MOCK
    if let Ok(ref val) = original_mock {
        unsafe {
            std::env::set_var("MYTHRAX_TEST_MOCK", val);
        }
    }

    assert!(result.is_err());
    let err_msg = format!("{:?}", result.err().unwrap());
    assert!(err_msg.contains("No embedding model loaded"));

    Ok(())
}
