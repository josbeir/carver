use gtk::gdk::{self, ModifierType};

use super::note_open_intent;
use crate::mvu::NoteOpenIntent;

#[test]
fn plain_primary_click_should_follow_the_default() {
    assert_eq!(
        note_open_intent(ModifierType::empty(), gdk::BUTTON_PRIMARY),
        NoteOpenIntent::Default
    );
    assert_eq!(
        note_open_intent(ModifierType::SHIFT_MASK, gdk::BUTTON_PRIMARY),
        NoteOpenIntent::Default
    );
}

#[test]
fn modified_clicks_should_choose_the_tab_target() {
    assert_eq!(
        note_open_intent(ModifierType::CONTROL_MASK, gdk::BUTTON_PRIMARY),
        NoteOpenIntent::Background
    );
    assert_eq!(
        note_open_intent(
            ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK,
            gdk::BUTTON_PRIMARY
        ),
        NoteOpenIntent::NewTab
    );
}

#[test]
fn middle_click_should_open_in_the_background() {
    assert_eq!(
        note_open_intent(ModifierType::empty(), gdk::BUTTON_MIDDLE),
        NoteOpenIntent::Background
    );
    // The middle button wins even with a keyboard modifier held.
    assert_eq!(
        note_open_intent(ModifierType::CONTROL_MASK, gdk::BUTTON_MIDDLE),
        NoteOpenIntent::Background
    );
}
