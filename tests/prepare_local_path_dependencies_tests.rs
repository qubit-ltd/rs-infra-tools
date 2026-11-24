// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

#[test]
fn test_local_path_dependencies_are_pinned_and_strictly_checked() {
    let directory = tempdir().expect("temporary directory");
    let project = directory.path().join("project");
    let bin = directory.path().join("bin");
    fs::create_dir_all(project.join(".infra/ci")).expect("project CI directory");
    fs::create_dir_all(&bin).expect("mock command directory");
    write_config(&project, "../dependency", "/mock/dependency.git", REVISION);
    install_mock_git(&bin);

    let script = helper_script();
    let output = run_helper(&script, &project, &bin, REVISION, &[], &[]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let checkout = directory.path().join("dependency");
    assert_eq!(
        fs::read_to_string(checkout.join("fixture.txt")).unwrap(),
        "mock source\n"
    );

    let locked = run_helper(&script, &project, &bin, REVISION, &["--check-locked"], &[]);
    assert!(locked.status.success(), "{}", String::from_utf8_lossy(&locked.stderr));

    fs::write(checkout.join("untracked.txt"), "keep existing checkout\n").expect("dirty marker");
    let dirty = run_helper(&script, &project, &bin, REVISION, &["--check-locked"], &[]);
    assert!(!dirty.status.success());
    assert!(String::from_utf8_lossy(&dirty.stderr).contains("dirty at"));
    fs::remove_file(checkout.join("untracked.txt")).expect("remove dirty marker");

    let mismatched_revision = "0".repeat(40);
    write_config(&project, "../dependency", "/mock/dependency.git", &mismatched_revision);
    let reuse = run_helper(&script, &project, &bin, REVISION, &[], &[]);
    assert!(reuse.status.success(), "{}", String::from_utf8_lossy(&reuse.stderr));
    assert_eq!(
        fs::read_to_string(checkout.join("fixture.txt")).unwrap(),
        "mock source\n"
    );
    let mismatch = run_helper(&script, &project, &bin, REVISION, &["--check-locked"], &[]);
    assert!(!mismatch.status.success());
    assert!(String::from_utf8_lossy(&mismatch.stderr).contains("locked dependency '../dependency' HEAD mismatch"));
}

#[test]
fn test_fetch_failure_cleans_staging_and_a_later_run_succeeds() {
    let directory = tempdir().expect("temporary directory");
    let project = directory.path().join("project");
    let bin = directory.path().join("bin");
    let fail_once = directory.path().join("fetch-failed-once");
    fs::create_dir_all(project.join(".infra/ci")).expect("project CI directory");
    fs::create_dir_all(&bin).expect("mock command directory");
    write_config(&project, "../dependency", "/mock/dependency.git", REVISION);
    install_mock_git(&bin);

    let script = helper_script();
    let first = run_helper(
        &script,
        &project,
        &bin,
        REVISION,
        &[],
        &[("MOCK_GIT_FAIL_FETCH_ONCE", fail_once.as_os_str())],
    );
    assert!(!first.status.success());
    assert!(String::from_utf8_lossy(&first.stderr).contains("simulated fetch failure"));
    assert!(!directory.path().join("dependency").exists());
    assert_no_staging_artifacts(directory.path(), "dependency");

    let retry = run_helper(&script, &project, &bin, REVISION, &[], &[]);
    assert!(retry.status.success(), "{}", String::from_utf8_lossy(&retry.stderr));
    assert_eq!(
        fs::read_to_string(directory.path().join("dependency/fixture.txt")).unwrap(),
        "mock source\n"
    );
}

#[test]
fn test_strict_locked_check_rejects_a_missing_dependency_list() {
    let directory = tempdir().expect("temporary directory");
    let project = directory.path().join("project");
    let bin = directory.path().join("bin");
    fs::create_dir_all(&project).expect("project directory");
    fs::create_dir_all(&bin).expect("mock command directory");
    install_mock_git(&bin);

    let output = run_helper(&helper_script(), &project, &bin, REVISION, &["--check-locked"], &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("locked dependency list is missing"));
}

#[test]
fn test_destination_created_at_publish_time_is_preserved() {
    let directory = tempdir().expect("temporary directory");
    let project = directory.path().join("project");
    let bin = directory.path().join("bin");
    fs::create_dir_all(project.join(".infra/ci")).expect("project CI directory");
    fs::create_dir_all(&bin).expect("mock command directory");
    write_config(&project, "../dependency", "/mock/dependency.git", REVISION);
    install_mock_git(&bin);
    install_racing_mv(&bin);

    let script = helper_script();
    let output = run_helper(&script, &project, &bin, REVISION, &[], &[]);
    assert!(
        !output.status.success(),
        "unexpected successful publish: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("appeared during publication; preserving it"));

    let target = directory.path().join("dependency");
    assert_eq!(fs::read_to_string(target.join("marker")).unwrap(), "keep me\n");
    let target_entries = fs::read_dir(&target)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(target_entries, [std::ffi::OsString::from("marker")]);
    assert_no_staging_artifacts(directory.path(), "dependency");
}

#[test]
fn test_nested_plain_directory_is_not_accepted_as_a_dependency_checkout() {
    let directory = tempdir().expect("temporary directory");
    let project = directory.path().join("project");
    let parent_repo = directory.path().join("parent-repo");
    let nested = parent_repo.join("dependency");
    let bin = directory.path().join("bin");
    fs::create_dir_all(project.join(".infra/ci")).expect("project CI directory");
    fs::create_dir_all(parent_repo.join(".git")).expect("parent repo marker");
    fs::create_dir_all(&nested).expect("nested ordinary directory");
    fs::create_dir_all(&bin).expect("mock command directory");
    write_config(&project, "../parent-repo/dependency", "/mock/dependency.git", REVISION);
    install_mock_git(&bin);

    let parent_head = Command::new("git")
        .args(["-C", nested.to_str().unwrap(), "rev-parse", "HEAD"])
        .env("PATH", prepend_path(&bin))
        .env("MOCK_GIT_HEAD", REVISION)
        .output()
        .expect("query mock parent repository HEAD");
    assert!(parent_head.status.success(), "fixture must expose parent HEAD");
    assert_eq!(String::from_utf8_lossy(&parent_head.stdout).trim(), REVISION);
    let parent_root = Command::new("git")
        .args(["-C", nested.to_str().unwrap(), "rev-parse", "--show-toplevel"])
        .env("PATH", prepend_path(&bin))
        .env("MOCK_GIT_HEAD", REVISION)
        .output()
        .expect("query mock parent repository root");
    assert!(parent_root.status.success());
    assert_eq!(
        String::from_utf8_lossy(&parent_root.stdout).trim(),
        parent_repo.to_str().unwrap()
    );

    let script = helper_script();
    let prepare = run_helper(&script, &project, &bin, REVISION, &[], &[]);
    assert!(!prepare.status.success());
    assert!(String::from_utf8_lossy(&prepare.stderr).contains("refusing to replace existing dependency path"));
    assert!(!nested.join("fixture.txt").exists());

    let locked = run_helper(&script, &project, &bin, REVISION, &["--check-locked"], &[]);
    assert!(!locked.status.success());
    assert!(String::from_utf8_lossy(&locked.stderr).contains("is not a Git checkout root"));
}

#[test]
fn test_linked_worktree_git_pointer_is_accepted() {
    let directory = tempdir().expect("temporary directory");
    let project = directory.path().join("project");
    let checkout = directory.path().join("dependency");
    let bin = directory.path().join("bin");
    fs::create_dir_all(project.join(".infra/ci")).expect("project CI directory");
    fs::create_dir_all(&checkout).expect("linked checkout");
    fs::write(checkout.join(".git"), "gitdir: /mock/worktrees/dependency\n").expect("git pointer");
    fs::write(checkout.join("fixture.txt"), "linked source\n").expect("fixture");
    fs::create_dir_all(&bin).expect("mock command directory");
    write_config(&project, "../dependency", "/mock/dependency.git", REVISION);
    install_mock_git(&bin);

    let output = run_helper(&helper_script(), &project, &bin, REVISION, &["--check-locked"], &[]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_two_helpers_racing_to_publish_have_one_winner() {
    let directory = tempdir().expect("temporary directory");
    let project = directory.path().join("project");
    let bin = directory.path().join("bin");
    let barrier = directory.path().join("fetch-barrier");
    fs::create_dir_all(project.join(".infra/ci")).expect("project CI directory");
    fs::create_dir_all(&bin).expect("mock command directory");
    fs::create_dir_all(&barrier).expect("fetch barrier");
    write_config(&project, "../dependency", "/mock/dependency.git", REVISION);
    install_mock_git(&bin);

    let script = helper_script();
    let children = (0..2)
        .map(|_| {
            let mut command = Command::new("bash");
            command
                .arg(&script)
                .env("PATH", prepend_path(&bin))
                .env("MOCK_GIT_HEAD", REVISION)
                .env("MOCK_GIT_BARRIER_DIR", &barrier)
                .env("RS_INFRA_PROJECT_ROOT", &project)
                .env("RS_INFRA_SHARED_ROOT", env!("CARGO_MANIFEST_DIR"))
                .env("RS_INFRA_CLEANUP_OWNER_PID", "test")
                .env("RS_INFRA_NETWORK_MAX_ATTEMPTS", "1")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("spawn dependency preparation helper")
        })
        .collect::<Vec<_>>();
    let outputs = children
        .into_iter()
        .map(|child| child.wait_with_output().expect("wait for helper"))
        .collect::<Vec<_>>();
    assert_eq!(
        fs::read_dir(&barrier).unwrap().count(),
        2,
        "both helpers reached the synchronized fetch"
    );
    assert_eq!(outputs.iter().filter(|output| output.status.success()).count(), 1);
    assert_eq!(outputs.iter().filter(|output| !output.status.success()).count(), 1);
    let failed = outputs.iter().find(|output| !output.status.success()).unwrap();
    let error = String::from_utf8_lossy(&failed.stderr);
    assert!(
        error.contains("another dependency preparation is publishing")
            || error.contains("refusing to replace existing dependency path"),
        "unexpected losing helper error (status {}): stdout={} stderr={}",
        failed.status,
        String::from_utf8_lossy(&failed.stdout),
        error
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("dependency/fixture.txt")).unwrap(),
        "mock source\n"
    );
    assert_no_staging_artifacts(directory.path(), "dependency");
}

fn helper_script() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(".infra/lib/prepare-local-path-dependencies.sh")
}

fn write_config(project: &Path, relative_path: &str, source: &str, revision: &str) {
    fs::write(
        project.join(".infra/ci/local-path-dependencies.tsv"),
        format!("{relative_path}\t{source}\t{revision}\n"),
    )
    .expect("local dependency configuration");
}

fn run_helper(
    script: &Path,
    project: &Path,
    bin: &Path,
    head: &str,
    args: &[&str],
    extra_env: &[(&str, &std::ffi::OsStr)],
) -> std::process::Output {
    let mut command = Command::new("bash");
    command
        .arg(script)
        .args(args)
        .env("PATH", prepend_path(bin))
        .env("MOCK_GIT_HEAD", head)
        .env("RS_INFRA_PROJECT_ROOT", project)
        .env("RS_INFRA_SHARED_ROOT", env!("CARGO_MANIFEST_DIR"))
        .env("RS_INFRA_CLEANUP_OWNER_PID", "test")
        .env("RS_INFRA_NETWORK_MAX_ATTEMPTS", "1");
    for (name, value) in extra_env {
        command.env(name, value);
    }
    command.output().expect("run dependency preparation helper")
}

fn prepend_path(bin: &Path) -> std::ffi::OsString {
    let mut paths = vec![bin.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).expect("join test command search path")
}

fn install_mock_git(bin: &Path) {
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$*" == *" fetch "* && -n "${MOCK_GIT_FAIL_FETCH_ONCE:-}" && ! -e "$MOCK_GIT_FAIL_FETCH_ONCE" ]]; then
    touch "$MOCK_GIT_FAIL_FETCH_ONCE"
    echo 'simulated fetch failure' >&2
    exit 31
fi
if [[ "$*" == *" fetch "* && -n "${MOCK_GIT_BARRIER_DIR:-}" ]]; then
    checkout=
    previous=
    for argument in "$@"; do
        if [[ "$previous" == -C ]]; then checkout=$argument; fi
        previous=$argument
    done
    touch "$MOCK_GIT_BARRIER_DIR/${checkout##*/}"
    while :; do
        count=0
        for marker in "$MOCK_GIT_BARRIER_DIR"/*; do [[ -e "$marker" ]] && ((count += 1)); done
        (( count >= 2 )) && break
        sleep 0.01
    done
fi
if [[ "$1" == init ]]; then
    mkdir -p "$2"
    mkdir -p "$2/.git"
    printf 'mock source\n' > "$2/fixture.txt"
    exit 0
fi
if [[ "$1" == -C ]]; then
    checkout=$2
    shift 2
    case "$1" in
        remote|checkout) exit 0 ;;
        rev-parse)
            if [[ "${2:-}" == --show-toplevel ]]; then
                cursor=$checkout
                while [[ "$cursor" != / && ! -e "$cursor/.git" ]]; do cursor=$(dirname "$cursor"); done
                [[ -e "$cursor/.git" ]] || exit 128
                printf '%s\n' "$cursor"
                exit 0
            fi
            cursor=$checkout
            while [[ "$cursor" != / && ! -e "$cursor/.git" ]]; do cursor=$(dirname "$cursor"); done
            [[ -e "$cursor/.git" ]] || exit 128
            printf '%s\n' "$MOCK_GIT_HEAD"
            exit 0
            ;;
        status)
            if [[ -e "$checkout/untracked.txt" ]]; then
                printf '?? untracked.txt\n'
            fi
            exit 0
            ;;
    esac
fi
exit 0
"#;
    write_executable(&bin.join("git"), script);
}

fn install_racing_mv(bin: &Path) {
    let real_mv = Command::new("which").arg("mv").output().expect("locate mv");
    assert!(real_mv.status.success());
    let real_mv = String::from_utf8(real_mv.stdout).unwrap().trim().to_owned();
    let script = format!(
        "#!/usr/bin/env bash\nif [[ \"$1\" == --help ]]; then echo 'usage: mv -T -n SOURCE DEST'; exit 0; fi\nif [[ \"$*\" == *\"-Tn \"* ]]; then mkdir \"$3\"; printf 'keep me\\n' > \"$3/marker\"; fi\nexec '{}' \"$@\"\n",
        real_mv
    );
    write_executable(&bin.join("mv"), &script);
}

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).expect("write command mock");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("make command mock executable");
}

fn assert_no_staging_artifacts(parent: &Path, name: &str) {
    let staging_prefix = format!(".{name}.prepare.");
    let lock_name = format!(".{name}.prepare-publish.lock");
    for entry in fs::read_dir(parent).expect("read fixture parent") {
        let entry = entry.expect("read fixture entry");
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        assert!(
            !file_name.starts_with(&staging_prefix),
            "staging directory leaked: {file_name}"
        );
        assert_ne!(file_name, lock_name, "publish lock leaked");
    }
}
