//! Black-box startup coverage for the GTK application binary.

use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

#[test]
#[ignore = "requires the Weston harness and starts the GTK application process"]
fn application_should_create_an_isolated_library_on_startup()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let config_home = directory.path().join("config");
    let data_home = directory.path().join("data");
    let cache_home = directory.path().join("cache");
    let database = data_home.join("carver/library.sqlite3");
    let application_id = format!("io.github.josbeir.Carver.Test{}", std::process::id());
    let mut child = Command::new(env!("CARGO_BIN_EXE_carver-gtk"))
        .env("CARVER_APPLICATION_ID", application_id)
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_DATA_HOME", data_home)
        .env("XDG_CACHE_HOME", cache_home)
        // Headless CI does not provide desktop portal or accessibility services.
        .env("CARVER_DISABLE_PORTALS", "1")
        .env("GTK_A11Y", "none")
        .spawn()?;
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let mut started = false;
    while Instant::now() < deadline {
        if database.is_file() {
            started = true;
            break;
        }
        if let Some(status) = child.try_wait()? {
            return Err(format!("application exited before startup completed: {status}").into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();

    if started {
        Ok(())
    } else {
        Err("application did not create its database before the startup timeout".into())
    }
}
