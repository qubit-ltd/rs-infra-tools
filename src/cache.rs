// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Content-addressed-enough per-tool cache and artifact verification.

use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use sha2::Digest;
use sha2::Sha256;

use crate::lock::LockEntry;

/// Ensures that the locked artifact is present, verified, and executable.
///
/// A local `file://` source is copied directly; an HTTPS source is downloaded
/// with `curl`. The artifact is verified against its SHA-256 digest before it
/// is atomically installed in the cache. Network access and filesystem writes
/// are side effects of this operation.
///
/// # Parameters
///
/// - `entry`: Validated tool metadata describing the expected artifact and
///   target platform.
///
/// # Returns
///
/// The executable path in the cache. A cached file is reused only when its
/// digest matches the locked SHA-256 value.
///
/// # Errors
///
/// Returns an error when the current host does not match the entry, the cache
/// cannot be accessed, the artifact cannot be copied or downloaded, its digest
/// does not match, or installation fails.
pub fn ensure(entry: &LockEntry) -> Result<PathBuf> {
    let target = env::var("RS_INFRA_TARGET").unwrap_or_else(|_| host_target().to_owned());
    if entry.target != target {
        bail!(
            "tool '{}' targets {}, but the current platform is {}",
            entry.name,
            entry.target,
            target
        );
    }
    let cache_path = cache_path(entry)?;
    if cache_path.is_file() && digest(&cache_path)? == entry.sha256.to_ascii_lowercase() {
        return Ok(cache_path);
    }

    let parent = cache_path.parent().context("cache path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create cache directory {}", parent.display()))?;
    let temporary = parent.join(format!(".{}.tmp-{}", entry.name, std::process::id()));
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    download(entry, &temporary)?;
    let actual = digest(&temporary)?;
    if actual != entry.sha256.to_ascii_lowercase() {
        fs::remove_file(&temporary).ok();
        bail!(
            "SHA-256 mismatch for {}: expected {}, got {actual}",
            entry.name,
            entry.sha256
        );
    }
    make_executable(&temporary)?;
    fs::rename(&temporary, &cache_path)
        .with_context(|| format!("failed to install {} at {}", entry.name, cache_path.display()))?;
    Ok(cache_path)
}

/// Returns the target triple supported by the current host, or `unsupported`.
///
/// # Returns
///
/// The target triple for the supported architecture and operating-system
/// combinations, or `unsupported` when the host is not recognized.
fn host_target() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        _ => "unsupported",
    }
}

/// Builds the cache path for a locked artifact.
///
/// # Parameters
///
/// - `entry`: Validated artifact metadata whose name, revision, and target
///   determine the path components.
///
/// # Returns
///
/// The artifact path below the configured cache root.
///
/// # Errors
///
/// Returns an error when none of `RS_INFRA_CACHE_DIR`, `XDG_CACHE_HOME`, or
/// `HOME` is set.
fn cache_path(entry: &LockEntry) -> Result<PathBuf> {
    let root = env::var_os("RS_INFRA_CACHE_DIR")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_CACHE_HOME").map(|path| PathBuf::from(path).join("qubit")))
        .or_else(|| env::var_os("HOME").map(|path| PathBuf::from(path).join(".cache/qubit")))
        .context("RS_INFRA_CACHE_DIR, XDG_CACHE_HOME, or HOME must be set")?;
    Ok(root
        .join("rs-infra")
        .join(&entry.name)
        .join(&entry.revision)
        .join(&entry.target)
        .join(&entry.name))
}

/// Copies or downloads an artifact to a temporary destination.
///
/// # Parameters
///
/// - `entry`: Validated metadata describing the local or HTTPS source.
/// - `destination`: Temporary path that receives the artifact bytes.
///
/// # Errors
///
/// Returns an error when the local copy fails, `curl` cannot start, or the
/// HTTPS download exits unsuccessfully.
fn download(entry: &LockEntry, destination: &Path) -> Result<()> {
    if let Some(path) = entry.source.strip_prefix("file://") {
        fs::copy(path, destination).with_context(|| format!("failed to copy local artifact {path}"))?;
        return Ok(());
    }

    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--proto",
            "=https",
            "--tlsv1.2",
            &entry.source,
            "--output",
        ])
        .arg(destination)
        .output()
        .context("failed to start curl; install curl or use a file:// source")?;
    if !output.status.success() {
        bail!(
            "failed to download {}: {}",
            entry.source,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Computes the lowercase SHA-256 digest of a file.
///
/// # Parameters
///
/// - `path`: File whose complete contents are read and hashed.
///
/// # Returns
///
/// The lowercase hexadecimal SHA-256 digest.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or read.
fn digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Adds executable permissions to a cached artifact on Unix.
///
/// # Parameters
///
/// - `path`: Existing artifact whose owner, group, and other execute bits are
///   set to `755`.
///
/// # Errors
///
/// Returns an error when metadata lookup or permission update fails.
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

/// Leaves executable permissions unchanged on non-Unix platforms.
///
/// # Parameters
///
/// - `_path`: Artifact path, intentionally unused because this platform has no
///   Unix permission update in this implementation.
///
/// # Returns
///
/// Always succeeds without changing the artifact.
#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Runs a verified tool and returns its process exit code.
///
/// The child process inherits the caller's standard streams. If the process
/// terminates without an exit code, this returns `1`.
///
/// # Parameters
///
/// - `path`: Verified executable to start.
/// - `args`: Arguments passed to the child process unchanged.
///
/// # Returns
///
/// The child's exit code, or `1` when it terminates without one.
///
/// # Errors
///
/// Returns an error when the operating system cannot start or wait for the
/// child process.
pub fn run(path: &Path, args: &[String]) -> Result<i32> {
    let status = Command::new(path).args(args).status()?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::ffi::OsString;
    use std::io::Write;
    use std::path::Path;
    use std::process::Command;
    use std::sync::Mutex;

    use super::cache_path;
    use super::digest;
    use super::download;
    use super::ensure;
    use super::run;
    use crate::lock::LockEntry;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_test_env<T>(changes: &[(&str, Option<OsString>)], action: impl FnOnce() -> T) -> T {
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let saved = changes
            .iter()
            .map(|(key, _)| ((*key).to_owned(), env::var_os(key)))
            .collect::<Vec<_>>();

        for (key, value) in changes {
            unsafe {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }

        struct RestoreEnv(Vec<(String, Option<OsString>)>);

        impl Drop for RestoreEnv {
            fn drop(&mut self) {
                for (key, value) in &self.0 {
                    unsafe {
                        match value {
                            Some(value) => env::set_var(key, value),
                            None => env::remove_var(key),
                        }
                    }
                }
            }
        }

        let _restore = RestoreEnv(saved);
        action()
    }

    fn entry(name: &str, source: &Path, sha256: &str) -> LockEntry {
        LockEntry {
            name: name.into(),
            source: format!("file://{}", source.display()),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            sha256: sha256.into(),
        }
    }

    fn with_cache<T>(cache: &Path, action: impl FnOnce() -> T) -> T {
        with_test_env(
            &[
                ("RS_INFRA_CACHE_DIR", Some(cache.as_os_str().to_owned())),
                ("RS_INFRA_TARGET", Some("x86_64-unknown-linux-gnu".into())),
            ],
            action,
        )
    }

    #[test]
    fn test_digest_encodes_sha256_as_lowercase_hex() {
        let source = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(source.path(), b"abc").unwrap();

        assert_eq!(
            digest(source.path()).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_local_artifact_is_cached_and_reused() {
        let source = tempfile::NamedTempFile::new().unwrap();
        let mut file = source.reopen().unwrap();
        file.write_all(b"#!/bin/sh\nprintf cached\n").unwrap();
        let digest = digest(source.path()).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let entry = entry("fake-tool", source.path(), &digest);

        with_cache(cache.path(), || {
            let path = ensure(&entry).unwrap();
            assert!(path.is_file());
            assert_eq!(std::fs::read(&path).unwrap(), b"#!/bin/sh\nprintf cached\n");

            std::fs::write(source.path(), b"changed source").unwrap();
            assert_eq!(ensure(&entry).unwrap(), path);
            assert_eq!(std::fs::read(path).unwrap(), b"#!/bin/sh\nprintf cached\n");
        });
    }

    #[test]
    fn test_ensure_rejects_a_different_target_before_cache_access() {
        let source = tempfile::NamedTempFile::new().unwrap();
        let entry = entry("wrong-target-tool", source.path(), "unused");
        let result = with_test_env(
            &[("RS_INFRA_TARGET", Some("unsupported-target".into()))],
            || ensure(&entry),
        );

        let error = result.unwrap_err().to_string();
        assert!(error.contains("targets x86_64-unknown-linux-gnu"));
        assert!(error.contains("current platform is unsupported-target"));
    }

    #[test]
    fn test_cache_path_uses_xdg_root_when_explicit_cache_is_missing() {
        let source = tempfile::NamedTempFile::new().unwrap();
        let entry = entry("xdg-tool", source.path(), "unused");
        let xdg = tempfile::tempdir().unwrap();
        let path = with_test_env(
            &[
                ("RS_INFRA_CACHE_DIR", None),
                ("XDG_CACHE_HOME", Some(xdg.path().as_os_str().to_owned())),
            ],
            || cache_path(&entry).unwrap(),
        );

        assert_eq!(
            path,
            xdg.path()
                .join("qubit/rs-infra/xdg-tool/0123456789abcdef0123456789abcdef01234567/x86_64-unknown-linux-gnu/xdg-tool")
        );
    }

    #[test]
    fn test_cache_path_uses_home_fallback_when_other_roots_are_missing() {
        let source = tempfile::NamedTempFile::new().unwrap();
        let entry = entry("home-tool", source.path(), "unused");
        let home = tempfile::tempdir().unwrap();
        let path = with_test_env(
            &[
                ("RS_INFRA_CACHE_DIR", None),
                ("XDG_CACHE_HOME", None),
                ("HOME", Some(home.path().as_os_str().to_owned())),
            ],
            || cache_path(&entry).unwrap(),
        );

        assert_eq!(
            path,
            home.path()
                .join(".cache/qubit/rs-infra/home-tool/0123456789abcdef0123456789abcdef01234567/x86_64-unknown-linux-gnu/home-tool")
        );
    }

    #[test]
    fn test_host_target_matches_the_current_platform() {
        let expected = match (std::env::consts::ARCH, std::env::consts::OS) {
            ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
            ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
            ("x86_64", "macos") => "x86_64-apple-darwin",
            ("aarch64", "macos") => "aarch64-apple-darwin",
            ("x86_64", "windows") => "x86_64-pc-windows-msvc",
            ("aarch64", "windows") => "aarch64-pc-windows-msvc",
            _ => "unsupported",
        };

        assert_eq!(super::host_target(), expected);
    }

    #[test]
    fn test_cache_path_requires_a_root_when_all_fallbacks_are_missing() {
        let source = tempfile::NamedTempFile::new().unwrap();
        let entry = entry("no-root-tool", source.path(), "unused");
        let result = with_test_env(
            &[
                ("RS_INFRA_CACHE_DIR", None),
                ("XDG_CACHE_HOME", None),
                ("HOME", None),
            ],
            || cache_path(&entry),
        );

        assert!(result.unwrap_err().to_string().contains("HOME must be set"));
    }

    #[test]
    fn test_ensure_replaces_corrupt_cached_contents_with_verified_source() {
        let source = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(source.path(), b"verified artifact bytes").unwrap();
        let digest = digest(source.path()).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let entry = entry("repair-tool", source.path(), &digest);

        with_cache(cache.path(), || {
            let path = cache_path(&entry).unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"corrupt bytes").unwrap();

            let installed = ensure(&entry).unwrap();
            assert_eq!(installed, path);
            assert_eq!(
                std::fs::read(installed).unwrap(),
                b"verified artifact bytes"
            );
        });
    }

    #[test]
    fn test_ensure_rejects_digest_mismatch_and_removes_temporary_file() {
        let source = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(source.path(), b"unexpected artifact").unwrap();
        let cache = tempfile::tempdir().unwrap();
        let entry = entry("mismatch-tool", source.path(), &"0".repeat(64));

        with_cache(cache.path(), || {
            let result = ensure(&entry);
            let error = result.unwrap_err().to_string();
            assert!(error.contains("SHA-256 mismatch for mismatch-tool"));
            let path = cache_path(&entry).unwrap();
            assert!(!path.exists());
            assert!(!path
                .parent()
                .unwrap()
                .join(format!(".mismatch-tool.tmp-{}", std::process::id()))
                .exists());
        });
    }

    #[test]
    fn test_ensure_reports_a_missing_local_source() {
        let cache = tempfile::tempdir().unwrap();
        let source = cache.path().join("missing-source");
        let entry = entry("missing-source-tool", &source, &"0".repeat(64));

        with_cache(cache.path(), || {
            let error = ensure(&entry).unwrap_err().to_string();
            assert!(error.contains("failed to copy local artifact"));
            assert!(!cache_path(&entry).unwrap().exists());
        });
    }

    #[test]
    fn test_ensure_reports_cache_directory_creation_failure() {
        let source = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(source.path(), b"artifact").unwrap();
        let digest = digest(source.path()).unwrap();
        let cache = tempfile::tempdir().unwrap();
        std::fs::write(cache.path().join("rs-infra"), b"not a directory").unwrap();
        let entry = entry("blocked-cache-tool", source.path(), &digest);

        with_cache(cache.path(), || {
            let error = ensure(&entry).unwrap_err().to_string();
            assert!(error.contains("failed to create cache directory"));
            assert_eq!(
                std::fs::read(cache.path().join("rs-infra")).unwrap(),
                b"not a directory"
            );
        });
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_reports_installation_failure_without_losing_download() {
        let source = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(source.path(), b"verified artifact").unwrap();
        let digest = digest(source.path()).unwrap();
        let cache = tempfile::tempdir().unwrap();
        let entry = entry("blocked-install-tool", source.path(), &digest);

        with_cache(cache.path(), || {
            let path = cache_path(&entry).unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::create_dir(&path).unwrap();

            let error = ensure(&entry).unwrap_err().to_string();
            assert!(error.contains("failed to install blocked-install-tool"));
            assert!(path.is_dir());
            assert_eq!(
                std::fs::read(
                    path.parent()
                        .unwrap()
                        .join(format!(".blocked-install-tool.tmp-{}", std::process::id()))
                )
                .unwrap(),
                b"verified artifact"
            );
        });
    }

    #[test]
    fn test_download_reports_an_unavailable_local_https_endpoint() {
        let status = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "cache::tests::test_download_failure_child",
                "--nocapture",
            ])
            .env("RS_INFRA_CACHE_TEST_DOWNLOAD_CHILD", "1")
            .env("NO_PROXY", "127.0.0.1,localhost")
            .env("no_proxy", "127.0.0.1,localhost")
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn test_download_failure_child() {
        if env::var_os("RS_INFRA_CACHE_TEST_DOWNLOAD_CHILD").is_none() {
            return;
        }

        let source = tempfile::NamedTempFile::new().unwrap();
        let mut entry = entry("https-tool", source.path(), "unused");
        entry.source = "https://127.0.0.1:1/unavailable".into();
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("download.tmp");

        let error = download(&entry, &destination).unwrap_err().to_string();
        assert!(error.contains("failed to download"));
        assert!(!destination.exists());
    }

    #[test]
    fn test_download_reports_missing_curl_without_process_environment_changes() {
        let empty_path = tempfile::tempdir().unwrap();
        let status = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "cache::tests::test_download_start_failure_child",
                "--nocapture",
            ])
            .env("RS_INFRA_CACHE_TEST_CURL_START_CHILD", "1")
            .env("PATH", empty_path.path())
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn test_download_start_failure_child() {
        if env::var_os("RS_INFRA_CACHE_TEST_CURL_START_CHILD").is_none() {
            return;
        }

        let source = tempfile::NamedTempFile::new().unwrap();
        let mut entry = entry("missing-curl-tool", source.path(), "unused");
        entry.source = "https://127.0.0.1/unavailable".into();
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("download.tmp");

        let error = download(&entry, &destination).unwrap_err().to_string();
        assert!(error.contains("failed to start curl"));
        assert!(!destination.exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_run_returns_exit_status_and_reports_start_failure() {
        let success = run(Path::new("/bin/sh"), &["-c".into(), "exit 0".into()]).unwrap();
        assert_eq!(success, 0);

        let failure = run(Path::new("/bin/sh"), &["-c".into(), "exit 23".into()]).unwrap();
        assert_eq!(failure, 23);

        let signal = run(Path::new("/bin/sh"), &["-c".into(), "kill -TERM $$".into()]).unwrap();
        assert_eq!(signal, 1);

        let missing = run(Path::new("/definitely/missing/rs-infra-tool"), &[]);
        assert!(missing.is_err());
    }
}
