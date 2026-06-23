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
    c
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
}

#[test]
fn test_forge_missing_file_exits_nonzero() {
    let tmp = tempdir().expect("temp dir");
    let out = cmd(tmp.path())
        .args(["forge", "/tmp/does_not_exist_xyz_mythrax_e2e.md"])
        .output()
        .expect("spawn forge");
    assert!(
        !out.status.success(),
        "forge on a missing file should exit non-zero, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
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
        .args(["forge", source.to_str().unwrap(), "--scope", "e2e_test"])
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
        stdout.contains("Forge ingestion complete"),
        "Expected 'Forge ingestion complete' in stdout, got: {}",
        stdout
    );
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
        .args(["forge", pdf_path.to_str().unwrap(), "--scope", "e2e_test"])
        .output()
        .expect("spawn forge pdf");

    assert!(
        out.status.success(),
        "forge on a valid PDF should exit 0.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
