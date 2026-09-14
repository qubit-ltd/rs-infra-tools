//! Locked installation and execution of versioned infrastructure tools.

pub mod cache;
pub mod lock;

use std::path::Path;

use anyhow::Result;

/// Ensures a tool from a lock file and returns its absolute executable path.
pub fn ensure(lock_path: &Path, tool_name: &str) -> Result<std::path::PathBuf> {
    let lock = lock::LockFile::read(lock_path)?;
    let entry = lock.entry(tool_name)?;
    cache::ensure(entry)
}
