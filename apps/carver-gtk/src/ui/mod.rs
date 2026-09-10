//! GTK widget boundaries for the application window.

pub(crate) mod add;
pub(crate) mod bases;
pub(crate) mod browser;
pub(crate) mod dialogs;
pub(crate) mod editor;
pub(crate) mod formatting;
pub(crate) mod sidebar;
pub(crate) mod trash;

#[cfg(test)]
#[path = "../tests/mod.rs"]
pub(crate) mod tests;
