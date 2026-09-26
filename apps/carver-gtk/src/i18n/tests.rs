//! Tests for locale-directory resolution.

use std::{ffi::OsString, path::PathBuf};

use super::locate_localedir_from;

#[test]
fn override_directory_takes_precedence_over_flatpak() {
    let directory = locate_localedir_from(Some(OsString::from("/tmp/carver-locale")), true);

    assert_eq!(directory, PathBuf::from("/tmp/carver-locale"));
}

#[test]
fn flatpak_uses_app_localedir_without_override() {
    let directory = locate_localedir_from(None, true);

    assert_eq!(directory, PathBuf::from("/app/share/locale"));
}

#[test]
fn system_install_uses_usr_localedir() {
    let directory = locate_localedir_from(None, false);

    assert_eq!(directory, PathBuf::from("/usr/share/locale"));
}
