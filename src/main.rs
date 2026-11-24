// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Command-line entry point for `rs-infra-tools`.

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use cli::command::Command;
use qubit_infra_tools::cache;
use qubit_infra_tools::ensure;
use qubit_infra_tools::runtime;

mod cli;

/// Parses options, executes the selected operation, and always reports its
/// final status before exiting.
fn main() {
    let cli = Cli::parse();
    let result = execute(cli.command);

    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("❌ rs-infra-tools: failed: {error:#}");
            std::process::exit(1);
        }
    }
}

/// Runs the selected tool operation and returns its process exit code.
///
/// Errors from tool resolution or execution are returned to `main` for
/// consistent reporting on stderr.
///
/// # Parameters
///
/// - `command`: parsed operation to execute.
fn execute(command: Command) -> Result<i32> {
    match command {
        Command::Ensure { lock, tool } => {
            let executable = ensure(&lock, &tool)?;
            // Keep the path on stdout: shell wrappers use it as a command
            // substitution. Status messages go to stderr so they remain
            // visible without changing that interface.
            println!("{}", executable.display());
            eprintln!("✅ rs-infra-tools: ensure '{tool}' succeeded");
            Ok(0)
        }
        Command::Exec { lock, tool, args } => {
            let executable = ensure(&lock, &tool)?;
            let code = cache::run(&executable, &args)?;
            if code == 0 {
                eprintln!("✅ rs-infra-tools: exec '{tool}' succeeded (exit code 0)");
            } else {
                eprintln!("❌ rs-infra-tools: exec '{tool}' failed (exit code {code})");
            }
            Ok(code)
        }
        Command::Latest { project, tool, args } => runtime::run_latest(&project, &tool, &args),
        Command::Prewarm {
            project,
            install_style_analyzer,
        } => {
            if install_style_analyzer {
                runtime::prewarm_with_style_analyzer(&project)?;
            } else {
                runtime::prewarm(&project)?;
            }
            eprintln!("✅ rs-infra-tools: latest tool cache prepared");
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Cli;
    use super::Command;

    #[test]
    fn test_prewarm_requires_explicit_project_install_flag() {
        let ordinary = Cli::try_parse_from(["rs-infra-tools", "prewarm", "--project", "/project"]).unwrap();
        assert!(matches!(
            ordinary.command,
            Command::Prewarm {
                install_style_analyzer: false,
                ..
            }
        ));
        let install = Cli::try_parse_from([
            "rs-infra-tools",
            "prewarm",
            "--project",
            "/project",
            "--install-style-analyzer",
        ])
        .unwrap();
        assert!(matches!(
            install.command,
            Command::Prewarm {
                install_style_analyzer: true,
                ..
            }
        ));
    }
}
