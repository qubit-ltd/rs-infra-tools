use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "rs-infra-tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Ensure {
        #[arg(long)]
        lock: PathBuf,
        #[arg(long)]
        tool: String,
    },
    Exec {
        #[arg(long)]
        lock: PathBuf,
        #[arg(long)]
        tool: String,
        #[arg(last = true)]
        args: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Ensure { lock, tool } => {
            println!("{}", qubit_infra_tools::ensure(&lock, &tool)?.display());
        }
        Command::Exec { lock, tool, args } => {
            let executable = qubit_infra_tools::ensure(&lock, &tool)?;
            let code = qubit_infra_tools::cache::run(&executable, &args)?;
            std::process::exit(code);
        }
    }
    Ok(())
}
