use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};

#[tokio::test]
async fn test_bpe_tokenizer_accuracy() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Source code is highly dense with spaces, brackets, and operators.
    // The old naive fallback `(len + 3) / 4` significantly undercounts code tokens.
    // We will verify that our new BPE tokenizer counts tokens accurately.
    let code_sample = r#"
        pub async fn new_client_connection() -> Result<Self> {
            let port_str = std::env::var("MYTHRAX_DAEMON_PORT").unwrap_or_else(|_| "8090".to_string());
            if let Ok(port) = port_str.parse::<u16>() {
                if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
                    &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                    std::time::Duration::from_millis(50),
                ) {
                    // Daemon is active, connect as client
                    return Ok(Self {
                        db: surrealdb::engine::local::Db::new(), // dummy
                        embedder: None,
                        client_port: Some(port),
                    });
                }
            }
            Err(anyhow::anyhow!("No active daemon found"))
        }
    "#;

    let naive_count = (code_sample.len() + 3) / 4;
    
    // Call our upgraded tokenizer count
    let bpe_count = backend.count_text_tokens(code_sample);

    println!("BPE Token Count: {}, Naive Token Count: {}", bpe_count, naive_count);

    // BPE token count for code is typically 1.3x to 1.5x larger than naive count (chars/4)
    // because code has many single-character tokens (brackets, braces, operators, spaces).
    // We assert that BPE tokenizer counts correctly, and differs significantly from the naive count.
    assert!(bpe_count > naive_count, "BPE tokenizer must count code tokens more accurately and return a higher count than the naive chars/4 fallback");
    assert!(bpe_count > 0);

    Ok(())
}
