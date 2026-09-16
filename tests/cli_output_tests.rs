use std::fs;
use std::process::Command;

use sha2::Digest;
use sha2::Sha256;

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const TARGET: &str = "x86_64-unknown-linux-gnu";

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
#[cfg(unix)]
fn ensure_reports_success_without_corrupting_path_output() {
    let directory = tempfile::tempdir().unwrap();
    let artifact = directory.path().join("tool");
    let bytes = b"#!/bin/sh\nexit 0\n";
    fs::write(&artifact, bytes).unwrap();
    let lock = directory.path().join("tools.lock");
    fs::write(
        &lock,
        format!(
            "tool file://{} {REVISION} {TARGET} {}\n",
            artifact.display(),
            digest(bytes)
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rs-infra-tools"))
        .args(["ensure", "--lock"])
        .arg(&lock)
        .args(["--tool", "tool"])
        .env("RS_INFRA_CACHE_DIR", directory.path().join("cache"))
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).trim().ends_with("/tool"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("ensure 'tool' succeeded"));
}

#[test]
#[cfg(unix)]
fn exec_reports_child_failure_and_preserves_exit_code() {
    let directory = tempfile::tempdir().unwrap();
    let artifact = directory.path().join("tool");
    let bytes = b"#!/bin/sh\nexit 7\n";
    fs::write(&artifact, bytes).unwrap();
    let lock = directory.path().join("tools.lock");
    fs::write(
        &lock,
        format!(
            "tool file://{} {REVISION} {TARGET} {}\n",
            artifact.display(),
            digest(bytes)
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rs-infra-tools"))
        .args(["exec", "--lock"])
        .arg(&lock)
        .args(["--tool", "tool"])
        .env("RS_INFRA_CACHE_DIR", directory.path().join("cache"))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&output.stderr).contains("exec 'tool' failed (exit code 7)"));
}

#[test]
fn invalid_lock_reports_failure_message() {
    let directory = tempfile::tempdir().unwrap();
    let lock = directory.path().join("missing.lock");
    let output = Command::new(env!("CARGO_BIN_EXE_rs-infra-tools"))
        .args(["ensure", "--lock"])
        .arg(lock)
        .args(["--tool", "tool"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("rs-infra-tools: failed:"));
}
