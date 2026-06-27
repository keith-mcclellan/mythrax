use anyhow::Result;
use mythrax_core::db::SurrealBackend;
use std::net::TcpListener;

#[tokio::test]
async fn test_client_server_auto_routing_detection() -> Result<()> {
    // 1. Find a free port by binding a listener and dropping it.
    let free_listener = TcpListener::bind("127.0.0.1:0")?;
    let free_port = free_listener.local_addr()?.port();
    drop(free_listener);

    unsafe {
        std::env::set_var("MYTHRAX_DAEMON_PORT", free_port.to_string());
    }

    // 2. Initialize backend in embedded mode.
    // Since the port is free (no daemon listening), it should default to embedded mode.
    let backend_embedded = SurrealBackend::new_in_memory().await?;
    assert!(!backend_embedded.is_client_mode(), "Backend must default to embedded mode when no daemon is running");

    // 3. Now, we start a mock daemon on the same port to trigger client mode detection.
    let _mock_daemon = TcpListener::bind(format!("127.0.0.1:{}", free_port))?;
    
    // Re-initialize backend. It should now detect the active port and switch to client mode.
    let backend_client = SurrealBackend::new_client_connection().await;
    
    // Clean up env var immediately
    unsafe {
        std::env::remove_var("MYTHRAX_DAEMON_PORT");
    }

    let backend = backend_client?;
    assert!(backend.is_client_mode(), "Backend must switch to Client Mode when the daemon port is active");

    Ok(())
}
