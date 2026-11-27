// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Installs the optional compiler cache from a pinned release archive.

use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure as require;

use super::cache_lock::CacheLock;
use super::executable_file_name;
use crate::cache::digest;
use crate::cache::make_executable;

const VERSION: &str = "v0.20.1";

/// Returns a managed binary directory when a supported host needs sccache.
pub(super) fn prepare(cache: &Path) -> Option<PathBuf> {
    const WRAPPERS: [&str; 4] = [
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    ];
    if env::var_os("RS_INFRA_SCCACHE").is_some_and(|value| value == "0")
        || env::var_os("GITHUB_ACTIONS").is_some_and(|value| value == "true")
        || WRAPPERS.into_iter().any(|name| env::var_os(name).is_some())
        || Command::new("sccache")
            .arg("--version")
            .output()
            .is_ok_and(|result| result.status.success())
    {
        return None;
    }
    let Some((target, checksum)) = release(env::consts::OS, env::consts::ARCH) else {
        eprintln!(
            "rs-infra: no sccache release for {}/{}",
            env::consts::OS,
            env::consts::ARCH
        );
        return None;
    };
    match ensure(cache, target, checksum, download) {
        Ok(directory) => Some(directory),
        Err(error) => {
            eprintln!("rs-infra: sccache setup failed; continuing with Cargo cache: {error:#}");
            None
        }
    }
}

/// Maps supported operating systems and architectures to pinned release assets.
fn release(os: &str, arch: &str) -> Option<(&'static str, &'static str)> {
    Some(match (os, arch) {
        ("linux", "x86_64") => (
            "x86_64-unknown-linux-musl",
            "c9f6c5a3ae84d9da0956b28c357a365827bda33f3383647fc0b67484c74a5ea9",
        ),
        ("linux", "aarch64") => (
            "aarch64-unknown-linux-musl",
            "505324daad5fcbb1a3cab5d57d26ef42baf4d10b236df7de93a008196f1f2230",
        ),
        ("macos", "x86_64") => (
            "x86_64-apple-darwin",
            "9e828824e52e12bd9c164859e893f0fb344cb3811faf7ea708f3c718d108c100",
        ),
        ("macos", "aarch64") => (
            "aarch64-apple-darwin",
            "d2658c9a24bab25a24fdc1917cb5b614b7c20a4ce8f363e5303d37b7e699d5ec",
        ),
        ("windows", "x86_64") => (
            "x86_64-pc-windows-msvc",
            "d96117e04c04449151a84818596356df7887026809f6fb988817ff76460d1073",
        ),
        ("windows", "aarch64") => (
            "aarch64-pc-windows-msvc",
            "5c6ccc53c310bbcfaf5909f1ca77ea0b8fe237c4c6c553ae342a7c88e23170c2",
        ),
        _ => return None,
    })
}

/// Verifies the archive before extracting and atomically installing its binary.
fn ensure(
    cache: &Path,
    target: &str,
    checksum: &str,
    fetch: impl FnOnce(&str, &Path) -> Result<()>,
) -> Result<PathBuf> {
    let directory = cache.join("tools/sccache").join(VERSION).join(target);
    let executable = executable_file_name("sccache");
    let destination = directory.join(&executable);
    let _lock = CacheLock::acquire(
        &cache
            .join("locks")
            .join(format!("sccache-{VERSION}-{target}.lock")),
    )?;
    if destination.is_file() {
        return Ok(directory);
    }

    fs::create_dir_all(&directory)?;
    let staged = tempfile::tempdir_in(&directory)?;
    let archive = staged.path().join("sccache.tar.gz");
    let release = format!("sccache-{VERSION}-{target}");
    let url =
        format!("https://github.com/mozilla/sccache/releases/download/{VERSION}/{release}.tar.gz");
    fetch(&url, &archive)?;
    require!(
        digest(&archive)? == checksum,
        "SHA-256 mismatch for {release}"
    );

    let staged_binary = staged.path().join(&executable);
    let status = Command::new("tar")
        .arg("-xOf")
        .arg(&archive)
        .arg(format!("{release}/{executable}"))
        .stdout(Stdio::from(fs::File::create(&staged_binary)?))
        .status()
        .context("failed to launch tar for sccache")?;
    require!(status.success(), "failed to extract {release}: {status}");
    require!(
        fs::metadata(&staged_binary)?.len() > 0,
        "{release} contains an empty binary"
    );
    make_executable(&staged_binary)?;
    fs::rename(staged_binary, destination)?;
    eprintln!("rs-infra: prepared sccache {VERSION} for {target}");
    Ok(directory)
}

/// Fetches a release archive with a bounded HTTPS request.
fn download(url: &str, destination: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--max-time",
            "45",
        ])
        .arg("--output")
        .arg(destination)
        .arg(url)
        .status()
        .context("failed to launch curl for sccache")?;
    require!(status.success(), "sccache download failed: {status}");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_verified_sccache_archive_is_installed_and_reused() {
        use std::fs;
        use std::process::Command;

        let fixture = tempfile::tempdir().unwrap();
        let host = "x86_64-unknown-linux-musl";
        let source = fixture
            .path()
            .join(format!("sccache-{}-{host}", super::VERSION));
        fs::create_dir(&source).unwrap();
        let executable = super::super::executable_file_name("sccache");
        fs::write(source.join(&executable), b"test binary").unwrap();
        let archive = fixture.path().join("fixture.tar.gz");
        assert!(
            Command::new("tar")
                .current_dir(fixture.path())
                .args(["-czf"])
                .arg(&archive)
                .arg(source.file_name().unwrap())
                .status()
                .unwrap()
                .success()
        );

        let cache = fixture.path().join("cache");
        let checksum = crate::cache::digest(&archive).unwrap();
        let install = |checksum: &str| {
            super::ensure(&cache, host, checksum, |_, destination| {
                fs::copy(&archive, destination)?;
                Ok(())
            })
        };
        assert!(install(&"0".repeat(64)).is_err());
        let path = install(&checksum).unwrap();
        assert_eq!(fs::read(path.join(&executable)).unwrap(), b"test binary");
        assert_eq!(
            super::ensure(&cache, host, &checksum, |_, _| panic!(
                "must reuse installed cache"
            ))
            .unwrap(),
            path
        );
    }
}
