//! Package-aware MCP launcher and setup instructions for Carver.
//!
//! Enable `cli` to expose supported client names through `clap::ValueEnum`.

#![forbid(unsafe_code)]

use std::env;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Agent client supported by Carver's setup instructions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum AgentClient {
    /// `OpenAI` Codex.
    Codex,
    /// Anthropic Claude Code.
    ClaudeCode,
    /// GitHub Copilot CLI.
    #[cfg_attr(feature = "cli", value(name = "copilot"))]
    CopilotCli,
    /// GitHub Copilot in Visual Studio Code.
    #[cfg_attr(feature = "cli", value(name = "vscode"))]
    VsCodeCopilot,
    /// Any MCP client that accepts a stdio command and argument list.
    #[cfg_attr(feature = "cli", value(alias = "other"))]
    Generic,
}

/// How Carver is installed on the local machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallChannel {
    /// A native package exposes `carver-mcp` on `PATH`.
    Native,
    /// An `AppImage` bundles the MCP executable.
    AppImage {
        /// Absolute path to the `AppImage`, supplied by its runtime.
        path: String,
    },
    /// A Flatpak application owns the MCP executable.
    Flatpak {
        /// Installed Flatpak application identifier.
        app_id: String,
    },
    /// A Snap package owns the MCP executable.
    Snap {
        /// Installed Snap package name.
        name: String,
    },
}

/// Command and arguments used by an MCP client to start Carver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpInvocation {
    /// Executable available to the host-side MCP client.
    pub command: String,
    /// Arguments passed to the executable.
    pub arguments: Vec<String>,
}

impl InstallChannel {
    /// Detects the package environment of the current Carver process.
    #[must_use]
    pub fn detect() -> Self {
        if std::path::Path::new("/.flatpak-info").is_file() {
            let app_id =
                env::var("FLATPAK_ID").unwrap_or_else(|_| "io.github.josbeir.Carver".to_owned());
            return Self::Flatpak { app_id };
        }
        if let Ok(name) = env::var("SNAP_NAME") {
            return Self::Snap { name };
        }
        if let Ok(path) = env::var("APPIMAGE") {
            return Self::AppImage { path };
        }
        Self::Native
    }

    /// Returns the host-side command that starts Carver's MCP service.
    #[must_use]
    pub fn mcp_invocation(&self, allow_write: bool) -> McpInvocation {
        let mut invocation = match self {
            Self::Native => McpInvocation {
                command: "carver-mcp".to_owned(),
                arguments: Vec::new(),
            },
            Self::AppImage { path } => McpInvocation {
                command: path.clone(),
                arguments: vec![
                    "--appimage-extract-and-run".to_owned(),
                    "--command=carver-mcp".to_owned(),
                ],
            },
            Self::Flatpak { app_id } => McpInvocation {
                command: "flatpak".to_owned(),
                arguments: vec![
                    "run".to_owned(),
                    "--command=carver-mcp".to_owned(),
                    app_id.clone(),
                ],
            },
            Self::Snap { name } => McpInvocation {
                command: format!("{name}.mcp"),
                arguments: Vec::new(),
            },
        };
        if allow_write {
            invocation.arguments.push("--allow-write".to_owned());
        }
        invocation
    }
}

/// Copyable setup material for one agent client.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupInstruction {
    /// Command that adds the server to a command-line client.
    pub command: Option<String>,
    /// JSON configuration for clients configured through a file.
    pub configuration: Option<String>,
    /// Command that verifies the registration when supported.
    pub verification: Option<String>,
}

/// Failures generating client setup instructions.
#[derive(Debug, Error)]
pub enum SetupError {
    /// The launch arguments cannot be represented in a shell command.
    #[error("Could not quote the MCP launch command: {0}")]
    Shell(#[from] shlex::QuoteError),
    /// The client configuration could not be serialized.
    #[error("Could not serialize the MCP configuration: {0}")]
    Json(#[from] serde_json::Error),
}

/// Creates setup material for one client and installation channel.
///
/// # Errors
///
/// Returns an error when a shell launch argument contains a NUL byte or JSON
/// configuration cannot be serialized.
pub fn setup_instruction(
    client: AgentClient,
    channel: &InstallChannel,
    allow_write: bool,
) -> Result<SetupInstruction, SetupError> {
    let invocation = channel.mcp_invocation(allow_write);
    let (registration, verification) = match client {
        AgentClient::Codex => ("codex mcp add carver --", "codex mcp get carver"),
        AgentClient::ClaudeCode => (
            "claude mcp add --scope user carver --",
            "claude mcp get carver",
        ),
        AgentClient::CopilotCli => ("copilot mcp add carver --", "copilot mcp get carver"),
        AgentClient::VsCodeCopilot => {
            return Ok(SetupInstruction {
                command: None,
                configuration: Some(vscode_configuration(&invocation)?),
                verification: None,
            });
        }
        AgentClient::Generic => {
            return Ok(SetupInstruction {
                command: None,
                configuration: Some(generic_configuration(&invocation)?),
                verification: None,
            });
        }
    };
    let launch = shell_command(&invocation)?;
    Ok(SetupInstruction {
        command: Some(format!("{registration} {launch}")),
        configuration: None,
        verification: Some(verification.to_owned()),
    })
}

fn shell_command(invocation: &McpInvocation) -> Result<String, shlex::QuoteError> {
    shlex::try_join(
        std::iter::once(invocation.command.as_str())
            .chain(invocation.arguments.iter().map(String::as_str)),
    )
}

fn vscode_configuration(invocation: &McpInvocation) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&serde_json::json!({
        "servers": {
            "carver": {
                "type": "stdio",
                "command": invocation.command,
                "args": invocation.arguments,
            }
        }
    }))
}

fn generic_configuration(invocation: &McpInvocation) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&serde_json::json!({
        "transport": "stdio",
        "command": invocation.command,
        "args": invocation.arguments,
    }))
}

#[cfg(test)]
mod tests;
