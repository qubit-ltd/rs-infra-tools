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
pub fn ensure(lock_path: &Path, tool_name: &str) -> Result<PathBuf> {
    let lock = LockFile::read(lock_path)?;
    let entry = lock.entry(tool_name)?;
    cache::ensure(entry)
}
