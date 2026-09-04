//! Pure state transitions for the GTK frontend.
//!
//! GTK callbacks translate widget input into [`AppMsg`]. The reducer in this module updates
//! [`AppModel`] and returns typed [`Effect`] values for the runtime to execute.

mod effect;
mod model;
mod msg;
mod runtime;
mod update;

pub use effect::Effect;
pub use model::{
    AppModel, BrowserModel, EditorSessionId, LoadState, Preferences, RequestId, Route, TimerId,
    UiError,
};
pub use msg::{
    AppMsg, BrowserMsg, EditorMsg, LibraryReply, NavigationMsg, PreferencesMsg, SidebarMsg,
    TrashMsg,
};
pub use runtime::AppRuntime;
pub use update::update;

#[cfg(test)]
pub(crate) mod tests;
