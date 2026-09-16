// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Command-line entry point for `rs-infra-tools`.

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

/// Parses options, executes the selected operation, and always reports its
/// final status before exiting.
fn main() {
    let cli = Cli::parse();
    let result = execute(cli.command);

    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("rs-infra-tools: failed: {error:#}");
            std::process::exit(1);
        }
    }
}

fn execute(command: Command) -> Result<i32> {
    match command {
        Command::Ensure { lock, tool } => {
            let executable = ensure(&lock, &tool)?;
            // Keep the path on stdout: shell wrappers use it as a command
            // substitution. Status messages go to stderr so they remain
            // visible without changing that interface.
            println!("{}", executable.display());
            eprintln!("rs-infra-tools: ensure '{tool}' succeeded");
            Ok(0)
        }
        Command::Exec { lock, tool, args } => {
            let executable = ensure(&lock, &tool)?;
            let code = cache::run(&executable, &args)?;
            if code == 0 {
                eprintln!("rs-infra-tools: exec '{tool}' succeeded (exit code 0)");
            } else {
                eprintln!("rs-infra-tools: exec '{tool}' failed (exit code {code})");
            }
            Ok(code)
        }
    }
}
