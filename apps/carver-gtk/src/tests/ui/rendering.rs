//! Render preference changes must refresh both previews without modifying notes.
use super::*;
use crate::mvu::{AppMsg, EditorMsg, PreferencesMsg};

pub(super) fn rendering_preference_should_refresh_previews_without_saving() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    let category = fixture.client.create_category("Rendering")?;
    let note = fixture.client.create_note(category.id)?;
    let source = "::: toc\n:::\n\n# Heading\n\n::: details \"More\"\nhttps://example.com\n:::";
    let saved = fixture.client.save_note(note.id, note.revision, source)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: saved.id,
        revision: saved.revision,
        source: saved.source.clone(),
    }));
    for (mode, name) in [
        (
            carver_config::EditorMode::Rendered,
            "editor-rendered-preview",
        ),
        (carver_config::EditorMode::Source, "source-split-preview"),
    ] {
        fixture
            .runtime
            .dispatch(AppMsg::Preferences(PreferencesMsg::SetEditorMode(mode)));
        fixture
            .runtime
            .dispatch(AppMsg::Preferences(PreferencesMsg::SetSourceSplitView(
                true,
            )));
        let view = widget_as::<webkit6::WebView>(&fixture.surface, name).ok_or("preview")?;
        fixture.runtime.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetEnhancedCarveRendering(true),
        ));
        assert_web_script_should_be_true(
            &view,
            "Boolean(document.querySelector('nav.toc a[href=\"#Heading\"]') && document.querySelector('details a[href=\"https://example.com\"]'))",
        );
        assert_web_script_should_be_true(
            &view,
            "document.querySelector('details summary').click(); document.querySelector('details').open",
        );
        assert_web_script_should_be_true(
            &view,
            "document.querySelector('nav.toc a').click(); true",
        );
        assert_web_script_should_be_true(
            &view,
            "location.hash === '#Heading' && Boolean(document.querySelector('h1'))",
        );
        fixture.runtime.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetEnhancedCarveRendering(false),
        ));
        assert_web_script_should_be_true(
            &view,
            "Boolean(document.querySelector('h1')) && !document.querySelector('nav.toc, details, a[href=\"https://example.com\"]')",
        );
    }
    assert_eq!(
        fixture.runtime.model().editor.ok_or("editor")?.source,
        saved.source
    );
    let reopened = fixture.client.note(note.id)?.ok_or("saved note")?;
    assert_eq!(reopened.revision, saved.revision);
    assert_eq!(reopened.updated_at, saved.updated_at);
    fixture.window.close();
    Ok(())
}
