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
        ".infra/bin/align-ci.sh",
        include_str!("../.infra/bin/align-ci.sh"),
    ),
    (
        ".infra/bin/ci-check.sh",
        include_str!("../.infra/bin/ci-check.sh"),
    ),
    (
        ".infra/bin/coverage.sh",
        include_str!("../.infra/bin/coverage.sh"),
    ),
    (
        ".infra/bin/dependency-update.sh",
        include_str!("../.infra/bin/dependency-update.sh"),
    ),
    (
        ".infra/bin/update-infra.sh",
        include_str!("../.infra/bin/update-infra.sh"),
    ),
    (
        ".infra/bin/infra-tool.sh",
        include_str!("../.infra/bin/infra-tool.sh"),
    ),
    (
        ".infra/bin/prepare-local-path-dependencies.sh",
        include_str!("../.infra/bin/prepare-local-path-dependencies.sh"),
    ),
    (
        ".infra/bin/project-ci-check.sh",
        include_str!("../.infra/bin/project-ci-check.sh"),
    ),
    (
        ".infra/bin/style-check.sh",
        include_str!("../.infra/bin/style-check.sh"),
    ),
    (
        ".infra/lib/cleanup-build-artifacts.sh",
        include_str!("../.infra/lib/cleanup-build-artifacts.sh"),
    ),
    (
        ".infra/lib/coverage-report.sh",
        include_str!("../.infra/lib/coverage-report.sh"),
    ),
    (
        ".infra/lib/dependency-update.sh",
        include_str!("../.infra/lib/dependency-update.sh"),
    ),
    (
        ".infra/lib/infra-tool.sh",
        include_str!("../.infra/lib/infra-tool.sh"),
    ),
    (
        ".infra/lib/network-retry.sh",
        include_str!("../.infra/lib/network-retry.sh"),
    ),
    (
        ".infra/lib/prepare-local-path-dependencies.sh",
        include_str!("../.infra/lib/prepare-local-path-dependencies.sh"),
    ),
    (
        ".infra/lib/tests/cleanup-build-artifacts-tests.sh",
        include_str!("../.infra/lib/tests/cleanup-build-artifacts-tests.sh"),
    ),
];

const TOOL_CONFIGS: &[(&str, &str)] = &[
    (
        ".infra/ci/tool.toml",
        include_str!("../.infra/ci/tool.toml"),
    ),
    (
        ".infra/coverage/tool.toml",
        include_str!("../.infra/coverage/tool.toml"),
    ),
    (
        ".infra/dependency/tool.toml",
        include_str!("../.infra/dependency/tool.toml"),
    ),
    (
        ".infra/pages/tool.toml",
        include_str!("../.infra/pages/tool.toml"),
    ),
    (
        ".infra/style/tool.toml",
        include_str!("../.infra/style/tool.toml"),
    ),
    (
        ".infra/tools/tool.toml",
        include_str!("../.infra/tools/tool.toml"),
    ),
    (
        ".infra/verify/tool.toml",
        include_str!("../.infra/verify/tool.toml"),
    ),
];

/// Writes the pinned shared rs-infra scripts into a project.
///
/// Existing managed scripts and standard tool metadata are replaced while
/// custom tool fields and revisions are retained. Project-specific policy and
/// check configuration files are left untouched. Returns an error with the
/// affected path when a managed file is a symlink or cannot be written.
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
        let parent = path
            .parent()
            .context("managed script has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("unable to create {}", parent.display()))?;
        write_atomically(&path, content)?;
    }
    sync_tool_configs(&project)?;
    remove_legacy_tool_artifacts(&project)?;
    Ok(())
}

/// Refreshes standard fields in enabled tool configurations while preserving
/// custom fields and the revision selected by `update-infra.sh`.
fn sync_tool_configs(project: &Path) -> Result<()> {
    const STANDARD_KEYS: [&str; 4] = ["source", "revision", "binary", "package"];

    for (relative, template) in TOOL_CONFIGS {
        let path = project.join(relative);
        if path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            bail!("refusing to replace symlink {}", path.display());
        }
        let Ok(current) = fs::read_to_string(&path) else {
            continue;
        };
        let standard_values = template
            .lines()
            .filter_map(|line| {
                let (key, _) = line.split_once('=')?;
                STANDARD_KEYS
                    .contains(&key.trim())
                    .then_some((key.trim(), line))
            })
            .collect::<Vec<_>>();
        let mut seen = Vec::new();
        let mut updated = String::new();
        for line in current.lines() {
            let Some((key, _)) = line.split_once('=') else {
                updated.push_str(line);
                updated.push('\n');
                continue;
            };
            let key = key.trim();
            if !STANDARD_KEYS.contains(&key) {
                updated.push_str(line);
                updated.push('\n');
                continue;
            }
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            if key == "revision" {
                updated.push_str(line);
            } else if let Some((_, standard_line)) = standard_values
                .iter()
                .find(|(template_key, _)| *template_key == key)
            {
                updated.push_str(standard_line);
            }
            updated.push('\n');
        }
        for (key, standard_line) in &standard_values {
            if seen.contains(key) {
                continue;
            }
            updated.push_str(standard_line);
            updated.push('\n');
        }
        if current != updated {
            fs::write(&path, updated)
                .with_context(|| format!("unable to update {}", path.display()))?;
        }
    }
    Ok(())
}

/// Replaces a managed script atomically so a running updater can keep reading
/// its old inode.
fn write_atomically(path: &Path, content: &str) -> Result<()> {
    let parent = path
        .parent()
        .context("managed script has no parent directory")?;
    let file_name = path
        .file_name()
        .context("managed script has no filename")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let result = (|| {
        fs::write(&temporary, content)
            .with_context(|| format!("unable to write {}", temporary.display()))?;
        ensure_executable(&temporary)?;
        fs::rename(&temporary, path)
            .with_context(|| format!("unable to replace managed script {}", path.display()))
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
        fs::remove_dir_all(&nested_bin).with_context(|| {
            format!(
                "unable to remove legacy tool binaries {}",
                nested_bin.display()
            )
        })?;
    }

    for name in [".crates.toml", ".crates2.json"] {
        let path = legacy_root.join(name);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("unable to remove {}", path.display()));
            }
        }
    }

    match fs::read_dir(&legacy_root) {
        Ok(entries) => {
            for entry in entries {
                let entry =
                    entry.with_context(|| format!("unable to read {}", legacy_root.display()))?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("rs-infra-") && name.ends_with(".revision") {
                    fs::remove_file(entry.path()).with_context(|| {
                        format!(
                            "unable to remove legacy tool marker {}",
                            entry.path().display()
                        )
                    })?;
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("unable to read {}", legacy_root.display()));
        }
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
    use std::process::Command;

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
        for name in [
            "align-ci.sh",
            "ci-check.sh",
            "coverage.sh",
            "dependency-update.sh",
            "style-check.sh",
            "update-infra.sh",
        ] {
            assert!(
                !project.path().join(name).exists(),
                "unexpected root script: {name}"
            );
        }
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

    /// Runs the installed entry point when the selected CI tasks omit coverage.
    #[cfg(unix)]
    #[test]
    fn test_ci_check_skips_report_when_ci_did_not_collect_coverage() {
        use std::os::unix::fs::PermissionsExt;

        let project = tempfile::tempdir().expect("create temporary project");
        sync(project.path()).expect("sync shared infrastructure scripts");
        let bin = project.path().join(".infra/bin");
        let prepare = bin.join("prepare-local-path-dependencies.sh");
        let infra_tool = bin.join("infra-tool.sh");
        let report = project.path().join(".infra/lib/coverage-report.sh");
        fs::write(&prepare, "#!/bin/sh\nexit 0\n").expect("prepare mock");
        fs::write(&infra_tool, "#!/bin/sh\nprintf '%s\\n' \"$*\" > ci-args\n").expect("CI mock");
        fs::write(&report, "#!/bin/sh\necho called > report-called\n").expect("report mock");
        for path in [&prepare, &infra_tool, &report] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("mock permissions");
        }

        let output = Command::new(bin.join("ci-check.sh"))
            .arg("--ignore-coverage-thresholds")
            .current_dir(project.path())
            .output()
            .expect("run CI entry point");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!project.path().join("report-called").exists());
        let args = fs::read_to_string(project.path().join("ci-args")).expect("CI invocation");
        assert!(args.contains("--ignore-coverage-thresholds check"));
    }

    #[test]
    fn test_remove_legacy_tool_artifacts_preserves_current_tool_binary() {
        let project = tempfile::tempdir().expect("create temporary project");
        let legacy_root = project.path().join(".infra/tools/bin");
        let nested_bin = legacy_root.join("bin");
        fs::create_dir_all(&nested_bin).expect("create legacy nested binary directory");
        fs::write(nested_bin.join("rs-infra-ci"), "legacy binary").expect("write legacy binary");
        fs::write(legacy_root.join("rs-infra-ci.revision"), "old revision")
            .expect("write legacy revision marker");
        fs::write(legacy_root.join(".crates.toml"), "legacy install metadata")
            .expect("write legacy install metadata");
        fs::write(legacy_root.join("rs-infra-tools"), "current binary")
            .expect("write current tool binary");

        remove_legacy_tool_artifacts(project.path()).expect("remove legacy tool artifacts");

        assert!(
            !nested_bin.exists(),
            "legacy nested binaries should be removed"
        );
        assert!(!legacy_root.join("rs-infra-ci.revision").exists());
        assert!(!legacy_root.join(".crates.toml").exists());
        assert_eq!(
            fs::read_to_string(legacy_root.join("rs-infra-tools")).expect("read current binary"),
            "current binary"
        );
    }
}
