// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Describes one supported infrastructure tool.

/// Identifies an infrastructure executable, its repository, and its Cargo
/// package.
#[derive(Clone, Copy)]
pub(super) struct Tool {
    /// The binary name exposed to callers.
    pub(super) name: &'static str,
    /// The GitHub repository name.
    pub(super) repository: &'static str,
    /// The Cargo package that owns the binary.
    pub(super) package: &'static str,
}

impl Tool {
    /// Creates a static description of one infrastructure tool.
    ///
    /// # Parameters
    ///
    /// - `name`: executable name registered with the runtime.
    /// - `repository`: GitHub repository name without the organization prefix.
    /// - `package`: Cargo package that provides the executable.
    pub(super) const fn new(name: &'static str, repository: &'static str, package: &'static str) -> Self {
        Self {
            name,
            repository,
            package,
        }
    }

    /// Returns the Git clone URL for this tool's repository.
    pub(super) fn source(self) -> String {
        format!("https://github.com/qubit-ltd/{}.git", self.repository)
    }
}
