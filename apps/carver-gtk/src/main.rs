//! Carver's GTK application entry point.

#![forbid(unsafe_code)]

mod app;
mod browser;
mod dialogs;
mod editor;
mod formatting;
pub mod mvu;
mod sidebar;
mod trash;
pub mod view;

fn main() -> glib::ExitCode {
    app::run()
}
#[cfg(test)]
mod tests;
