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
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create cache directory {}", parent.display()))?;
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
    fs::rename(&temporary, &cache_path).with_context(|| {
        format!(
            "failed to install {} at {}",
            entry.name,
            cache_path.display()
        )
    })?;
    Ok(cache_path)
}

/// Returns the target triple supported by the current host, or `unsupported`.
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
fn download(entry: &LockEntry, destination: &Path) -> Result<()> {
    if let Some(path) = entry.source.strip_prefix("file://") {
        fs::copy(path, destination)
            .with_context(|| format!("failed to copy local artifact {path}"))?;
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
    Ok(format!("{:x}", hasher.finalize()))
}

/// Adds executable permissions to a cached artifact on Unix.
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

/// Leaves executable permissions unchanged on non-Unix platforms.
#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Runs a verified tool and returns its process exit code.
///
/// The child process inherits the caller's standard streams. If the process
/// terminates without an exit code, this returns `1`.
pub fn run(path: &Path, args: &[String]) -> Result<i32> {
    let status = Command::new(path).args(args).status()?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::io::Write;

    use crate::lock::LockEntry;

    use super::digest;
    use super::ensure;

    #[test]
    fn local_artifact_is_cached_and_reused() {
        let source = tempfile::NamedTempFile::new().unwrap();
        let mut file = source.reopen().unwrap();
        file.write_all(b"#!/bin/sh\nprintf cached\n").unwrap();
        let digest = digest(source.path()).unwrap();
        let cache = tempfile::tempdir().unwrap();
        unsafe {
            env::set_var("RS_INFRA_CACHE_DIR", cache.path());
        }
        let entry = LockEntry {
            name: "fake-tool".into(),
            source: format!("file://{}", source.path().display()),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            sha256: digest,
        };
        let path = ensure(&entry).unwrap();
        assert!(path.is_file());
        assert_eq!(ensure(&entry).unwrap(), path);
    }
}
