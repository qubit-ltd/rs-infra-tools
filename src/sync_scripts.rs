//! Synchronizes the project's shared rs-infra shell scripts.

use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

const SCRIPTS: &[(&str, &str)] = &[
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
/// Existing files at the four managed paths are replaced; all other project
/// files are left untouched. Returns an error with the affected path when a
/// managed file is a symlink or cannot be written.
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
        fs::write(&path, content).with_context(|| format!("unable to write {}", path.display()))?;
        ensure_executable(&path)?;
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
