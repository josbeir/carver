//! Maps pointer activations to workspace note-open intents.

use crate::mvu::{AppDispatcher, AppMsg, NavigationMsg, NoteOpenIntent};
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;

/// Resolves the open intent from pointer modifiers and the pressed button.
///
/// A plain primary click follows the configured default. Ctrl/Cmd or the middle
/// mouse button opens in a background tab, and Ctrl/Cmd+Shift forces a new
/// foreground tab.
pub(crate) fn note_open_intent(modifiers: gdk::ModifierType, button: u32) -> NoteOpenIntent {
    if button == gdk::BUTTON_MIDDLE {
        return NoteOpenIntent::Background;
    }
    match (
        modifiers.contains(gdk::ModifierType::CONTROL_MASK),
        modifiers.contains(gdk::ModifierType::SHIFT_MASK),
    ) {
        (true, true) => NoteOpenIntent::NewTab,
        (true, false) => NoteOpenIntent::Background,
        _ => NoteOpenIntent::Default,
    }
}

/// Opens a note with pointer modifiers, leaving plain activation to the widget.
pub(crate) fn connect_modified_note_open(
    widget: &impl IsA<gtk::Widget>,
    dispatcher: &AppDispatcher,
    note_id: carver_sdk::NoteId,
) {
    connect_modified_note_open_with(widget, dispatcher, move || Some(note_id));
}

/// Like [`connect_modified_note_open`], resolving the note when the press fires.
///
/// A capture-phase legacy controller reads the real button event, so the intent
/// is taken from the actual modifiers instead of gesture state. Modified presses
/// are consumed (`Stop`), which prevents the widget's own activation from also
/// opening the note in the foreground; plain presses fall through unchanged.
/// List rows are recycled, so the browser resolves the target note from the
/// row's current content instead of a value captured at bind time.
pub(crate) fn connect_modified_note_open_with(
    widget: &impl IsA<gtk::Widget>,
    dispatcher: &AppDispatcher,
    note_id: impl Fn() -> Option<carver_sdk::NoteId> + 'static,
) {
    let dispatcher = dispatcher.clone();
    let controller = gtk::EventControllerLegacy::new();
    controller.set_name(Some("note-modifier-open-controller"));
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    controller.connect_event(move |_, event| {
        let Some(button) = event.downcast_ref::<gdk::ButtonEvent>() else {
            return glib::Propagation::Proceed;
        };
        if button.event_type() != gdk::EventType::ButtonPress {
            return glib::Propagation::Proceed;
        }
        let intent = note_open_intent(button.modifier_state(), button.button());
        if matches!(intent, NoteOpenIntent::Default) {
            return glib::Propagation::Proceed;
        }
        let Some(note_id) = note_id() else {
            return glib::Propagation::Proceed;
        };
        let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id,
            intent,
        }));
        glib::Propagation::Stop
    });
    widget.add_controller(controller);
}

#[cfg(test)]
mod tests;
