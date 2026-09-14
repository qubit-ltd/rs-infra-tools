use std::fs;

use qubit_infra_tools::lock::LockFile;

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn lock_file_rejects_wrong_field_count() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tools.lock");
    fs::write(&path, "rs-infra-tools file:///tool only-three-fields").unwrap();
    assert!(LockFile::read(&path).is_err());
}

#[test]
fn lock_file_accepts_the_public_contract() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tools.lock");
    fs::write(
        &path,
        format!("rs-infra-tools file:///tool {REVISION} x86_64-unknown-linux-gnu {SHA}"),
    )
    .unwrap();
    assert!(LockFile::read(&path).is_ok());
}
