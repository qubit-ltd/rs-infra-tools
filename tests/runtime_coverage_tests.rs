// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Isolated CLI tests for runtime resolution and execution.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

const TOOLS: &[&str] = &[
    "rs-infra-ci",
    "rs-infra-coverage",
    "rs-infra-dependency",
    "rs-infra-pages",
    "rs-infra-platform",
    "rs-infra-style",
    "rs-infra-verify",
];

struct Fixture {
    root: tempfile::TempDir,
    revision: String,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let manager = root.path().join("manager");
        fs::create_dir_all(root.path().join("project")).unwrap();
        let fake_bin = root.path().join("fake-bin");
        fs::create_dir_all(manager.join(".infra/bin")).unwrap();
        fs::create_dir_all(manager.join(".git")).unwrap();
        fs::create_dir_all(&fake_bin).unwrap();
        fs::write(manager.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        let revision = "0123456789abcdef0123456789abcdef01234567".to_owned();
        write_executable(
            &fake_bin.join("git"),
            r###"#!/bin/sh
printf '%s\n' "$*" >> "$FIXTURE_GIT_LOG"
case " $* " in
  *" ls-remote "*)
    if [ "${FIXTURE_EMPTY_REVISION:-}" = yes ]; then :
    elif [ "${FIXTURE_NON_UTF8_REVISION:-}" = yes ]; then printf '\377\n'
    elif [ "${FIXTURE_BAD_REVISION:-}" = yes ]; then printf 'invalid refs/heads/main\n'
    else printf '%s refs/heads/main\n' "$FIXTURE_REVISION"; fi
    if [ "${FIXTURE_FAIL_LS_REMOTE:-}" = yes ]; then echo lookup-failed >&2; exit 1; fi
    if [ "${FIXTURE_RETRY_LS_REMOTE:-}" = yes ] && [ ! -f "$FIXTURE_RETRY_MARKER" ]; then touch "$FIXTURE_RETRY_MARKER"; echo transient >&2; exit 1; fi
    exit 0 ;;
  *" init "*)
    if [ "${FIXTURE_FAIL_INIT:-}" = yes ]; then echo init-failed >&2; exit 1; fi
    for argument in "$@"; do source_dir=$argument; done
    mkdir -p "$source_dir/.git"
    exit 0 ;;
  *" remote add "*) if [ "${FIXTURE_FAIL_REMOTE:-}" = yes ]; then echo remote-failed >&2; exit 1; fi; exit 0 ;;
  *" fetch "*)
    if [ "${FIXTURE_FAIL_GIT_FETCH:-}" = yes ]; then echo fetch-failed >&2; exit 1; fi
    exit 0 ;;
  *" checkout "*)
    if [ "${FIXTURE_FAIL_CHECKOUT:-}" = yes ]; then echo checkout-failed >&2; exit 1; fi
    source_dir=
    previous=
    for argument in "$@"; do if [ "$previous" = -C ]; then source_dir=$argument; fi; previous=$argument; done
    if [ "${FIXTURE_NO_MANIFEST:-}" != yes ]; then printf '[workspace]\nmembers = []\n' > "$source_dir/Cargo.toml"; fi
    exit 0 ;;
  *" rev-parse "*)
    source_dir=
    previous=
    show_toplevel=no
    for argument in "$@"; do
      if [ "$previous" = -C ]; then source_dir=$argument; fi
      if [ "$argument" = --show-toplevel ]; then show_toplevel=yes; fi
      previous=$argument
    done
    if [ "$show_toplevel" = yes ] && [ "${FIXTURE_FAIL_ROOT_LOOKUP:-}" = yes ]; then echo root-lookup-failed >&2; exit 1; fi
    if [ "$show_toplevel" != yes ] && [ "${FIXTURE_FAIL_HEAD:-}" = yes ]; then echo rev-parse-failed >&2; exit 1; fi
    if [ "$show_toplevel" = yes ]; then
      if [ "${FIXTURE_BAD_ROOT:-}" = yes ]; then printf '/wrong/root\n'; else printf '%s\n' "$source_dir"; fi
    elif [ "${FIXTURE_BAD_HEAD:-}" = yes ]; then printf 'ffffffffffffffffffffffffffffffffffffffff\n'
    else printf '%s\n' "$FIXTURE_REVISION"; fi
    exit 0 ;;
esac
exit 0
"###,
        );
        write_executable(
            &fake_bin.join("cargo"),
            r###"#!/bin/sh
printf '%s\n' "$*" >> "$FIXTURE_CARGO_LOG"
case " $* " in *" fetch "*) if [ "${FIXTURE_CARGO_FAIL_FETCH:-}" = yes ]; then echo cargo-fetch-failed >&2; exit 1; fi; exit 0 ;; esac
case " $* " in *" build "*) if [ "${FIXTURE_CARGO_FAIL_BUILD:-}" = yes ]; then echo cargo-build-failed >&2; exit 1; fi ;; esac
target_dir=
binary=
previous=
for argument in "$@"; do
  if [ "$previous" = --target-dir ]; then target_dir=$argument; fi
  if [ "$previous" = --bin ]; then binary=$argument; fi
  previous=$argument
done
mkdir -p "$target_dir/release"
if [ "${FIXTURE_CARGO_NO_BINARY:-}" = yes ]; then exit 0; fi
cat > "$target_dir/release/$binary" <<'TOOL'
#!/bin/sh
printf '%s\n' "$*" > "$FIXTURE_TOOL_LOG"
printf '%s\n' "$PATH" > "$FIXTURE_TOOL_PATH"
exit "${FIXTURE_TOOL_EXIT:-0}"
TOOL
chmod +x "$target_dir/release/$binary"
bin_dir=$(dirname "$target_dir")/bin
case "${FIXTURE_PUBLISH_FAILURE:-}" in
  copy) mkdir -p "$bin_dir/.$binary.tmp-$PPID" ;;
  rename) mkdir -p "$bin_dir/$binary" ;;
esac
"###,
        );
        Self { root, revision }
    }

    fn manager(&self) -> std::path::PathBuf {
        self.root.path().join("manager")
    }

    fn command(&self, args: &[&str]) -> Command {
        let fake_bin = self.root.path().join("fake-bin");
        let inherited_path = std::env::var_os("PATH").unwrap_or_default();
        let path =
            std::env::join_paths(std::iter::once(fake_bin).chain(std::env::split_paths(&inherited_path))).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_rs-infra-tools"));
        command
            .args(args)
            .env("PATH", path)
            .env("FIXTURE_REVISION", &self.revision)
            .env("FIXTURE_MANAGER", self.manager())
            .env("FIXTURE_GIT_LOG", self.root.path().join("git.log"))
            .env("FIXTURE_RETRY_MARKER", self.root.path().join("retry.once"))
            .env("FIXTURE_CARGO_LOG", self.root.path().join("cargo.log"))
            .env("FIXTURE_TOOL_LOG", self.root.path().join("tool.log"))
            .env("FIXTURE_TOOL_PATH", self.root.path().join("tool-path.log"))
            .env("RS_INFRA_CACHE_DIR", self.root.path().join("cache"));
        command
    }

    fn prewarm(&self) {
        let output = self
            .command(&["prewarm", "--project"])
            .arg(self.root.path().join("project"))
            .env("RS_INFRA_SHARED_ROOT", self.manager())
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }
}

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn host_target() -> String {
    let output = Command::new("rustc").arg("-vV").output().unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap()
        .to_owned()
}

fn rustc_release() -> String {
    let output = Command::new("rustc").arg("--version").output().unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .replace([' ', '/'], "-")
}

#[test]
fn test_prewarm_builds_each_tool_using_fake_git_and_cargo() {
    let fixture = Fixture::new();
    fixture.prewarm();
    let executable = fixture
        .root
        .path()
        .join("cache/rs-infra/tools/rs-infra-platform")
        .join(&fixture.revision)
        .join(host_target())
        .join(rustc_release())
        .join("bin/rs-infra-platform");
    assert!(executable.is_file(), "{}", executable.display());
    let git_log = fs::read_to_string(fixture.root.path().join("git.log")).unwrap();
    let cargo_log = fs::read_to_string(fixture.root.path().join("cargo.log")).unwrap();
    for tool in TOOLS {
        assert!(git_log.contains(&format!(
            "ls-remote https://github.com/qubit-ltd/{tool}.git refs/heads/main"
        )));
        assert!(git_log.contains("fetch --force --depth 1 origin"));
        assert!(cargo_log.contains("fetch --locked --manifest-path"));
        assert!(cargo_log.contains(&format!(
            "--package qubit-infra-{}",
            tool.trim_start_matches("rs-infra-")
        )));
        let executable = fixture
            .root
            .path()
            .join("cache/rs-infra/tools")
            .join(tool)
            .join(&fixture.revision)
            .join(host_target())
            .join(rustc_release())
            .join("bin")
            .join(tool);
        assert!(executable.is_file(), "missing cached executable for {tool}");
    }
}

#[test]
fn test_latest_forwards_arguments_path_and_child_exit_status() {
    let fixture = Fixture::new();
    fixture.prewarm();
    let cached = fixture
        .root
        .path()
        .join("cache/rs-infra/tools/rs-infra-platform")
        .join(&fixture.revision)
        .join(host_target())
        .join(rustc_release())
        .join("bin/rs-infra-platform");
    assert!(cached.is_file(), "{}", cached.display());
    assert_eq!(&fs::read(&cached).unwrap()[..10], b"#!/bin/sh\n");
    assert_ne!(fs::metadata(&cached).unwrap().permissions().mode() & 0o111, 0);
    let project = fixture.root.path().join("project");
    let output = fixture
        .command(&[
            "latest",
            "--project",
            project.to_str().unwrap(),
            "--tool",
            "rs-infra-platform",
            "--",
            "--version",
            "fixture-arg",
        ])
        .env("RS_INFRA_SHARED_ROOT", fixture.manager())
        .env("FIXTURE_TOOL_EXIT", "7")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("tool.log")).unwrap().trim(),
        "--version fixture-arg"
    );
    assert!(
        fs::read_to_string(fixture.root.path().join("tool-path.log"))
            .unwrap()
            .starts_with(fixture.root.path().join("cache/rs-infra/invocations").to_str().unwrap())
    );
}

#[test]
fn test_latest_ci_uses_a_snapshot_with_every_supported_tool() {
    let fixture = Fixture::new();
    fixture.prewarm();
    let project = fixture.root.path().join("project");
    let output = fixture
        .command(&[
            "latest",
            "--project",
            project.to_str().unwrap(),
            "--tool",
            "rs-infra-ci",
            "--",
            "ci-arg",
        ])
        .env("RS_INFRA_SHARED_ROOT", fixture.manager())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("tool.log")).unwrap().trim(),
        "ci-arg"
    );
    let tool_path = fs::read_to_string(fixture.root.path().join("tool-path.log")).unwrap();
    let bin_dir = std::env::split_paths(std::ffi::OsStr::new(tool_path.trim()))
        .next()
        .expect("PATH must start with the invocation bin directory");
    for tool in TOOLS {
        assert!(bin_dir.join(tool).is_file(), "missing {tool} in {}", bin_dir.display());
    }
}

#[test]
fn test_latest_reports_context_when_a_cached_tool_cannot_be_started() {
    let fixture = Fixture::new();
    fixture.prewarm();
    let executable = fixture
        .root
        .path()
        .join("cache/rs-infra/tools/rs-infra-platform")
        .join(&fixture.revision)
        .join(host_target())
        .join(rustc_release())
        .join("bin/rs-infra-platform");
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(&executable, permissions).unwrap();
    let project = fixture.root.path().join("project");
    let output = fixture
        .command(&[
            "latest",
            "--project",
            project.to_str().unwrap(),
            "--tool",
            "rs-infra-platform",
        ])
        .env("RS_INFRA_SHARED_ROOT", fixture.manager())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("failed to start rs-infra-platform"));
}

#[test]
fn test_latest_falls_back_to_existing_path_when_cache_path_cannot_be_joined() {
    let fixture = Fixture::new();
    let cache = fixture.root.path().join("cache:with-colon");
    let project = fixture.root.path().join("project");
    let prewarm = fixture
        .command(&["prewarm", "--project"])
        .arg(&project)
        .env("RS_INFRA_CACHE_DIR", &cache)
        .env("RS_INFRA_SHARED_ROOT", fixture.manager())
        .output()
        .unwrap();
    assert!(prewarm.status.success(), "{}", String::from_utf8_lossy(&prewarm.stderr));
    let output = fixture
        .command(&[
            "latest",
            "--project",
            project.to_str().unwrap(),
            "--tool",
            "rs-infra-platform",
        ])
        .env("RS_INFRA_CACHE_DIR", &cache)
        .env("RS_INFRA_SHARED_ROOT", fixture.manager())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let tool_path = fs::read_to_string(fixture.root.path().join("tool-path.log")).unwrap();
    assert!(tool_path.starts_with(fixture.root.path().join("fake-bin").to_str().unwrap()));
    assert!(!tool_path.starts_with(cache.to_str().unwrap()));
}

#[test]
fn test_latest_rejects_unknown_tools_before_cache_or_git_work() {
    let fixture = Fixture::new();
    let output = fixture.command(&["latest", "--tool", "not-a-tool"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown infrastructure tool 'not-a-tool'"));
    assert!(!fixture.root.path().join("git.log").exists());
    assert!(!fixture.root.path().join("cache").exists());
}

#[test]
fn test_latest_reports_missing_cache_configuration_and_rejects_non_checkout_manager_roots() {
    let fixture = Fixture::new();
    let no_cache = fixture
        .command(&["latest", "--tool", "rs-infra-platform"])
        .env_remove("RS_INFRA_CACHE_DIR")
        .env_remove("XDG_CACHE_HOME")
        .env_remove("HOME")
        .output()
        .unwrap();
    assert_eq!(no_cache.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&no_cache.stderr).contains("RS_INFRA_CACHE_DIR, XDG_CACHE_HOME, or HOME must be set")
    );

    for (variable, manager) in [
        ("FIXTURE_BAD_ROOT", fixture.manager()),
        ("FIXTURE_FAIL_ROOT_LOOKUP", fixture.manager()),
    ] {
        let output = fixture
            .command(&["prewarm"])
            .env("RS_INFRA_SHARED_ROOT", manager)
            .env(variable, "yes")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("RS_INFRA_SHARED_ROOT does not contain"));
    }
    let no_entrypoints = fixture.root.path().join("manager-without-entrypoints");
    fs::create_dir_all(no_entrypoints.join(".git")).unwrap();
    let output = fixture
        .command(&["prewarm"])
        .env("RS_INFRA_SHARED_ROOT", no_entrypoints)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("RS_INFRA_SHARED_ROOT does not contain"));
}

#[test]
fn test_prewarm_uses_xdg_and_home_cache_fallbacks() {
    for (environment, expected) in [("xdg", "xdg-cache/qubit"), ("home", "home-cache/.cache/qubit")] {
        let fixture = Fixture::new();
        let mut command = fixture.command(&["prewarm"]);
        command
            .env("RS_INFRA_SHARED_ROOT", fixture.manager())
            .env_remove("RS_INFRA_CACHE_DIR");
        if environment == "xdg" {
            command.env("XDG_CACHE_HOME", fixture.root.path().join("xdg-cache"));
        } else {
            command
                .env_remove("XDG_CACHE_HOME")
                .env("HOME", fixture.root.path().join("home-cache"));
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(fixture.root.path().join(expected).join("rs-infra/invocations").is_dir());
    }
}

#[test]
fn test_prewarm_fetches_manager_source_when_shared_checkout_is_not_configured() {
    let fixture = Fixture::new();
    let output = fixture
        .command(&["prewarm", "--project"])
        .arg(fixture.root.path().join("project"))
        .env_remove("RS_INFRA_SHARED_ROOT")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let git_log = fs::read_to_string(fixture.root.path().join("git.log")).unwrap();
    assert!(git_log.contains("ls-remote https://github.com/qubit-ltd/rs-infra-tools.git refs/heads/main"));
    assert!(git_log.contains("remote add origin https://github.com/qubit-ltd/rs-infra-tools.git"));
    assert!(git_log.contains("fetch --force --depth 1 origin"));
    assert!(
        fixture
            .root
            .path()
            .join("cache/rs-infra/sources/rs-infra-tools")
            .is_dir()
    );
}

#[test]
fn test_prewarm_installs_the_selected_style_analyzer_into_the_project() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    let output = fixture
        .command(&["prewarm", "--project"])
        .arg(&project)
        .arg("--install-style-analyzer")
        .env("RS_INFRA_SHARED_ROOT", fixture.manager())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(project.join(".infra/style/bin/rs-infra-style").is_file());
    let metadata = fs::read_to_string(project.join(".infra/style/tool.revision.json")).unwrap();
    assert!(metadata.contains(&fixture.revision));
}

#[test]
fn test_prewarm_rejects_invalid_manager_and_invalid_main_revision() {
    let fixture = Fixture::new();
    let bad_manager = fixture
        .command(&["prewarm"])
        .env("RS_INFRA_SHARED_ROOT", fixture.root.path().join("missing-manager"))
        .output()
        .unwrap();
    assert_eq!(bad_manager.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad_manager.stderr).contains("RS_INFRA_SHARED_ROOT does not contain"));
    let bad_revision = fixture
        .command(&["prewarm"])
        .env("RS_INFRA_SHARED_ROOT", fixture.manager())
        .env("FIXTURE_BAD_REVISION", "yes")
        .env("RS_INFRA_NETWORK_MAX_ATTEMPTS", "1")
        .output()
        .unwrap();
    assert_eq!(bad_revision.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad_revision.stderr).contains("returned an invalid Git SHA"));
}

#[test]
fn test_prewarm_retries_transient_revision_lookup_in_an_isolated_process() {
    let fixture = Fixture::new();
    let output = fixture
        .command(&["prewarm"])
        .env("RS_INFRA_SHARED_ROOT", fixture.manager())
        .env("FIXTURE_RETRY_LS_REMOTE", "yes")
        .env("RS_INFRA_NETWORK_MAX_ATTEMPTS", "2")
        .env("RS_INFRA_NETWORK_RETRY_DELAY_SECONDS", "0")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("retrying in 0s"));
    assert!(fixture.root.path().join("retry.once").is_file());
}

#[test]
fn test_prewarm_reports_checkout_and_build_failures_without_network_access() {
    let cases = [
        ("FIXTURE_FAIL_LS_REMOTE", "yes", "lookup-failed"),
        ("FIXTURE_EMPTY_REVISION", "yes", "main branch returned no revision"),
        (
            "FIXTURE_NON_UTF8_REVISION",
            "yes",
            "git ls-remote returned non-UTF-8 output",
        ),
        ("FIXTURE_FAIL_INIT", "yes", "failed to initialize source cache"),
        ("FIXTURE_FAIL_REMOTE", "yes", "failed to configure source cache"),
        ("FIXTURE_FAIL_GIT_FETCH", "yes", "fetch tool source failed"),
        ("FIXTURE_FAIL_CHECKOUT", "yes", "failed to check out"),
        ("FIXTURE_FAIL_HEAD", "yes", "resolved to , expected"),
        ("FIXTURE_BAD_HEAD", "yes", "resolved to ffffffffff"),
        ("FIXTURE_NO_MANIFEST", "yes", "has no Cargo.toml"),
        ("FIXTURE_CARGO_FAIL_FETCH", "yes", "fetch Cargo dependencies failed"),
        ("FIXTURE_CARGO_FAIL_BUILD", "yes", "failed to build"),
        ("FIXTURE_CARGO_NO_BINARY", "yes", "cargo build succeeded but"),
        ("FIXTURE_PUBLISH_FAILURE", "copy", "failed to publish rs-infra-ci"),
        ("FIXTURE_PUBLISH_FAILURE", "rename", "failed to install rs-infra-ci"),
    ];
    for (variable, value, expected) in cases {
        let fixture = Fixture::new();
        let output = fixture
            .command(&["prewarm"])
            .env("RS_INFRA_SHARED_ROOT", fixture.manager())
            .env(variable, value)
            .env("RS_INFRA_NETWORK_MAX_ATTEMPTS", "1")
            .env("RS_INFRA_NETWORK_RETRY_DELAY_SECONDS", "0")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{variable}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{variable}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
