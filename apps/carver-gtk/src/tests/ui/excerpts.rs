//! Display-backed coverage of storage excerpts in native note cards.

use super::*;

pub(super) fn note_card_should_display_the_complete_final_grapheme() -> TestResult {
    let (_directory, client) = test_state()?;
    let expected = format!("{}👩🏽‍💻", "a".repeat(179));
    let source = format!("{expected}remaining");
    let category = client.create_category("Unicode")?;
    let created = client.create_note(category.id)?;
    let saved = client.save_note(created.id, created.revision, &source)?;
    let summary = client.recent_notes(None, 1, 0)?.pop().ok_or("summary")?;
    let details = crate::ui::browser::note_card_details(&summary, false, None);
    let window = gtk::Window::builder().child(&details).build();
    window.present();
    let label =
        widget_as::<gtk::Label>(details.upcast_ref(), &format!("note-excerpt:{}", saved.id))
            .ok_or("excerpt label")?;

    assert_eq!(label.text(), expected);
    let persisted = client.note(saved.id)?.ok_or("persisted note")?;
    assert_eq!(persisted.source, source);
    assert_eq!(persisted.revision, saved.revision);
    assert_eq!(persisted.updated_at, saved.updated_at);
    window.close();
    Ok(())
}
