//! Carver's GTK application entry point.

#![forbid(unsafe_code)]

mod app;
pub mod mvu;
mod ui;
pub mod view;

fn main() -> glib::ExitCode {
    app::run()
}
