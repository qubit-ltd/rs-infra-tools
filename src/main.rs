// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use qubit_infra_tools::cache;
use qubit_infra_tools::ensure;

/// Command-line options for `rs-infra-tools`.
#[derive(Debug, Parser)]
#[command(name = "rs-infra-tools")]
struct Cli {
    /// The lock-file operation to execute.
    #[command(subcommand)]
    command: Command,
}

/// Supported lock-file and execution operations.
#[derive(Debug, Subcommand)]
enum Command {
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
}

/// Parses options and executes the selected infrastructure-tool operation.
///
/// Returns an error when the lock file or tool cannot be ensured. The `exec`
/// command exits with the child process's status code.
fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Ensure { lock, tool } => {
            println!("{}", ensure(&lock, &tool)?.display());
        }
        Command::Exec { lock, tool, args } => {
            let executable = ensure(&lock, &tool)?;
            let code = cache::run(&executable, &args)?;
            std::process::exit(code);
        }
    }
    Ok(())
}
