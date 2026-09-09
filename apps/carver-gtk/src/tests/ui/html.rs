//! Display-backed preview and copy regressions for HTML-aware image rewriting.

use super::*;
use crate::mvu::{AppMsg, EditorMsg};

pub(super) fn preview_and_copy_should_preserve_source_with_quoted_image_attributes() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    let source_text =
        "# Images\n\n```=html\n<IMG alt='&lt;b&gt; &amp; A > B' SRC='assets&#47;missing.png'>\n```";
    let category = fixture.client.create_category("Images")?;
    let created = fixture.client.create_note(category.id)?;
    let saved = fixture
        .client
        .save_note(created.id, created.revision, source_text)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: saved.id,
        revision: saved.revision,
        source: source_text.into(),
    }));
    let root = &fixture.surface;
    let mode =
        widget_as::<gtk::ToggleButton>(root, "editor-mode-rendered").ok_or("preview mode")?;
    mode.set_active(true);
    let preview =
        widget_as::<webkit6::WebView>(root, "editor-rendered-preview").ok_or("preview")?;
    assert_web_script_should_be_true(
        &preview,
        "document.querySelector('img')?.getAttribute('src') === 'carver-asset:///assets/missing.png' && document.querySelector('img')?.alt === '<b> & A > B'",
    );
    let copy = widget_as::<gtk::Button>(root, "copy-note-button").ok_or("copy note")?;
    copy.emit_clicked();
    let clipboard = copy.display().clipboard();
    assert!(run_main_context_until(|| clipboard
        .formats()
        .contain_mime_type("text/html")));
    let copied = Rc::new(std::cell::RefCell::new(None));
    let result = Rc::clone(&copied);
    glib::MainContext::default().spawn_local(async move {
        let html = async {
            let (stream, _) = clipboard
                .read_future(&["text/html"], glib::Priority::DEFAULT)
                .await?;
            let mut html = Vec::new();
            loop {
                let bytes = stream
                    .read_bytes_future(4096, glib::Priority::DEFAULT)
                    .await?;
                if bytes.is_empty() {
                    break;
                }
                html.extend_from_slice(&bytes);
            }
            Ok::<_, glib::Error>(String::from_utf8_lossy(&html).into_owned())
        }
        .await;
        *result.borrow_mut() = Some(html);
    });
    assert!(run_main_context_until(|| copied.borrow().is_some()));
    let html = copied.borrow_mut().take().ok_or("clipboard response")??;
    assert!(html.contains("<span>[Image: &lt;b&gt; &amp; A &gt; B]</span>"));
    assert!(!html.contains("<img") && !html.contains("<IMG"));
    let source_mode =
        widget_as::<gtk::ToggleButton>(root, "editor-mode-source").ok_or("source mode")?;
    source_mode.set_active(true);
    let source = widget_as::<sourceview5::View>(root, "source-editor").ok_or("source editor")?;
    assert_eq!(
        crate::ui::editor::buffer_text(&source.buffer()),
        source_text
    );
    let reopened = fixture.client.note(saved.id)?.ok_or("persisted note")?;
    assert_eq!(reopened.source, saved.source);
    assert_eq!(reopened.revision, saved.revision);
    assert_eq!(reopened.updated_at, saved.updated_at);
    fixture.window.close();
    Ok(())
}
