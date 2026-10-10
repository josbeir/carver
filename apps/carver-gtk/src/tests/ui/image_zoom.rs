//! Display-backed image viewing without document or persistence mutations.

use super::document_sidebar::SidebarFixture;
use super::*;
use crate::mvu::{AppMsg, EditorMsg, EditorSaveState};
use carver_sdk::Note;

fn fixture() -> Result<(SidebarFixture, Note), Box<dyn std::error::Error>> {
    let fixture = document_sidebar::fixture_with_managed_assets()?;
    let image = gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 200, 100)
        .ok_or("image fixture")?;
    image.fill(0x33aa_66ff);
    let category = fixture.client.create_category("Image zoom")?;
    let created = fixture.client.create_note(category.id)?;
    let path =
        fixture
            .client
            .store_asset(created.id, "png", &image.save_to_bufferv("png", &[])?)?;
    let source = format!("# Images\n\n![First]({path}){{width=\"50%\"}}\n\n![Second]({path})");
    let saved = fixture
        .client
        .save_note(created.id, created.revision, &source)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: saved.id,
        revision: saved.revision,
        source: saved.source.clone(),
    }));
    Ok((fixture, saved))
}

fn assert_unchanged(fixture: &SidebarFixture, saved: &Note) -> TestResult {
    let model = fixture.runtime.model();
    let document = model.editor.as_ref().ok_or("editor document")?;
    assert_eq!(document.source, saved.source);
    assert_eq!(document.save_state, EditorSaveState::Clean);
    let reopened = fixture.client.note(saved.id)?.ok_or("persisted note")?;
    assert_eq!(reopened.source, saved.source);
    assert_eq!(reopened.revision, saved.revision);
    assert_eq!(reopened.updated_at, saved.updated_at);
    Ok(())
}

fn open_second_image(view: &webkit6::WebView) {
    assert_web_script_should_be_true(
        view,
        r"(() => {
        const buttons = document.querySelectorAll('.image-zoom-button');
        if (buttons.length !== 2 || buttons[1].hidden) return false;
        buttons[1].focus();
        buttons[1].click();
        const dialog = document.querySelector('.image-zoom-dialog');
        const image = dialog.querySelector('img');
        const rect = image.getBoundingClientRect();
        const close = dialog.querySelector('button').getBoundingClientRect();
        return dialog.open && image.alt === 'Second'
            && Math.abs(rect.width / rect.height - 2) < 0.01
            && rect.width > 200 && rect.left >= 31 && rect.right <= innerWidth - 31
            && rect.top >= 119 && rect.bottom <= innerHeight - 31
            && Math.abs(close.top - (innerWidth - close.right)) < 1
            && close.top >= 63 && close.right <= innerWidth - 63
            && document.activeElement === dialog.querySelector('button');
    })()",
    );
}

pub(super) fn rich_image_zoom_should_preserve_selection_source_and_saved_size() -> TestResult {
    let (fixture, saved) = fixture()?;
    let mode = widget_as::<adw::ViewStack>(&fixture.surface, "editor-mode-stack").ok_or("mode")?;
    let rich =
        widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich editor")?;
    mode.set_visible_child_name("rich");
    assert_web_script_should_be_true(
        &rich,
        r"(() => {
        const image = document.querySelector('#editor img[src]');
        if (!image?.complete || !image.naturalWidth) return false;
        if (!window.carverEditor.focusMedia(image.getAttribute('src'), 0)) return false;
        window.zoomSelection = window.carverEditor.editor.state.selection.toJSON();
        return image.style.width === '50%' && !document.querySelector('#editor button.image-zoom-button');
    })()",
    );
    open_second_image(&rich);
    assert_web_script_should_be_true(
        &rich,
        r"(() => {
        const dialog = document.querySelector('.image-zoom-dialog');
        dialog.dispatchEvent(new Event('cancel', {cancelable:true}));
        return !dialog.open && document.activeElement === document.querySelectorAll('.image-zoom-button')[1]
            && document.querySelector('#editor img[src]').style.width === '50%';
    })()",
    );
    // WebKit rebuilds the DOM range when native focus changes. Tiptap's model
    // is the stable authored selection that must survive viewing an image.
    assert_web_script_should_be_true(
        &rich,
        r"(() => {
        window.carverEditor.focus();
        const selection = window.carverEditor.editor.state.selection;
        return document.activeElement === window.carverEditor.editor.view.dom
            && selection.node?.type.name === 'image'
            && JSON.stringify(selection.toJSON()) === JSON.stringify(window.zoomSelection);
    })()",
    );
    assert_unchanged(&fixture, &saved)?;
    fixture.window.close();
    Ok(())
}

pub(super) fn preview_image_zoom_should_close_on_backdrop_and_mode_switch() -> TestResult {
    let (fixture, saved) = fixture()?;
    let mode = widget_as::<adw::ViewStack>(&fixture.surface, "editor-mode-stack").ok_or("mode")?;
    let preview = widget_as::<webkit6::WebView>(&fixture.surface, "editor-rendered-preview")
        .ok_or("preview")?;
    mode.set_visible_child_name("rendered");
    open_second_image(&preview);
    assert_web_script_should_be_true(
        &preview,
        r"(() => {
        const dialog = document.querySelector('.image-zoom-dialog');
        dialog.querySelector('img').click();
        if (!dialog.open) return false;
        dialog.click();
        return !dialog.open;
    })()",
    );
    open_second_image(&preview);
    mode.set_visible_child_name("rich");
    assert_web_script_should_be_true(
        &preview,
        "!document.querySelector('.image-zoom-dialog').open",
    );
    mode.set_visible_child_name("rendered");
    assert_web_script_should_be_true(
        &preview,
        "!document.querySelector('.image-zoom-dialog').open",
    );
    assert_unchanged(&fixture, &saved)?;
    fixture.window.close();
    Ok(())
}

pub(super) fn split_preview_image_zoom_should_close_when_split_is_hidden() -> TestResult {
    let (fixture, saved) = fixture()?;
    let split = widget_as::<gtk::ToggleButton>(&fixture.surface, "source-split-toggle")
        .ok_or("split toggle")?;
    let preview = widget_as::<webkit6::WebView>(&fixture.surface, "source-split-preview")
        .ok_or("split preview")?;
    split.set_active(true);
    open_second_image(&preview);
    assert_web_script_should_be_true(
        &preview,
        r"(() => {
        const dialog = document.querySelector('.image-zoom-dialog');
        dialog.querySelector('button').click();
        return !dialog.open;
    })()",
    );
    open_second_image(&preview);
    split.set_active(false);
    assert_web_script_should_be_true(
        &preview,
        "!document.querySelector('.image-zoom-dialog').open",
    );
    assert_unchanged(&fixture, &saved)?;
    fixture.window.close();
    Ok(())
}

pub(super) fn image_zoom_should_dismiss_when_another_note_loads() -> TestResult {
    let (fixture, saved) = fixture()?;
    let mode = widget_as::<adw::ViewStack>(&fixture.surface, "editor-mode-stack").ok_or("mode")?;
    let rich =
        widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich editor")?;
    let other = fixture.client.create_note(saved.category_id)?;
    for name in ["rich", "rendered"] {
        fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
            note_id: saved.id,
            revision: saved.revision,
            source: saved.source.clone(),
        }));
        mode.set_visible_child_name(name);
        let view = if name == "rich" {
            rich.clone()
        } else {
            widget_as::<webkit6::WebView>(&fixture.surface, "editor-rendered-preview")
                .ok_or("preview")?
        };
        open_second_image(&view);
        fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
            note_id: other.id,
            revision: other.revision,
            source: other.source.clone(),
        }));
        assert_web_script_should_be_true(
            &view,
            "!document.querySelector('.image-zoom-dialog')?.open && !document.querySelector('.image-zoom-button')",
        );
    }
    let reopened = fixture.client.note(saved.id)?.ok_or("original note")?;
    assert_eq!(reopened.source, saved.source);
    assert_eq!(reopened.revision, saved.revision);
    assert_eq!(reopened.updated_at, saved.updated_at);
    fixture.window.close();
    Ok(())
}
