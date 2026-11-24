mod activate;
mod backend;
mod common;
mod sync;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Synchronize the generated registrations and lock the workspace.
    Sync(sync::Options),
    /// Serve the Pixi build protocol over stdin and stdout.
    #[command(hide = true)]
    Backend,
    #[command(hide = true)]
    Activate { phase: activate::Phase },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Some(Command::Sync(options)) => sync::run_sync_command(options),
        Some(Command::Backend) => backend::serve_build_protocol(),
        Some(Command::Activate { phase }) => phase.print_shell_changes(),
        None => Ok(Cli::command().print_help()?),
    }
}
