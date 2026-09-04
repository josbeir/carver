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
    ActionKey, AppModel, BrowserModel, EditorSessionId, LoadState, MoveUndo, Preferences,
    RequestId, Resource, Route, TimerId, UiError,
};
pub use msg::{
    ActionMsg, AppMsg, BrowserMsg, EditorMsg, LibraryReply, NavigationMsg, PreferencesMsg,
    SidebarMsg, TrashMsg, TrashMutation,
};
pub use runtime::AppRuntime;
pub use update::update;

#[cfg(test)]
pub(crate) mod tests;
