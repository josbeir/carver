//! Package-aware MCP launcher and setup instructions for Carver.

#![forbid(unsafe_code)]

use std::env;

use serde::{Deserialize, Serialize};

/// Agent client supported by Carver's setup instructions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentClient {
    /// `OpenAI` Codex.
    Codex,
    /// Anthropic Claude Code.
    ClaudeCode,
    /// GitHub Copilot CLI.
    CopilotCli,
    /// GitHub Copilot in Visual Studio Code.
    VsCodeCopilot,
    /// Any MCP client that accepts a stdio command and argument list.
    Generic,
}

/// How Carver is installed on the local machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallChannel {
    /// A native package exposes `carver-mcp` on `PATH`.
    Native,
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

/// Creates setup material for one client and installation channel.
///
/// # Errors
///
/// Returns an error when the Visual Studio Code JSON configuration cannot be
/// serialized.
pub fn setup_instruction(
    client: AgentClient,
    channel: &InstallChannel,
    allow_write: bool,
) -> Result<SetupInstruction, serde_json::Error> {
    let invocation = channel.mcp_invocation(allow_write);
    let launch = shell_command(&invocation);
    match client {
        AgentClient::Codex => Ok(SetupInstruction {
            command: Some(format!("codex mcp add carver -- {launch}")),
            configuration: None,
            verification: Some("codex mcp get carver".to_owned()),
        }),
        AgentClient::ClaudeCode => Ok(SetupInstruction {
            command: Some(format!("claude mcp add --scope user carver -- {launch}")),
            configuration: None,
            verification: Some("claude mcp get carver".to_owned()),
        }),
        AgentClient::CopilotCli => Ok(SetupInstruction {
            command: Some(format!("copilot mcp add carver -- {launch}")),
            configuration: None,
            verification: Some("copilot mcp get carver".to_owned()),
        }),
        AgentClient::VsCodeCopilot => Ok(SetupInstruction {
            command: None,
            configuration: Some(vscode_configuration(&invocation)?),
            verification: None,
        }),
        AgentClient::Generic => Ok(SetupInstruction {
            command: None,
            configuration: Some(generic_configuration(&invocation)?),
            verification: None,
        }),
    }
}

fn shell_command(invocation: &McpInvocation) -> String {
    std::iter::once(invocation.command.as_str())
        .chain(invocation.arguments.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-_.=/".contains(character))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
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
mod tests {
    use super::*;

    #[test]
    fn flatpak_instruction_should_start_the_package_command() {
        let instruction = setup_instruction(
            AgentClient::Codex,
            &InstallChannel::Flatpak {
                app_id: "io.github.josbeir.Carver".to_owned(),
            },
            true,
        )
        .unwrap_or_else(|error| panic!("instruction should serialize: {error}"));

        assert_eq!(
            instruction.command,
            Some("codex mcp add carver -- flatpak run --command=carver-mcp io.github.josbeir.Carver --allow-write".to_owned())
        );
    }

    #[test]
    fn vscode_instruction_should_use_stdio_configuration() {
        let instruction =
            setup_instruction(AgentClient::VsCodeCopilot, &InstallChannel::Native, false)
                .unwrap_or_else(|error| panic!("instruction should serialize: {error}"));

        assert!(
            instruction
                .configuration
                .as_deref()
                .is_some_and(|configuration| configuration.contains("\"type\": \"stdio\""))
        );
    }

    #[test]
    fn generic_instruction_should_describe_a_stdio_transport() {
        let instruction = setup_instruction(AgentClient::Generic, &InstallChannel::Native, false)
            .unwrap_or_else(|error| panic!("instruction should serialize: {error}"));

        assert!(
            instruction
                .configuration
                .as_deref()
                .is_some_and(|configuration| configuration.contains("\"transport\": \"stdio\""))
        );
    }
}
