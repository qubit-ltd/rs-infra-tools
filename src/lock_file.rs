// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Parsing and validation for the `tools.lock` file.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

use super::LockEntry;

/// Parsed `tools.lock` file and its validated artifact entries.
///
/// # Examples
///
/// ```
/// use std::fs;
///
/// use qubit_infra_tools::lock::LockFile;
///
/// let directory = tempfile::tempdir()?;
/// let path = directory.path().join("tools.lock");
/// fs::write(
///     &path,
///     "rustfmt file:///rustfmt 0123456789abcdef0123456789abcdef01234567 \
///      x86_64-unknown-linux-gnu \
///      0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
/// )?;
/// let lock = LockFile::read(&path)?;
/// assert_eq!(lock.entry("rustfmt")?.target, "x86_64-unknown-linux-gnu");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Default)]
pub struct LockFile {
    /// Validated entries indexed by their tool names.
    entries: BTreeMap<String, LockEntry>,
}

impl LockFile {
    /// Reads and validates a lock file from disk.
    ///
    /// Returns an error when the file cannot be read, a record does not have
    /// five fields, an entry is invalid, or a tool name is duplicated. This
    /// method performs filesystem I/O.
    ///
    /// # Parameters
    ///
    /// - `path`: Lock-file path to read as UTF-8 text.
    ///
    /// # Returns
    ///
    /// A lock file containing every non-comment, non-empty record after
    /// validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read as UTF-8, a record does
    /// not contain exactly five fields, an entry is invalid, or a tool name is
    /// duplicated.
    pub fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read lock file {}", path.display()))?;
        let mut entries = BTreeMap::new();

        for (line_number, raw_line) in text.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() != 5 {
                bail!("lock file line {} must contain 5 fields", line_number + 1);
            }
            let entry = LockEntry {
                name: fields[0].to_owned(),
                source: fields[1].to_owned(),
                revision: fields[2].to_owned(),
                target: fields[3].to_owned(),
                sha256: fields[4].to_owned(),
            };
            validate_entry(&entry)
                .with_context(|| format!("invalid lock entry on line {}", line_number + 1))?;
            if entries.insert(entry.name.clone(), entry).is_some() {
                bail!("duplicate tool on line {}", line_number + 1);
            }
        }

        Ok(Self { entries })
    }

    /// Returns the locked artifact for `name`.
    ///
    /// Returns an error when no entry has that tool name. The returned entry
    /// is borrowed from this lock file and remains valid while it is borrowed.
    ///
    /// # Parameters
    ///
    /// - `name`: Tool name to find in the validated entry map.
    ///
    /// # Returns
    ///
    /// A shared reference to the matching entry while `self` remains borrowed.
    ///
    /// # Errors
    ///
    /// Returns an error when no entry has the requested tool name.
    #[must_use = "the lock lookup result must be handled"]
    pub fn entry(&self, name: &str) -> Result<&LockEntry> {
        self.entries
            .get(name)
            .with_context(|| format!("tool '{name}' is not present in the lock file"))
    }
}

/// Validates the fields that identify and verify one locked artifact.
///
/// # Parameters
///
/// - `entry`: Artifact metadata to validate without modifying it.
///
/// # Errors
///
/// Returns an error when the name, revision, digest, source, or target violates
/// the lock-file field constraints.
fn validate_entry(entry: &LockEntry) -> Result<()> {
    if entry.name.is_empty() || entry.name.contains('/') {
        bail!("tool name must be a non-empty simple name");
    }
    if entry.revision.len() != 40 || !entry.revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("revision must be a 40-character hexadecimal Git SHA");
    }
    if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("sha256 must be a 64-character hexadecimal digest");
    }
    if entry.target.is_empty() || entry.source.is_empty() {
        bail!("source and target must be non-empty");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::LockFile;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn parses_comments_and_valid_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tools.lock");
        fs::write(
            &path,
            format!(
                "# comment\nrs-infra-style file:///style {REVISION} x86_64-unknown-linux-gnu {SHA}\n"
            ),
        )
        .unwrap();
        let lock = LockFile::read(&path).unwrap();
        assert_eq!(lock.entry("rs-infra-style").unwrap().revision, REVISION);
    }

    #[test]
    fn rejects_duplicate_names_and_bad_digests() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tools.lock");
        fs::write(
            &path,
            "a file:///a 0000000000000000000000000000000000000000 x bad\na file:///b 0000000000000000000000000000000000000000 x 0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        assert!(LockFile::read(&path).is_err());
    }
}
