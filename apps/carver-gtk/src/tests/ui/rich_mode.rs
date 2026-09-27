//! Display-backed rich editor clipboard and file-drop coverage.
use super::*;

pub(super) fn assert_rich_selection_copy_should_publish_portable_content(
    source: &gtk::TextView,
    editor_stack: &adw::ViewStack,
    rich: &webkit6::WebView,
) -> TestResult {
    for text in ["first projection", "# Portable selection"] {
        editor_stack.set_visible_child_name("source");
        source.buffer().set_text(text);
        editor_stack.set_visible_child_name("rich");
        assert_web_script_should_be_true(
            rich,
            &format!(
                "window.carverEditor?.source() === {}",
                serde_json::to_string(text)?
            ),
        );
    }

    assert_web_script_should_be_true(
        rich,
        "(() => { const editor = window.carverEditor?.editor; if (!editor) return false; editor.commands.selectAll(); return !editor.view.dom.dispatchEvent(new ClipboardEvent('copy', {bubbles:true, cancelable:true})); })()",
    );
    let clipboard = source.display().clipboard();
    assert!(run_main_context_until(|| {
        clipboard.formats().contain_mime_type("text/html")
            && clipboard
                .formats()
                .contain_mime_type("text/plain;charset=utf-8")
            && clipboard
                .formats()
                .contain_mime_type(crate::ui::editor::CARVER_CLIPBOARD_MIME)
    }));
    let copied_text = Rc::new(std::cell::RefCell::new(None));
    let copied_text_for_callback = Rc::clone(&copied_text);
    clipboard.read_text_async(None::<&gtk::gio::Cancellable>, move |result| {
        *copied_text_for_callback.borrow_mut() = result.ok().flatten().map(|text| text.to_string());
    });
    assert!(run_main_context_until(|| copied_text.borrow().is_some()));
    assert_eq!(
        copied_text.borrow().as_deref(),
        Some("Portable selection\n")
    );
    Ok(())
}

pub(super) fn assert_native_file_drop_should_insert_an_ordered_batch(
    source: &gtk::TextView,
    editor_stack: &adw::ViewStack,
) -> TestResult {
    let directory = tempfile::tempdir()?;
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    std::fs::write(&first, b"first contents")?;
    std::fs::write(&second, b"second contents")?;
    editor_stack.set_visible_child_name("source");
    let buffer = source.buffer();
    buffer.set_text("Before replace After");
    buffer.select_range(&buffer.iter_at_offset(7), &buffer.iter_at_offset(14));
    let controllers = source.observe_controllers();
    let target = (0..controllers.n_items())
        .find_map(|index| controllers.item(index).and_downcast::<gtk::DropTarget>())
        .ok_or("source drop target")?;
    let files = gtk::gdk::FileList::from_array(&[
        gtk::gio::File::for_path(first),
        gtk::gio::File::for_path(second),
    ]);
    assert!(target.emit_by_name::<bool>(
        "drop",
        &[&glib::BoxedValue(files.to_value()), &0.0_f64, &0.0_f64]
    ));
    assert!(run_main_context_until(|| {
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        text.starts_with("Before [first.txt](assets/")
            && text.contains(")\n[second.txt](assets/")
            && text.ends_with(") After")
    }));
    Ok(())
}

pub(super) fn rich_editor_should_round_trip_and_preserve_media(
    fixture: &WindowFixture,
) -> TestResult {
    let root = fixture.root()?;
    let source = fixture.source()?;
    let editor_stack = fixture.editor_mode_stack()?;
    let find_bar = fixture.find_bar()?;
    let find_entry = fixture.find_entry()?;
    let find_count = fixture.find_count()?;
    let find_close = widget_as::<gtk::Button>(&root, "editor-find-close").ok_or("find close")?;
    source.buffer().set_text("rich find target");
    editor_stack.set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich editor")?;
    let rich_source = std::rc::Rc::new(std::cell::RefCell::new(None));
    let rich_source_for_callback = std::rc::Rc::clone(&rich_source);
    rich.evaluate_javascript(
        "window.carverEditor?.source() ?? null",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            *rich_source_for_callback.borrow_mut() =
                result.ok().map(|value| value.to_str().clone());
        },
    );
    assert!(run_main_context_until(|| rich_source.borrow().is_some()));
    assert_eq!(rich_source.borrow().as_deref(), Some("rich find target"));
    find_bar.set_search_mode(true);
    find_entry.set_text("target");
    assert!(run_main_context_until(|| find_count.text() == "1 match"));
    find_close.emit_clicked();
    assert!(!find_bar.is_search_mode());
    let bold = widget_as::<gtk::ToggleButton>(&root, "format-bold-button").ok_or("bold")?;
    bold.grab_focus();
    bold.emit_clicked();
    assert!(run_main_context_until(|| {
        bold.is_active() && widget_is_window_focus(rich.upcast_ref())
    }));
    bold.grab_focus();
    bold.emit_clicked();
    assert!(run_main_context_until(|| {
        !bold.is_active() && widget_is_window_focus(rich.upcast_ref())
    }));
    assert_native_file_drop_should_insert_an_ordered_batch(&source, &editor_stack)?;
    editor_stack.set_visible_child_name("source");
    source
        .buffer()
        .set_text("![First](assets/first.png){width=\"50%\"}");
    editor_stack.set_visible_child_name("rich");
    let image_width = std::rc::Rc::new(std::cell::RefCell::new(None));
    let image_width_callback = std::rc::Rc::clone(&image_width);
    rich.evaluate_javascript(
        "document.querySelector('#editor img')?.style.width ?? null",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            *image_width_callback.borrow_mut() = result.ok().map(|value| value.to_str().clone());
        },
    );
    assert!(run_main_context_until(|| image_width.borrow().is_some()));
    assert_eq!(image_width.borrow().as_deref(), Some("50%"));
    rich.evaluate_javascript(
        "window.carverEditor.insertImage('assets/second.png')",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
    assert!(run_main_context_until(|| source
        .buffer()
        .text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        )
        .contains("assets/second.png")));
    assert_rich_selection_copy_should_publish_portable_content(&source, &editor_stack, &rich)?;
    Ok(())
}

pub(super) fn short_rich_document_should_not_scroll_the_writing_surface(
    fixture: &WindowFixture,
) -> TestResult {
    let root = fixture.root()?;
    let source = fixture.source()?;
    let editor_stack = fixture.editor_mode_stack()?;
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich editor")?;
    // The final block's trailing margin must stay inside `.ProseMirror`, so a
    // short or empty document never grows past the `WebKit` viewport.
    for document in ["", "# Title\n\nA short body."] {
        editor_stack.set_visible_child_name("source");
        source.buffer().set_text(document);
        editor_stack.set_visible_child_name("rich");
        assert_web_script_should_be_true(
            &rich,
            "(() => { const surface = document.querySelector('.ProseMirror'); if (!surface || window.innerHeight === 0) return false; return document.documentElement.scrollHeight <= window.innerHeight; })()",
        );
    }
    Ok(())
}

/// Returns the picker's dimension label, highlighted cell count, and whether
/// the grid action is currently enabled.
fn table_picker_highlight(root: &gtk::Widget) -> (String, usize, bool) {
    let Some(menu) = widget_as::<gtk::MenuButton>(root, "format-table-button") else {
        return (String::new(), 0, false);
    };
    let Some(content) = menu.popover().and_then(|popover| popover.child()) else {
        return (String::new(), 0, false);
    };
    let label = content
        .first_child()
        .and_downcast::<gtk::Label>()
        .map(|label| label.text().to_string())
        .unwrap_or_default();
    let Some(grid) = content
        .first_child()
        .and_then(|dimensions| dimensions.next_sibling())
        .and_downcast::<gtk::Grid>()
    else {
        return (label, 0, false);
    };
    let enabled = grid
        .first_child()
        .and_downcast::<gtk::Button>()
        .is_some_and(|cell| cell.is_sensitive());
    let mut selected = 0;
    let mut child = grid.first_child();
    while let Some(widget) = child {
        if widget.has_css_class("selected") {
            selected += 1;
        }
        child = widget.next_sibling();
    }
    (label, selected, enabled)
}

pub(super) fn rich_table_selection_should_update_the_picker(fixture: &WindowFixture) -> TestResult {
    let root = fixture.root()?;
    let source = fixture.source()?;
    let editor_stack = fixture.editor_mode_stack()?;
    editor_stack.set_visible_child_name("source");
    source
        .buffer()
        .set_text("|= a |= b |= c |\n| d | e | f |\n");
    editor_stack.set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich editor")?;
    assert_web_script_should_be_true(
        &rich,
        "document.querySelectorAll('.ProseMirror table th, .ProseMirror table td').length === 6",
    );
    // Drive the cursor into a real cell so the WebKit bridge publishes the
    // enclosing table to the native toolbar through the normal selection path.
    assert_web_script_should_be_true(
        &rich,
        "(() => { const editor = window.carverEditor?.editor; if (!editor) return false; let pos = null; editor.state.doc.descendants((node, position) => { if (pos === null && (node.type.name === 'tableCell' || node.type.name === 'tableHeader')) pos = position + 1; }); return pos !== null && editor.commands.setTextSelection(pos); })()",
    );
    assert!(run_main_context_until(|| {
        let (label, selected, enabled) = table_picker_highlight(&root);
        label == "2 × 3" && selected == 6 && enabled
    }));
    // Growing the table through the editor must be reflected on the next update.
    assert_web_script_should_be_true(
        &rich,
        "window.carverEditor?.editor?.chain().focus().addRowAfter().run() === true",
    );
    assert!(run_main_context_until(|| {
        let (label, selected, enabled) = table_picker_highlight(&root);
        label == "3 × 3" && selected == 9 && enabled
    }));
    // A table that outgrows the grid must disable the resize action instead of
    // offering a click that would drop trailing rows.
    assert_web_script_should_be_true(
        &rich,
        "(() => { const editor = window.carverEditor?.editor; if (!editor) return false; editor.chain().focus().addRowAfter().run(); editor.chain().focus().addRowAfter().run(); return true; })()",
    );
    assert!(run_main_context_until(|| {
        let (label, selected, enabled) = table_picker_highlight(&root);
        label == "5 × 3" && selected == 12 && !enabled
    }));
    Ok(())
}
