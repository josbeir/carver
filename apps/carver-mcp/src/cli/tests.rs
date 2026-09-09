use super::*;
use clap::error::ErrorKind;

type TestResult = Result<(), clap::Error>;

#[test]
fn server_arguments_should_default_to_read_only() -> TestResult {
    let args = Args::try_parse_from(["carver-mcp"])?;
    assert!(!args.allow_write);
    assert!(args.command.is_none());
    Ok(())
}

#[test]
fn server_arguments_should_enable_writes_only_with_the_explicit_flag() -> TestResult {
    let args = Args::try_parse_from(["carver-mcp", "--allow-write"])?;
    assert!(args.allow_write);
    Ok(())
}

#[test]
fn server_arguments_should_reject_misspelled_flags() {
    assert!(
        matches!(Args::try_parse_from(["carver-mcp", "--allow-wirte"]), Err(error) if error.kind() == ErrorKind::UnknownArgument)
    );
}

#[test]
fn server_arguments_should_reject_unknown_commands() {
    assert!(
        matches!(Args::try_parse_from(["carver-mcp", "serve"]), Err(error) if error.kind() == ErrorKind::InvalidSubcommand)
    );
}

#[test]
fn configure_arguments_should_accept_the_write_flag_before_the_subcommand() -> TestResult {
    let args = Args::try_parse_from(["carver-mcp", "--allow-write", "configure", "codex"])?;
    assert!(args.allow_write);
    assert!(matches!(
        args.command,
        Some(Command::Configure {
            client: AgentClient::Codex
        })
    ));
    Ok(())
}

#[test]
fn configure_arguments_should_accept_the_write_flag_after_the_client() -> TestResult {
    let args = Args::try_parse_from(["carver-mcp", "configure", "codex", "--allow-write"])?;
    assert!(args.allow_write);
    Ok(())
}

#[test]
fn configure_arguments_should_preserve_the_other_alias() -> TestResult {
    let args = Args::try_parse_from(["carver-mcp", "configure", "other"])?;
    assert!(!args.allow_write);
    assert!(matches!(
        args.command,
        Some(Command::Configure {
            client: AgentClient::Generic
        })
    ));
    Ok(())
}

#[test]
fn configure_arguments_should_require_a_supported_client() {
    assert!(
        matches!(Args::try_parse_from(["carver-mcp", "configure", "unknown"]), Err(error) if error.kind() == ErrorKind::InvalidValue)
    );
}

#[test]
fn configure_arguments_should_require_a_client() {
    assert!(
        matches!(Args::try_parse_from(["carver-mcp", "configure"]), Err(error) if error.kind() == ErrorKind::MissingRequiredArgument)
    );
}

#[test]
fn configure_arguments_should_reject_unknown_options() {
    assert!(
        matches!(Args::try_parse_from(["carver-mcp", "configure", "codex", "--write"]), Err(error) if error.kind() == ErrorKind::UnknownArgument)
    );
}

#[test]
fn configure_help_should_list_the_shared_client_names() -> Result<(), Box<dyn std::error::Error>> {
    let error = Args::try_parse_from(["carver-mcp", "configure", "--help"])
        .err()
        .ok_or("help should stop parsing")?;
    assert_eq!(error.kind(), ErrorKind::DisplayHelp);
    let help = error.to_string();
    for name in ["codex", "claude-code", "copilot", "vscode", "generic"] {
        assert!(
            help.contains(&format!("- {name}:")),
            "missing client {name}"
        );
    }
    Ok(())
}
