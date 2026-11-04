// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Publishes the resolved analyzer and verifiable identity into one project.

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use fs2::FileExt;
use sha2::Digest;
use sha2::Sha256;

/// Distinguishes staging files created by threads within the current process.
static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

/// Installs one explicitly resolved analyzer and its integrity metadata.
///
/// Readers must verify the executable digest against the metadata: each file
/// is atomically replaced, and an interruption between replacements is detected
/// as a mismatched pair rather than accepted as a valid installation. Repeating
/// installation repairs such an incomplete pair.
///
/// # Parameters
///
/// * `project` - Existing project root receiving ignored analyzer artifacts.
/// * `source` - Resolved shared-cache executable for the exact revision.
/// * `revision` - Full immutable lowercase Git commit of the executable.
///
/// # Returns
///
/// Returns `Ok(())` after the installation pair matches the resolved bytes.
///
/// # Errors
///
/// Returns an error for invalid identities, unsupported analyzers, unsafe
/// filesystem paths, concurrent source mutation, or publication failures.
pub(super) fn install(project: &Path, source: &Path, revision: &str) -> Result<()> {
    if revision.len() != 40
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("style analyzer revision must be a lowercase 40-character Git SHA");
    }
    let source_metadata = fs::symlink_metadata(source)?;
    if !source_metadata.file_type().is_file() {
        bail!(
            "resolved style analyzer must be a regular executable: {}",
            source.display()
        );
    }
    let bytes = fs::read(source)?;
    let probe = probe(source)?;
    if !probe.status.success() {
        bail!(
            "resolved rs-infra-style@{revision} does not support analyze: {}",
            String::from_utf8_lossy(&probe.stderr)
        );
    }
    if fs::read(source)? != bytes {
        bail!("resolved style analyzer changed during its capability probe");
    }
    let project = project.canonicalize()?;
    let style = directory(&project, Path::new(".infra/style"))?;
    let bin = directory(&project, Path::new(".infra/style/bin"))?;
    let executable = bin.join(if cfg!(windows) {
        "rs-infra-style.exe"
    } else {
        "rs-infra-style"
    });
    let metadata = style.join("tool.revision.json");
    let lock_path = style.join("tool.revision.lock");
    for destination in [&executable, &metadata, &lock_path] {
        regular_or_absent(destination)?;
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    lock.lock_exclusive()?;
    // Another installer may have published while this process waited.
    regular_or_absent(&executable)?;
    regular_or_absent(&metadata)?;
    let sha256: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let record = format!(
        "{{\n  \"schema_version\": 1,\n  \"tool\": \"rs-infra-style\",\n  \"revision\": \"{revision}\",\n  \"sha256\": \"{sha256}\"\n}}\n"
    );
    publish(&executable, &bytes, Some(source_metadata.permissions()))?;
    publish(&metadata, record.as_bytes(), None)?;
    eprintln!(
        "rs-infra: installed project style analyzer@{revision} at {}",
        executable.display()
    );
    Ok(())
}

/// Probes analyzer capability, retrying only transient executable-busy errors.
///
/// Parallel process creation can briefly retain a writable file descriptor
/// inherited before another thread closes a newly written executable. Other
/// spawn errors and unsuccessful analyzer responses are never retried here.
///
/// # Parameters
///
/// * `source` - Exact shared-cache executable selected by the manager.
///
/// # Returns
///
/// Returns the completed `analyze --help` output for capability validation.
///
/// # Errors
///
/// Returns the spawn error after a bounded executable-busy retry budget, or
/// immediately for other process-launch failures.
fn probe(source: &Path) -> Result<Output> {
    for attempt in 0..5 {
        match Command::new(source).args(["analyze", "--help"]).output() {
            Ok(output) => return Ok(output),
            Err(error) if cfg!(unix) && error.raw_os_error() == Some(26) && attempt < 4 => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error).context("failed to probe the resolved style analyzer"),
        }
    }
    unreachable!("the last probe attempt always returns")
}

/// Creates only ordinary directories below the canonical project root.
///
/// # Parameters
///
/// * `project` - Canonical project root defining the installation boundary.
/// * `relative` - Fixed relative destination directory.
///
/// # Returns
///
/// Returns the validated directory path.
///
/// # Errors
///
/// Returns an error for symbolic links, conflicting files, or creation errors.
fn directory(project: &Path, relative: &Path) -> Result<PathBuf> {
    let mut current = project.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => bail!(
                "analyzer installation directory is not an ordinary directory: {}",
                current.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
                if !fs::symlink_metadata(&current)?.file_type().is_dir() {
                    bail!("analyzer installation directory changed: {}", current.display());
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(current)
}

/// Refuses symbolic links and non-file objects at replaceable destinations.
///
/// # Parameters
///
/// * `path` - Destination which may be absent or an ordinary file.
///
/// # Errors
///
/// Returns an error for an unsafe destination or failed metadata lookup.
fn regular_or_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => bail!(
            "analyzer installation destination is not an ordinary file: {}",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Flushes a private staging file and atomically replaces one destination.
///
/// # Parameters
///
/// * `path` - Validated file destination under the project installation lock.
/// * `bytes` - Complete replacement content.
/// * `permissions` - Cached executable permissions, or default data-file mode.
///
/// # Errors
///
/// Returns an error when staging, flushing, permission setting, or replacement
/// fails; an unpublished staging file is removed before returning.
fn publish(path: &Path, bytes: &[u8], permissions: Option<fs::Permissions>) -> Result<()> {
    if path.is_file() && fs::read(path)? == bytes {
        if let Some(permissions) = permissions {
            fs::set_permissions(path, permissions)?;
        }
        return Ok(());
    }
    let (temporary, mut staging) = loop {
        let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let temporary = path.with_file_name(format!(".style-analyzer-{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&temporary) {
            Ok(staging) => break (temporary, staging),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let result = (|| -> Result<()> {
        staging.write_all(bytes)?;
        if let Some(permissions) = permissions {
            staging.set_permissions(permissions)?;
        }
        staging.sync_all()?;
        drop(staging);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::thread;

    use sha2::Digest;
    use sha2::Sha256;

    use super::install;

    const OLD: &str = "0123456789abcdef0123456789abcdef01234567";
    const NEW: &str = "fedcba9876543210fedcba9876543210fedcba98";

    fn analyzer(path: &Path, marker: &str, supported: bool) {
        fs::write(
            path,
            format!(
                "#!/bin/sh\n# {marker}\n[ \"$1\" = analyze ] && [ \"$2\" = --help ] || exit 7\nexit {}\n",
                if supported { 0 } else { 2 }
            ),
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn assert_identity(project: &Path, revision: &str) {
        let bytes = fs::read(project.join(".infra/style/bin/rs-infra-style")).unwrap();
        let expected: String = Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect();
        let record = fs::read_to_string(project.join(".infra/style/tool.revision.json")).unwrap();
        assert!(record.contains(&format!("\"revision\": \"{revision}\"")), "{record}");
        assert!(record.contains(&format!("\"sha256\": \"{expected}\"")), "{record}");
    }

    #[test]
    fn test_install_records_identity_and_repeated_update_repairs_corruption() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let project = root.path().join("project");
        fs::create_dir(&project).unwrap();
        analyzer(&source, "first", true);
        install(&project, &source, OLD).unwrap();
        assert_identity(&project, OLD);
        install(&project, &source, OLD).unwrap();
        assert_identity(&project, OLD);
        fs::write(project.join(".infra/style/bin/rs-infra-style"), b"corrupt").unwrap();
        install(&project, &source, OLD).unwrap();
        assert_identity(&project, OLD);
        analyzer(&source, "second", true);
        install(&project, &source, NEW).unwrap();
        assert_identity(&project, NEW);
        assert!(
            fs::read_to_string(project.join(".infra/style/bin/rs-infra-style"))
                .unwrap()
                .contains("second")
        );
    }

    #[test]
    fn test_unsupported_analyzer_preserves_accepted_installation() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let project = root.path().join("project");
        fs::create_dir(&project).unwrap();
        analyzer(&source, "accepted", true);
        install(&project, &source, OLD).unwrap();
        analyzer(&source, "legacy", false);
        assert!(install(&project, &source, NEW).is_err());
        assert_identity(&project, OLD);
    }

    #[test]
    fn test_symlinked_destination_components_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        analyzer(&source, "source", true);
        for relative in [
            ".infra",
            ".infra/style",
            ".infra/style/bin",
            ".infra/style/bin/rs-infra-style",
            ".infra/style/tool.revision.json",
            ".infra/style/tool.revision.lock",
        ] {
            let project = root.path().join(relative.replace('/', "_"));
            fs::create_dir_all(project.join(Path::new(relative).parent().unwrap())).unwrap();
            let outside = root.path().join("outside");
            fs::create_dir_all(&outside).unwrap();
            std::os::unix::fs::symlink(&outside, project.join(relative)).unwrap();
            assert!(install(&project, &source, OLD).is_err(), "{relative}");
        }
    }

    #[test]
    fn test_invalid_revision_and_nonregular_targets_fail_before_publication() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        analyzer(&source, "source", true);
        assert!(install(root.path(), &source, "main").is_err());
        assert!(!root.path().join(".infra").exists());
        fs::create_dir_all(root.path().join(".infra/style/bin/rs-infra-style")).unwrap();
        assert!(install(root.path(), &source, OLD).is_err());
    }

    #[test]
    fn test_concurrent_updates_finish_with_matching_executable_and_metadata() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        fs::create_dir(&project).unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        analyzer(&first, "first", true);
        analyzer(&second, "second", true);
        thread::scope(|scope| {
            let p = &project;
            let f = &first;
            scope.spawn(move || install(p, f, OLD).unwrap());
            let s = &second;
            scope.spawn(move || install(p, s, NEW).unwrap());
        });
        let record = fs::read_to_string(project.join(".infra/style/tool.revision.json")).unwrap();
        assert_identity(&project, if record.contains(OLD) { OLD } else { NEW });
    }
}
