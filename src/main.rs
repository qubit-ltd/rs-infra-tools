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
    use std::env;
    use std::ffi::OsString;
    use std::fs;
    use std::sync::Mutex;

    use clap::Parser;
    use sha2::Digest;
    use sha2::Sha256;

    use super::execute;
    use super::Cli;
    use super::Command;

    static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());

    struct EnvVarGuard {
        name: &'static str,
        original: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(name: &'static str, value: &std::ffi::OsStr) -> Self {
            let original = env::var_os(name);
            // SAFETY: Tests that mutate process environment hold ENVIRONMENT_LOCK;
            // the other test in this module only parses arguments and reads no environment.
            unsafe { env::set_var(name, value) };
            Self { name, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                // SAFETY: See the synchronization guarantee in `EnvVarGuard::set`.
                unsafe { env::set_var(self.name, value) };
            } else {
                // SAFETY: See the synchronization guarantee in `EnvVarGuard::set`.
                unsafe { env::remove_var(self.name) };
            }
        }
    }

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

    #[test]
    #[cfg(any(
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ),
        all(
            target_os = "macos",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )
    ))]
    fn test_execute_reports_successful_child() {
        let _environment_lock = ENVIRONMENT_LOCK.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let _cache_dir = EnvVarGuard::set("RS_INFRA_CACHE_DIR", directory.path().as_os_str());
        let source = directory.path().join("tool");
        let bytes = b"#!/bin/sh\nexit 0\n";
        fs::write(&source, bytes).unwrap();
        let target = match (env::consts::ARCH, env::consts::OS) {
            ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
            ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
            ("x86_64", "macos") => "x86_64-apple-darwin",
            ("aarch64", "macos") => "aarch64-apple-darwin",
            _ => panic!("unsupported test target"),
        };
        let digest = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let lock = directory.path().join("tools.lock");
        fs::write(
            &lock,
            format!(
                "test-success-tool file://{} 0123456789abcdef0123456789abcdef01234567 {target} {digest}\n",
                source.display()
            ),
        )
        .unwrap();

        let code = execute(Command::Exec {
            lock,
            tool: "test-success-tool".to_owned(),
            args: Vec::new(),
        })
        .unwrap();

        assert_eq!(code, 0);
    }

    #[test]
    fn test_execute_reports_unknown_latest_tool_and_failed_prewarm_modes() {
        let _environment_lock = ENVIRONMENT_LOCK.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let _cache_dir = EnvVarGuard::set("RS_INFRA_CACHE_DIR", directory.path().as_os_str());
        let _shared_root = EnvVarGuard::set(
            "RS_INFRA_SHARED_ROOT",
            directory.path().join("not-a-checkout").as_os_str(),
        );

        let latest_error = execute(Command::Latest {
            project: directory.path().to_owned(),
            tool: "unknown-tool".to_owned(),
            args: Vec::new(),
        })
        .unwrap_err();
        assert!(latest_error.to_string().contains("unknown infrastructure tool"));

        for install_style_analyzer in [false, true] {
            let prewarm_error = execute(Command::Prewarm {
                project: directory.path().to_owned(),
                install_style_analyzer,
            })
            .unwrap_err();
            assert!(
                prewarm_error.to_string().contains("RS_INFRA_SHARED_ROOT"),
                "unexpected prewarm error: {prewarm_error:#}"
            );
        }
    }
}
