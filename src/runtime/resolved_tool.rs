// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Holds the resolved revision and cached executable for one tool.

use std::path::PathBuf;

use super::tool::Tool;

/// Holds one selected tool's immutable revision and cached executable path.
pub(super) struct ResolvedTool {
    /// The tool represented by this resolved entry.
    pub(super) tool: Tool,
    /// The immutable Git revision used for the invocation.
    pub(super) revision: String,
    /// The executable built from `revision`.
    pub(super) executable: PathBuf,
}
