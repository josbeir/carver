//! Command-line arguments validated before opening the installed library.

use carver_agent_integration::AgentClient;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Local stdio Model Context Protocol server for Carver"
)]
pub(crate) struct Args {
    /// Permit tools that create or modify library content.
    #[arg(long, global = true)]
    pub(crate) allow_write: bool,
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Print setup instructions for an agent client without starting the server.
    Configure {
        /// Client that will connect to Carver.
        #[arg(value_enum)]
        client: AgentClient,
    },
}

#[cfg(test)]
mod tests;
