//! External CLI contract tests that never open the user's installed library.

use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn command() -> Result<(tempfile::TempDir, Command), std::io::Error> {
    let directory = tempfile::tempdir()?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_carver-mcp"));
    command
        .env("XDG_CONFIG_HOME", directory.path().join("config"))
        .env("XDG_DATA_HOME", directory.path().join("data"))
        .env("XDG_CACHE_HOME", directory.path().join("cache"))
        .env_remove("SNAP_NAME");
    Ok((directory, command))
}

#[test]
fn invalid_options_should_fail_before_creating_a_library() -> TestResult {
    let (directory, mut command) = command()?;
    let output = command.arg("--allow-wirte").output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr)?.contains("--allow-wirte"));
    assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}

#[test]
fn help_should_exit_successfully_without_creating_a_library() -> TestResult {
    let (directory, mut command) = command()?;
    let output = command.arg("--help").output()?;
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.contains("--allow-write"));
    assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}

#[test]
fn version_should_report_the_package_version_without_creating_a_library() -> TestResult {
    let (directory, mut command) = command()?;
    let output = command.arg("--version").output()?;
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)?.contains(env!("CARGO_PKG_VERSION")));
    assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}

#[test]
fn configure_should_print_read_only_setup_without_creating_a_library() -> TestResult {
    let (directory, mut command) = command()?;
    let output = command.args(["configure", "codex"]).output()?;
    assert!(output.status.success());
    let setup = String::from_utf8(output.stdout)?;
    assert!(setup.starts_with("codex mcp add carver -- "));
    assert!(!setup.contains("--allow-write"));
    assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}

#[test]
fn configure_should_forward_explicit_write_access_into_json_arguments() -> TestResult {
    let (directory, mut command) = command()?;
    let output = command
        .args(["configure", "vscode", "--allow-write"])
        .output()?;
    assert!(output.status.success());
    let setup: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(setup["servers"]["carver"]["type"], "stdio");
    let arguments = setup["servers"]["carver"]["args"]
        .as_array()
        .ok_or("argument list")?;
    assert_eq!(
        arguments
            .iter()
            .filter(|value| **value == "--allow-write")
            .count(),
        1
    );
    assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}

#[test]
fn configure_should_preserve_the_other_client_alias() -> TestResult {
    let (_directory, mut command) = command()?;
    let output = command.args(["configure", "other"]).output()?;
    assert!(output.status.success());
    let setup: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(setup["transport"], "stdio");
    Ok(())
}
