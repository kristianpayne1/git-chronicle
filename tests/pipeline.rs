use assert_cmd::Command;
use git2::{Repository, Signature};
use tempfile::TempDir;

// ── test repo helper ────────────────────────────────────────────────────────

/// Create a temporary git repository with two commits and return the TempDir
/// (keeps the directory alive for the duration of the test).
fn make_test_repo() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    let repo = Repository::init(dir.path()).expect("git init");
    let sig = Signature::now("Test Author", "test@example.com").expect("sig");

    // First commit
    std::fs::write(dir.path().join("README.md"), b"# Test repo\n").expect("write");
    let tree_oid = {
        let mut index = repo.index().expect("index");
        index.add_path(std::path::Path::new("README.md")).expect("add");
        index.write().expect("write index");
        index.write_tree().expect("write tree")
    };
    let tree = repo.find_tree(tree_oid).expect("find tree");
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .expect("first commit");

    // Second commit
    std::fs::write(dir.path().join("main.rs"), b"fn main() {}\n").expect("write");
    let tree_oid = {
        let mut index = repo.index().expect("index");
        index.add_path(std::path::Path::new("main.rs")).expect("add");
        index.write().expect("write index");
        index.write_tree().expect("write tree")
    };
    let tree = repo.find_tree(tree_oid).expect("find tree");
    let parent = repo.head().expect("head").peel_to_commit().expect("peel");
    repo.commit(Some("HEAD"), &sig, &sig, "Add main.rs", &tree, &[&parent])
        .expect("second commit");

    dir
}

// ── tests ───────────────────────────────────────────────────────────────────

/// Run the full pipeline against a real git repo with a mock Ollama server.
/// `#[tokio::test(flavor = "multi_thread")]` ensures the mockito server can
/// accept connections while `Command::output()` blocks a worker thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pipeline_success() {
    let dir = make_test_repo();

    let mut server = mockito::Server::new_async().await;
    let endpoint = server.url();

    let _mock = server
        .mock("POST", "/api/generate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"response":"This is the test narrative.","done":true}"#)
        .create_async()
        .await;

    let output = Command::cargo_bin("git-chronicle")
        .expect("binary")
        .args(["--backend", "ollama", "--model", "test-model", "--no-diffs"])
        .arg(dir.path())
        .env("CHRONICLE_ENDPOINT", &endpoint)
        .output()
        .expect("run");

    assert!(
        output.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("This is the test narrative."),
        "stdout did not contain narrative: {stdout}"
    );
}

/// A path with no .git directory should exit 1 with an error message.
#[test]
fn pipeline_error_bad_path() {
    let dir = TempDir::new().expect("tempdir");

    let output = Command::cargo_bin("git-chronicle")
        .expect("binary")
        .args(["--backend", "ollama", "--model", "test-model"])
        .arg(dir.path())
        .output()
        .expect("run");

    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1; stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("git-chronicle:"),
        "stderr should contain error prefix: {stderr}"
    );
}

/// `--output` should produce a file with one valid JSON object per line.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pipeline_writes_audit_file() {
    let dir = make_test_repo();
    let audit_path = dir.path().join("audit.jsonl");

    let mut server = mockito::Server::new_async().await;
    let endpoint = server.url();

    let _mock = server
        .mock("POST", "/api/generate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"response":"narrative text","done":true}"#)
        .create_async()
        .await;

    let output = Command::cargo_bin("git-chronicle")
        .expect("binary")
        .args(["--backend", "ollama", "--model", "test-model", "--no-diffs"])
        .arg(dir.path())
        .arg("--output")
        .arg(&audit_path)
        .env("CHRONICLE_ENDPOINT", &endpoint)
        .output()
        .expect("run");

    assert!(
        output.status.success(),
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = std::fs::read_to_string(&audit_path).expect("read audit file");
    assert!(!content.is_empty(), "audit file should not be empty");

    for line in content.lines() {
        let v: serde_json::Value =
            serde_json::from_str(line).expect("each line must be valid JSON");
        assert!(v.get("summary").is_some(), "expected 'summary' field: {line}");
        assert!(v.get("pass").is_some(), "expected 'pass' field: {line}");
        assert!(v.get("model").is_some(), "expected 'model' field: {line}");
    }
}
