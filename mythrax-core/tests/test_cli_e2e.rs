/// E2E CLI tests: spawn the compiled `mythrax` binary and assert on exit codes and output.
/// Run with: `cargo test --test test_cli_e2e -- --test-threads=1`
///
/// These tests spawn the real binary, which uses mem:// SurrealDB (no config file)
/// and MYTHRAX_MOCK_LLM=true to skip actual LLM calls.
/// Tests MUST run serially (test-threads=1) since multiple tests write to ~/mythrax-vault.
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Get the compiled binary path via the CARGO_BIN_EXE_ env macro.
fn binary() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_mythrax"))
}

/// Build a Command with MYTHRAX_MOCK_LLM set so forge doesn't hang on real LLM calls,
/// and override HOME to a temp directory so it uses mem:// instead of the system's locked RocksDB.
fn cmd(home: &std::path::Path) -> Command {
    let mut c = Command::new(binary());
    c.env("MYTHRAX_MOCK_LLM", "true");
    c.env("HOME", home);
    c.env("MYTHRAX_DAEMON_PORT", "18092");
    c
}

/// Helper to clean up a daemon process by sending SIGINT and waiting for exit.
/// This ensures that PID files are cleaned up and ports are released between tests.
fn cleanup_daemon(home: &std::path::Path) {
    let pid_file = home.join(".mythrax/daemon.pid");
    if pid_file.exists() {
        if let Ok(pid_content) = fs::read_to_string(&pid_file) {
            let pid = pid_content.trim();
            if !pid.is_empty() {
                let _ = Command::new("kill")
                    .args(["-2", pid])
                    .status();
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
        let _ = fs::remove_file(&pid_file);
    }
}

#[test]
fn test_help_exits_zero() {
    let tmp = tempdir().expect("temp dir");
    let out = Command::new(binary())
        .env("HOME", tmp.path())
        .arg("--help")
        .output()
        .expect("spawn --help");
    assert!(
        out.status.success(),
        "--help should exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    cleanup_daemon(tmp.path());
}

#[test]
fn test_forge_missing_file_exits_nonzero() {
    let tmp = tempdir().expect("temp dir");
    let out = cmd(tmp.path())
        .args(["ingest", "forge", "/tmp/does_not_exist_xyz_mythrax_e2e.md"])
        .output()
        .expect("spawn forge");
    assert!(
        !out.status.success(),
        "forge on a missing file should exit non-zero, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    cleanup_daemon(tmp.path());
}

#[test]
fn test_forge_text_file_exits_zero() {
    let tmp = tempdir().expect("temp dir");
    let source = tmp.path().join("doc.md");
    fs::write(
        &source,
        "# System Design\n\nAlways prefer composition over inheritance. \
         Use dependency injection to decouple components.",
    )
    .expect("write doc");

    let out = cmd(tmp.path())
        .args(["ingest", "forge", source.to_str().unwrap(), "--scope", "e2e_test"])
        .output()
        .expect("spawn forge");

    assert!(
        out.status.success(),
        "forge on valid text file should exit 0.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Successfully forged source document") || stdout.contains("Forge ingestion complete"),
        "Expected 'Successfully forged source document' or 'Forge ingestion complete' in stdout, got: {}",
        stdout
    );
    cleanup_daemon(tmp.path());
}

#[test]
fn test_forge_pdf_exits_zero() {
    use lopdf::{Dictionary, Document, Object, Stream};

    let tmp = tempdir().expect("temp dir");
    let pdf_path = tmp.path().join("test.pdf");

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let content_id = doc.new_object_id();

    let content = b"BT /F1 12 Tf 72 712 Td (Forge PDF E2E test content.) Tj ET";
    doc.objects
        .insert(content_id, Object::Stream(Stream::new(Dictionary::new(), content.to_vec())));

    let mut page_dict = Dictionary::new();
    page_dict.set("Type", "Page");
    page_dict.set("Parent", pages_id);
    page_dict.set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
    page_dict.set("Contents", content_id);
    let mut resources = Dictionary::new();
    let mut fonts = Dictionary::new();
    let mut font = Dictionary::new();
    font.set("Type", "Font");
    font.set("Subtype", "Type1");
    font.set("BaseFont", "Helvetica");
    fonts.set("F1", font);
    resources.set("Font", fonts);
    page_dict.set("Resources", resources);
    doc.objects.insert(page_id, Object::Dictionary(page_dict));

    let mut pages = Dictionary::new();
    pages.set("Type", "Pages");
    pages.set("Kids", vec![page_id.into()]);
    pages.set("Count", 1);
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let mut catalog = Dictionary::new();
    catalog.set("Type", "Catalog");
    catalog.set("Pages", pages_id);
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save pdf");
    fs::write(&pdf_path, buf).expect("write pdf");

    let out = cmd(tmp.path())
        .args(["ingest", "forge", pdf_path.to_str().unwrap(), "--scope", "e2e_test"])
        .output()
        .expect("spawn forge pdf");

    assert!(
        out.status.success(),
        "forge on a valid PDF should exit 0.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    cleanup_daemon(tmp.path());
}

#[test]
fn test_cli_daemon_run_and_cleanup() {
    let tmp = tempdir().expect("temp dir");
    let mut child = cmd(tmp.path())
        .args(["daemon", "run", "--port", "18091"])
        .spawn()
        .expect("spawn daemon run");

    // Poll for the PID file to be created (up to 10 seconds)
    let pid_file = tmp.path().join(".mythrax/daemon.pid");
    let mut found = false;
    for _ in 0..100 {
        if pid_file.exists() {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(found, "PID file should be created at {:?}", pid_file);

    // Wait for the TCP port to be open to ensure Axum is running and signals are handled
    let addr = "127.0.0.1:18091";
    let mut port_open = false;
    for _ in 0..100 {
        if std::net::TcpStream::connect(addr).is_ok() {
            port_open = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(port_open, "Daemon port 18091 should be listening");

    // Read the PID file and verify it contains the child's PID
    let pid_content = fs::read_to_string(&pid_file).expect("read PID file");
    assert_eq!(pid_content.trim(), child.id().to_string());

    // Send SIGINT (signal 2) to the child process
    let status = Command::new("kill")
        .args(["-2", &child.id().to_string()])
        .status()
        .expect("send SIGINT via kill");
    assert!(status.success(), "kill command should succeed");

    // Wait for the child process to exit
    let exit_status = child.wait().expect("wait for child");
    assert!(exit_status.success() || exit_status.code().is_none());

    // Check if the PID file has been deleted
    assert!(!pid_file.exists(), "PID file should be deleted on clean SIGINT exit");
    cleanup_daemon(tmp.path());
}

#[test]
fn test_cli_search_episodes_flag() {
    let tmp = tempdir().expect("temp dir");
    
    // Start daemon on port 18092
    let mut daemon = cmd(tmp.path())
        .args(["daemon", "run", "--port", "18092"])
        .spawn()
        .expect("spawn daemon");

    // Poll to let daemon boot and write the PID file
    let pid_file = tmp.path().join(".mythrax/daemon.pid");
    let mut found = false;
    for _ in 0..100 {
        if pid_file.exists() {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(found, "Daemon PID file should be created");

    // Wait for the TCP port to be open to ensure Axum is running and signals are handled
    let addr = "127.0.0.1:18092";
    let mut port_open = false;
    for _ in 0..100 {
        if std::net::TcpStream::connect(addr).is_ok() {
            port_open = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(port_open, "Daemon port 8090 should be listening");

    // Create a temporary document to save
    let doc_file = tmp.path().join("search_test_doc.md");
    fs::write(
        &doc_file,
        "# SpecialSearchQueryPattern\n\nThis is a specific test case content for e2e search.",
    )
    .expect("write doc");

    // Save episode via CLI
    let save_status = cmd(tmp.path())
        .args(["memory", "record", "search_test_doc", "--file", doc_file.to_str().unwrap(), "--scope", "e2e_search_test"])
        .status()
        .expect("spawn memory record");
    assert!(save_status.success(), "memory record command should succeed");

    // Perform default search (should exclude episodes)
    let search_default_out = cmd(tmp.path())
        .args(["memory", "query", "SpecialSearchQueryPattern", "--scope", "e2e_search_test"])
        .output()
        .expect("spawn memory query default");
    assert!(search_default_out.status.success());
    let default_stdout = String::from_utf8_lossy(&search_default_out.stdout);
    assert!(
        default_stdout.contains("[]"),
        "Default search should exclude episode, got stdout: {}",
        default_stdout
    );

    // Perform search with --episodes flag
    let search_episodes_out = cmd(tmp.path())
        .args(["memory", "query", "SpecialSearchQueryPattern", "--scope", "e2e_search_test", "--include-episodes"])
        .output()
        .expect("spawn search with --episodes");
    assert!(search_episodes_out.status.success());
    let episodes_stdout = String::from_utf8_lossy(&search_episodes_out.stdout);
    assert!(
        episodes_stdout.contains("SpecialSearchQueryPattern"),
        "Search with --episodes should include episode, got stdout: {}",
        episodes_stdout
    );

    // Stop daemon cleanly
    let status = Command::new("kill")
        .args(["-2", &daemon.id().to_string()])
        .status()
        .expect("kill daemon");
    assert!(status.success());
    let _ = daemon.wait();
    cleanup_daemon(tmp.path());
}
