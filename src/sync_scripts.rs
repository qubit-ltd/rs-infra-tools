// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Synchronizes the project's shared rs-infra shell scripts.

use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

const SCRIPTS: &[(&str, &str)] = &[
    (
        ".infra/bin/ci-check.sh",
        include_str!("../assets/project-infra/bin/ci-check.sh"),
    ),
    (
        ".infra/bin/update-infra.sh",
        include_str!("../assets/project-infra/bin/update-infra.sh"),
    ),
    (
        ".infra/bin/infra-tool.sh",
        include_str!("../assets/project-infra/bin/infra-tool.sh"),
    ),
    (
        ".infra/lib/infra-tool.sh",
        include_str!("../assets/project-infra/lib/infra-tool.sh"),
    ),
    (
        ".infra/lib/cleanup-build-artifacts.sh",
        include_str!("../assets/project-infra/lib/cleanup-build-artifacts.sh"),
    ),
];

/// Writes the pinned shared rs-infra scripts into a project.
///
/// Existing managed scripts are replaced and legacy generated binaries are
/// removed; unrelated project files are left untouched. Returns an error with
/// the affected path when a managed file is a symlink or cannot be written.
pub(super) fn sync(project: &Path) -> Result<()> {
    let project = project
        .canonicalize()
        .with_context(|| format!("unable to resolve project root {}", project.display()))?;

    for (relative, content) in SCRIPTS {
        let path = project.join(relative);
        if path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            bail!("refusing to replace symlink {}", path.display());
        }
        let current = fs::read(&path).ok();
        if current.as_deref() == Some(content.as_bytes()) {
            ensure_executable(&path)?;
            continue;
        }
        let parent = path.parent().context("managed script has no parent directory")?;
        fs::create_dir_all(parent).with_context(|| format!("unable to create {}", parent.display()))?;
        write_atomically(&path, content)?;
    }
    remove_legacy_tool_artifacts(&project)?;
    Ok(())
}

/// Replaces a managed script atomically so a running updater can keep reading
/// its old inode.
fn write_atomically(path: &Path, content: &str) -> Result<()> {
    let parent = path.parent().context("managed script has no parent directory")?;
    let file_name = path
        .file_name()
        .context("managed script has no filename")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let result = (|| {
        fs::write(&temporary, content).with_context(|| format!("unable to write {}", temporary.display()))?;
        ensure_executable(&temporary)?;
        fs::rename(&temporary, path).with_context(|| format!("unable to replace managed script {}", path.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Removes generated files from the former shared `.infra/tools/bin/bin`
/// layout.
fn remove_legacy_tool_artifacts(project: &Path) -> Result<()> {
    let legacy_root = project.join(".infra/tools/bin");
    let nested_bin = legacy_root.join("bin");
    if nested_bin.exists() {
        fs::remove_dir_all(&nested_bin)
            .with_context(|| format!("unable to remove legacy tool binaries {}", nested_bin.display()))?;
    }

    for name in [".crates.toml", ".crates2.json"] {
        let path = legacy_root.join(name);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("unable to remove {}", path.display())),
        }
    }

    match fs::read_dir(&legacy_root) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.with_context(|| format!("unable to read {}", legacy_root.display()))?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("rs-infra-") && name.ends_with(".revision") {
                    fs::remove_file(entry.path())
                        .with_context(|| format!("unable to remove legacy tool marker {}", entry.path().display()))?;
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("unable to read {}", legacy_root.display())),
    }
    Ok(())
}

/// Sets the executable mode required by the shell entry points.
fn ensure_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .with_context(|| format!("unable to inspect {}", path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .with_context(|| format!("unable to set executable mode on {}", path.display()))
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::remove_legacy_tool_artifacts;
    use super::sync;

    /// Verifies the shared updater installs the CI entry point used by
    /// migration.
    #[test]
    fn test_sync_installs_ci_check_entry_point() {
        let project = tempfile::tempdir().expect("create temporary project");
        sync(project.path()).expect("sync shared infrastructure scripts");
        let script = project.path().join(".infra/bin/ci-check.sh");
        let content = fs::read_to_string(&script).expect("read CI entry point");
        assert!(content.contains("rs-infra-ci --project"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(script)
                    .expect("CI entry point metadata")
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
        }
    }

    #[test]
    fn test_remove_legacy_tool_artifacts_preserves_current_tool_binary() {
        let project = tempfile::tempdir().expect("create temporary project");
        let legacy_root = project.path().join(".infra/tools/bin");
        let nested_bin = legacy_root.join("bin");
        fs::create_dir_all(&nested_bin).expect("create legacy nested binary directory");
        fs::write(nested_bin.join("rs-infra-ci"), "legacy binary").expect("write legacy binary");
        fs::write(legacy_root.join("rs-infra-ci.revision"), "old revision").expect("write legacy revision marker");
        fs::write(legacy_root.join(".crates.toml"), "legacy install metadata").expect("write legacy install metadata");
        fs::write(legacy_root.join("rs-infra-tools"), "current binary").expect("write current tool binary");

        remove_legacy_tool_artifacts(project.path()).expect("remove legacy tool artifacts");

        assert!(!nested_bin.exists(), "legacy nested binaries should be removed");
        assert!(!legacy_root.join("rs-infra-ci.revision").exists());
        assert!(!legacy_root.join(".crates.toml").exists());
        assert_eq!(
            fs::read_to_string(legacy_root.join("rs-infra-tools")).expect("read current binary"),
            "current binary"
        );
    }
}
