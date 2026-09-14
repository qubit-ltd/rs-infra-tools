// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public lock-file based artifact installation.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;

use crate::cache;
use crate::lock::LockFile;

/// Ensures a named locked tool and returns its cached executable path.
///
/// This reads the lock file and may download, verify, and install the
/// artifact, so it performs filesystem and possibly network I/O. Returns an
/// error when the lock file or named entry is invalid, or installation fails.
///
/// # Parameters
///
/// - `lock_path`: Path to the whitespace-delimited `tools.lock` file.
/// - `tool_name`: Simple tool name to look up in the lock file.
///
/// # Returns
///
/// The verified executable path in the local cache.
///
/// # Errors
///
/// Returns an error when the lock file cannot be read or validated, the tool
/// is absent, or the artifact cannot be downloaded, verified, or installed.
pub fn ensure(lock_path: &Path, tool_name: &str) -> Result<PathBuf> {
    let lock = LockFile::read(lock_path)?;
    let entry = lock.entry(tool_name)?;
    cache::ensure(entry)
}
