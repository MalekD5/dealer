use std::fs;
use std::process::{Command, Output};

use tempfile::TempDir;

fn project_with_script(script: &str) -> TempDir {
    let directory = tempfile::tempdir().expect("temporary project directory should be created");
    let manifest = serde_json::json!({
        "name": "cli-test-project",
        "version": "1.2.3",
        "scripts": { "test-script": script }
    });

    fs::write(
        directory.path().join("package.json"),
        serde_json::to_vec(&manifest).expect("test manifest should serialize"),
    )
    .expect("test manifest should be written");

    directory
}

fn run_dealer(project: &TempDir, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dealer"))
        .current_dir(project.path())
        .args(arguments)
        .output()
        .expect("dealer CLI should start")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn run_executes_a_manifest_script() {
    let project = project_with_script("echo dealer-cli-test");
    let output = run_dealer(&project, &["run", "test-script"]);
    let stdout = text(&output.stdout);

    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert!(stdout.contains("> cli-test-project@1.2.3 test-script"));
    assert!(stdout.contains("> echo dealer-cli-test"));
    assert!(stdout.contains("dealer-cli-test"));
}

#[test]
fn run_reports_an_unknown_script() {
    let project = project_with_script("echo unused");
    let output = run_dealer(&project, &["run", "missing"]);
    let stderr = text(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("script `missing` was not found"),
        "{stderr}"
    );
}

#[test]
fn run_propagates_a_script_failure() {
    let project = project_with_script("exit 7");
    let output = run_dealer(&project, &["run", "test-script"]);
    let stderr = text(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("script `test-script` exited with") && stderr.contains('7'),
        "{stderr}"
    );
}

#[test]
fn run_requires_a_script_name() {
    let project = project_with_script("echo unused");
    let output = run_dealer(&project, &["run"]);
    let stderr = text(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr.contains("required arguments were not provided"),
        "{stderr}"
    );
    assert!(stderr.contains("<SCRIPT_NAME>"), "{stderr}");
}
