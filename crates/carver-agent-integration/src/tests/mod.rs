use super::*;

#[test]
fn appimage_setup_should_preserve_spaces_and_the_write_gate()
-> Result<(), Box<dyn std::error::Error>> {
    let channel = InstallChannel::AppImage {
        path: "/home/user/My Apps/Carver.AppImage".into(),
    };
    for allow_write in [false, true] {
        let instruction = setup_instruction(AgentClient::Codex, &channel, allow_write)?;
        let words = shlex::split(&instruction.command.ok_or("command")?).ok_or("shell words")?;
        let mut expected = vec![
            "codex",
            "mcp",
            "add",
            "carver",
            "--",
            "/home/user/My Apps/Carver.AppImage",
            "--appimage-extract-and-run",
            "--command=carver-mcp",
        ];
        if allow_write {
            expected.push("--allow-write");
        }
        assert_eq!(words, expected);
    }
    Ok(())
}

#[test]
fn flatpak_instruction_should_start_the_package_command() -> Result<(), Box<dyn std::error::Error>>
{
    let instruction = setup_instruction(
        AgentClient::Codex,
        &InstallChannel::Flatpak {
            app_id: "io.github.josbeir.Carver".to_owned(),
        },
        true,
    )?;
    let words =
        shlex::split(&instruction.command.ok_or("shell command")?).ok_or("valid shell command")?;
    assert_eq!(
        words,
        [
            "codex",
            "mcp",
            "add",
            "carver",
            "--",
            "flatpak",
            "run",
            "--command=carver-mcp",
            "io.github.josbeir.Carver",
            "--allow-write"
        ]
    );
    Ok(())
}

#[test]
fn vscode_instruction_should_use_stdio_configuration() {
    let instruction = setup_instruction(AgentClient::VsCodeCopilot, &InstallChannel::Native, false)
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

#[test]
fn shell_setup_should_preserve_an_empty_argument() -> Result<(), Box<dyn std::error::Error>> {
    let instruction = setup_instruction(
        AgentClient::Codex,
        &InstallChannel::Flatpak {
            app_id: String::new(),
        },
        false,
    )?;
    let command = instruction.command.ok_or("shell command")?;
    let words = shlex::split(&command).ok_or("valid shell command")?;
    assert_eq!(words.last().map(String::as_str), Some(""));
    Ok(())
}

#[test]
fn shell_setup_should_quote_metacharacters_without_creating_extra_arguments()
-> Result<(), Box<dyn std::error::Error>> {
    let app_id = "app with 'quotes' ; $(touch /tmp/not-executed) `command`";
    let instruction = setup_instruction(
        AgentClient::ClaudeCode,
        &InstallChannel::Flatpak {
            app_id: app_id.into(),
        },
        true,
    )?;
    let words =
        shlex::split(&instruction.command.ok_or("shell command")?).ok_or("valid shell command")?;
    assert_eq!(&words[words.len() - 2..], [app_id, "--allow-write"]);
    Ok(())
}

#[test]
fn shell_setup_should_reject_nul_arguments() {
    let result = setup_instruction(
        AgentClient::Codex,
        &InstallChannel::Flatpak {
            app_id: "invalid\0id".into(),
        },
        false,
    );
    assert!(matches!(result, Err(SetupError::Shell(_))));
}

#[cfg(feature = "cli")]
#[test]
fn cli_names_should_preserve_existing_client_spellings() {
    use clap::ValueEnum;
    let names: Vec<_> = AgentClient::value_variants()
        .iter()
        .filter_map(clap::ValueEnum::to_possible_value)
        .map(|value| value.get_name().to_owned())
        .collect();
    assert_eq!(
        names,
        ["codex", "claude-code", "copilot", "vscode", "generic"]
    );
}
