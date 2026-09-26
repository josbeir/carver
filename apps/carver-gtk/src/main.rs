//! Carver's GTK application entry point.

#![forbid(unsafe_code)]

#[macro_use]
mod i18n;
mod app;
pub mod mvu;
mod ui;
pub mod view;

fn main() -> glib::ExitCode {
    i18n::init();
    app::run()
}
