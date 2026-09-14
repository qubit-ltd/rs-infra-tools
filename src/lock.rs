// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! The deliberately small, shell-bootstrap-friendly tools.lock format.

// Defines the lock entry type.
#[path = "lock_entry.rs"]
mod lock_entry;
// Defines the parsed lock file and its validation logic.
#[path = "lock_file.rs"]
mod lock_file;

pub use lock_entry::LockEntry;
pub use lock_file::LockFile;
