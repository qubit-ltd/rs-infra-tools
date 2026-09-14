// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! A validated artifact entry from a `tools.lock` file.

/// Identifies one immutable tool artifact and its expected digest.
///
/// # Examples
///
/// ```
/// use qubit_infra_tools::lock::LockEntry;
///
/// let entry = LockEntry {
///     name: "rustfmt".to_owned(),
///     source: "https://example.invalid/rustfmt".to_owned(),
///     revision: "0123456789abcdef0123456789abcdef01234567".to_owned(),
///     target: "x86_64-unknown-linux-gnu".to_owned(),
///     sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
///         .to_owned(),
/// };
/// assert_eq!(entry.name, "rustfmt");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEntry {
    /// The tool's non-empty simple executable name without path separators.
    pub name: String,
    /// The HTTPS or local `file://` source of the directly executable artifact.
    pub source: String,
    /// The 40-character hexadecimal Git revision used for cache isolation.
    pub revision: String,
    /// The non-empty target triple for which the artifact was built.
    pub target: String,
    /// The expected 64-character lowercase or uppercase hexadecimal SHA-256
    /// digest of the artifact.
    pub sha256: String,
}
