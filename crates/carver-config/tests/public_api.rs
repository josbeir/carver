//! External-consumer contract tests for `carver-config`.

use carver_config::{Config, load, save};

#[test]
fn save_should_round_trip_a_public_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("nested/config.toml");
    let mut config = Config::default();
    config.editor.autosave_delay_ms = 1_200;

    save(&path, &config)?;

    assert_eq!(load(&path)?, config);
    Ok(())
}
