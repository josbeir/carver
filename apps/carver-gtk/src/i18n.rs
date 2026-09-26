//! Locale and gettext initialization.
//!
//! The application exchanges user-facing text with GNU gettext through
//! [`gettextrs`]. Strings are marked at their call sites with `gettext`,
//! `ngettext`, and `pgettext`; interpolated templates go through `tr_fmt!`.
//! Catalogs live in `po/` and are compiled into `.mo` files at package time.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain};

/// The gettext message domain and the name of the compiled catalog.
pub(crate) const DOMAIN: &str = "io.github.josbeir.Carver";

/// Initializes the process locale and binds the gettext message domain.
///
/// Must run once before any translatable string is looked up. The locale
/// itself is established by GTK during `gtk_init` (called from
/// `Application::run`); this function binds the domain and its UTF-8 catalog
/// directory. `setlocale` is an `unsafe` call and the workspace forbids
/// `unsafe`, so the process locale is never set here.
pub(crate) fn init() {
    let localedir = locate_localedir();
    if let Err(error) = bindtextdomain(DOMAIN, &localedir) {
        eprintln!(
            "carver: could not bind the text domain to {}: {error}",
            localedir.display()
        );
    }
    if let Err(error) = bind_textdomain_codeset(DOMAIN, "UTF-8") {
        eprintln!("carver: could not set the text domain encoding: {error}");
    }
    if let Err(error) = textdomain(DOMAIN) {
        eprintln!("carver: could not select the text domain: {error}");
    }
}

/// Formats a translated runtime template with the given arguments.
///
/// The `.po` tooling only sees strings that are passed to `gettext` directly,
/// so always translate first and format second:
///
/// ```ignore
/// tr_fmt!(gettext("Moved {count} notes"), count = count)
/// ```
///
/// A malformed translation falls back to the untranslated template rather than
/// failing at runtime; `msgfmt -c` catches placeholder mismatches at build time.
#[macro_export]
macro_rules! tr_fmt {
    ($template:expr $(, $($args:tt)*)?) => {{
        let template = $template;
        formatx::formatx!(template.as_str() $(, $($args)*)?).unwrap_or_else(|_| template)
    }};
}

/// Resolves the message catalog directory for system, Flatpak, `AppImage`, and
/// source-tree development installs.
fn locate_localedir() -> PathBuf {
    let override_dir =
        std::env::var_os("CARVER_LOCALEDIR").or_else(|| std::env::var_os("TEXTDOMAINDIR"));
    locate_localedir_from(override_dir, is_flatpak())
}

/// Pure locale-directory selection shared by [`locate_localedir`].
fn locate_localedir_from(override_dir: Option<OsString>, flatpak: bool) -> PathBuf {
    if let Some(directory) = override_dir {
        return PathBuf::from(directory);
    }
    if flatpak {
        return PathBuf::from("/app/share/locale");
    }
    PathBuf::from("/usr/share/locale")
}

/// Reports whether the process is running inside a Flatpak sandbox.
fn is_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists()
}

#[cfg(test)]
mod tests;
