// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Defines the operations accepted by the command-line parser.

use std::path::PathBuf;

use clap::Subcommand;

/// Represents a supported lock-file or execution operation.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Ensures a locked tool and prints its executable path.
    Ensure {
        /// Path to the `tools.lock` file.
        #[arg(long)]
        lock: PathBuf,
        /// Name of the tool to ensure.
        #[arg(long)]
        tool: String,
    },
    /// Ensures a locked tool and executes it.
    Exec {
        /// Path to the `tools.lock` file.
        #[arg(long)]
        lock: PathBuf,
        /// Name of the tool to execute.
        #[arg(long)]
        tool: String,
        /// Arguments passed to the executable.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Runs the current main revision of an infrastructure tool.
    Latest {
        /// Project root passed to the tool.
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// Infrastructure executable name, for example `rs-infra-style`.
        #[arg(long)]
        tool: String,
        /// Arguments passed to the executable unchanged.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Resolves and caches every supported infrastructure tool.
    Prewarm {
        /// Project root whose configuration will be used later.
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// Install the resolved analyzer and its identity into the project.
        #[arg(long)]
        install_style_analyzer: bool,
    },
}
