//! Carver's GTK application entry point.

#![forbid(unsafe_code)]

mod app;
mod browser;
mod controller;
mod dialogs;
mod editor;
mod formatting;
mod note_move;
mod sidebar;
mod trash;

fn main() -> glib::ExitCode {
    app::run()
}
#[cfg(test)]
mod tests;
