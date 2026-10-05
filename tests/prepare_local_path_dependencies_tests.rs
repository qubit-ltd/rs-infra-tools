// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::fs;
use std::process::Command;

use tempfile::tempdir;

#[test]
fn local_path_dependencies_can_be_pinned_to_a_commit() {
    let directory = tempdir().expect("temporary directory");
    let remote = directory.path().join("dependency.git");
    let source = directory.path().join("source");
    let project = directory.path().join("project");

    git(
        directory.path(),
        ["init", "--bare", remote.to_str().unwrap()],
    );
    git(directory.path(), ["init", source.to_str().unwrap()]);
    git(&source, ["config", "user.name", "CI test"]);
    git(&source, ["config", "user.email", "ci@example.invalid"]);
    fs::write(
        source.join("Cargo.toml"),
        "[package]\nname='dependency'\nversion='0.1.0'\nedition='2024'\n",
    )
    .expect("dependency manifest");
    git(&source, ["add", "Cargo.toml"]);
    git(&source, ["commit", "-m", "initial dependency commit"]);
    git(
        &source,
        ["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&source, ["push", "origin", "HEAD:refs/heads/main"]);
    let revision = git_output(&source, ["rev-parse", "HEAD"]);

    fs::create_dir_all(project.join(".infra/ci")).expect("project CI directory");
    fs::write(
        project.join(".infra/ci/local-path-dependencies.tsv"),
        format!("../dependency\t{}\t{revision}\n", remote.display()),
    )
    .expect("local dependency configuration");

    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".infra/lib/prepare-local-path-dependencies.sh");
    let output = Command::new("bash")
        .arg(script)
        .env("RS_INFRA_PROJECT_ROOT", &project)
        .env("RS_INFRA_SHARED_ROOT", env!("CARGO_MANIFEST_DIR"))
        .env("RS_INFRA_CLEANUP_OWNER_PID", "test")
        .env("RS_INFRA_NETWORK_MAX_ATTEMPTS", "1")
        .output()
        .expect("run local dependency preparation");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let checkout = directory.path().join("dependency");
    assert_eq!(git_output(&checkout, ["rev-parse", "HEAD"]), revision);
}

fn git<const N: usize>(directory: &std::path::Path, args: [&str; N]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output<const N: usize>(directory: &std::path::Path, args: [&str; N]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
