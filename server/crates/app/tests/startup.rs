//! Runs the server binary to check what an operator sees when startup
//! fails: the reason on stderr and exit status 1, whatever `RUST_LOG`.

use std::fs;
use std::process::{Command, Output};

fn run(dir: &std::path::Path, vars: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_unified-mailbox"))
        .current_dir(dir)
        .env_clear()
        .envs(vars.iter().copied())
        .output()
        .expect("run the server")
}

#[test]
fn invalid_variables_listed() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        dir.path(),
        &[("DRY_RUN", "maybe"), ("SPAM_THRESHOLD", "101")],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr.contains("DRY_RUN"), "{stderr}");
    assert!(stderr.contains("SPAM_THRESHOLD"), "{stderr}");
}

#[test]
fn fatal_error_shown_when_logs_filter_it_out() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("afile"), "").unwrap();
    let out = run(
        dir.path(),
        &[("DATA_DIR", "./afile"), ("RUST_LOG", "sqlite=debug")],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr.contains("DATA_DIR"), "{stderr}");
}

#[test]
fn first_start_creates_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(dir.path(), &[("DATA_DIR", "./d"), ("RUST_LOG", "off")]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.path().join("d/unified-mailbox.db").is_file());
    assert!(dir.path().join("d/secret.key").is_file());
}
