// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Parses the supported `rs-infra-tools` command-line interface.

use clap::Parser;

use self::command::Command;

pub(crate) mod command;

/// Holds the command selected by the command-line parser.
#[derive(Debug, Parser)]
#[command(name = "rs-infra-tools")]
pub(crate) struct Cli {
    /// The lock-file operation to execute.
    #[command(subcommand)]
    pub(crate) command: Command,
}
