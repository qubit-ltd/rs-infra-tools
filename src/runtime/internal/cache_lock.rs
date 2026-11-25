// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Acquires and releases filesystem locks for shared cache entries.

use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use fs2::FileExt;

/// Holds an exclusive filesystem lock for one cache entry.
pub(super) struct CacheLock {
    /// Open file whose exclusive lock protects the associated cache entry.
    file: File,
}

impl CacheLock {
    /// Acquires an exclusive lock, creating its parent directory when needed.
    ///
    /// # Parameters
    ///
    /// - `path`: filesystem path used to coordinate access to one cache entry.
    pub(super) fn acquire(path: &Path) -> Result<Self> {
        fs::create_dir_all(path.parent().context("lock path has no parent")?)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .with_context(|| format!("failed to open cache lock {}", path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("failed to lock cache entry {}", path.display()))?;
        Ok(Self { file })
    }
}

impl Drop for CacheLock {
    /// Releases the filesystem lock when the guard is dropped.
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::CacheLock;

    #[test]
    fn test_acquire_creates_parent_and_lock_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/cache.lock");

        let lock = CacheLock::acquire(&path).unwrap();
        assert!(path.is_file());
        drop(lock);

        assert!(CacheLock::acquire(&path).is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn test_acquire_rejects_a_path_without_a_parent() {
        let error = CacheLock::acquire(Path::new("/")).err().unwrap();

        assert!(error.to_string().contains("lock path has no parent"));
    }

    #[test]
    fn test_acquire_reports_lock_open_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lock-is-a-directory");
        fs::create_dir(&path).unwrap();

        let error = CacheLock::acquire(&path).err().unwrap();

        assert!(
            format!("{error:#}").contains("failed to open cache lock"),
            "unexpected cache lock error: {error:#}"
        );
    }
}
