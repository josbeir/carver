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
    let rich =
        widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich editor")?;
    assert_web_script_should_be_true(
        &rich,
        "(() => { const panel = document.querySelector('.carve-div.details'); return panel && getComputedStyle(panel).borderRadius === '10px' && getComputedStyle(panel.querySelector('.admonition-title')).borderBottomWidth === '1px' && panel.querySelector('.carve-div-body').textContent.includes('https://example.com'); })()",
    );
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
            "getComputedStyle(document.querySelector('details')).borderRadius === '10px' && parseFloat(getComputedStyle(document.querySelector('nav.toc')).borderInlineStartWidth) > 1",
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

pub(super) fn code_fences_should_be_highlighted_in_previews_and_source() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    let category = fixture.client.create_category("Highlighting")?;
    let note = fixture.client.create_note(category.id)?;
    let source = "{.diff}\n```js\n  let icon = 1;\n- icon.add(\"a\");\n+ icon.remove(\"a\");\n```\n\n````\n```js\ninner\n```\n````\n\n# After\n\n```carve\n## Nested\n```\n";
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
        // Navigation reinstalls its script on every load; the highlighter must survive that.
        assert_web_script_should_be_true(
            &view,
            "(() => { const rows = document.querySelectorAll('pre.diff .carver-diff-line'); return rows.length >= 3 && Boolean(rows[0].querySelector('.hljs-keyword')) && rows[1].classList.contains('carver-diff-remove') && rows[2].classList.contains('carver-diff-add') && Boolean(document.querySelector('code.language-carve .hljs-section')); })()",
        );
    }

    let source_view =
        widget_as::<sourceview5::View>(&fixture.surface, "source-editor").ok_or("source")?;
    let buffer = source_view
        .buffer()
        .downcast::<sourceview5::Buffer>()
        .map_err(|_| "source buffer")?;
    buffer.ensure_highlight(&buffer.start_iter(), &buffer.end_iter());
    let line = |index: i32| buffer.iter_at_line(index).ok_or("line");
    assert!(buffer.iter_has_context_class(&line(2)?, "carve-code-block"));
    // The inner ``` of the four-backtick fence must not close it.
    assert!(buffer.iter_has_context_class(&line(10)?, "carve-code-block"));
    assert!(buffer.iter_has_context_class(&line(13)?, "carve-heading"));
    assert!(!buffer.iter_has_context_class(&line(13)?, "carve-code-block"));
    assert!(buffer.iter_has_context_class(&line(16)?, "carve-code-line"));
    assert!(buffer.iter_has_context_class(&line(16)?, "carve-heading"));
    fixture.window.close();
    Ok(())
}

pub(super) fn code_blocks_should_anchor_the_picker_and_keep_diff_lines_inline() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    let category = fixture.client.create_category("Code blocks")?;
    let note = fixture.client.create_note(category.id)?;
    let source = "```php\necho $var;\n```\n\n{.diff}\n```php\n  echo 'hello world';\n- echo $var;\n+ echo $var;\n```\n";
    let saved = fixture.client.save_note(note.id, note.revision, source)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: saved.id,
        revision: saved.revision,
        source: saved.source.clone(),
    }));
    let rich =
        widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich editor")?;
    // CarveKit renders the picker in `.carve-code-block-chrome` after the
    // scrollable <pre>, so it must be anchored to the block's top-right corner
    // rather than take a row of its own below the code.
    assert_web_script_should_be_true(
        &rich,
        "(() => { const block = document.querySelector('.carve-code-block'); const chrome = block?.querySelector(':scope > .carve-code-block-chrome'); if (!block || !chrome || !chrome.querySelector('select.carve-code-lang')) return false; const style = getComputedStyle(chrome); const blockRect = block.getBoundingClientRect(); const chromeRect = chrome.getBoundingClientRect(); return style.position === 'absolute' && Math.abs(chromeRect.right - blockRect.right) <= 12 && chromeRect.top - blockRect.top <= 12; })()",
    );
    // Diff decorations are inline in the editor. Block rows are a preview-only
    // treatment; in the editor token decorations split each line decoration, so
    // `display: block` would turn every token fragment into its own row.
    assert_web_script_should_be_true(
        &rich,
        "(() => { const lines = document.querySelectorAll('.ProseMirror .carver-diff-line'); return lines.length >= 2 && [...lines].every((line) => getComputedStyle(line).display !== 'block'); })()",
    );
    fixture.window.close();
    Ok(())
}
