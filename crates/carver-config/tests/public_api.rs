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

#[test]
fn save_should_ignore_a_preexisting_legacy_temporary_path() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    let legacy = path.with_extension("toml.tmp");
    std::fs::create_dir(&legacy)?;
    let config = Config::default();
    save(&path, &config)?;
    assert_eq!(load(&path)?, config);
    assert!(legacy.is_dir());
    Ok(())
}

#[test]
fn concurrent_saves_should_commit_complete_configurations_without_tempfile_collisions()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let writers: Vec<_> = (0..8)
            .map(|index| {
                let path = &path;
                let barrier = &barrier;
                scope.spawn(move || {
                    let mut config = Config::default();
                    config.editor.autosave_delay_ms = 1_000 + index;
                    barrier.wait();
                    for _ in 0..10 {
                        save(path, &config)?;
                    }
                    Ok::<_, carver_config::ConfigError>(())
                })
            })
            .collect();
        for writer in writers {
            writer
                .join()
                .map_err(|_| "configuration writer panicked")??;
        }
        Ok(())
    })?;
    let saved = load(&path)?;
    assert!((1_000..1_008).contains(&saved.editor.autosave_delay_ms));
    assert_eq!(std::fs::read_dir(directory.path())?.count(), 1);
    Ok(())
}
