//! The deliberately small, shell-bootstrap-friendly tools.lock format.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

/// One immutable tool artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEntry {
    pub name: String,
    pub source: String,
    pub revision: String,
    pub target: String,
    pub sha256: String,
}

/// Parsed tools.lock file.
#[derive(Debug, Default)]
pub struct LockFile {
    entries: BTreeMap<String, LockEntry>,
}

impl LockFile {
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

    pub fn entry(&self, name: &str) -> Result<&LockEntry> {
        self.entries
            .get(name)
            .with_context(|| format!("tool '{name}' is not present in the lock file"))
    }
}

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
    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn parses_comments_and_valid_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tools.lock");
        fs::write(&path, format!("# comment\nrs-infra-style file:///style {REVISION} x86_64-unknown-linux-gnu {SHA}\n")).unwrap();
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
