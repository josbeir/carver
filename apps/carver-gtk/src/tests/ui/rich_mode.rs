//! Display-backed rich editor clipboard and file-drop coverage.
use super::*;

pub(super) fn assert_rich_selection_copy_should_publish_portable_content(
    source: &gtk::TextView,
    source_mode: &gtk::ToggleButton,
    rich_mode: &gtk::ToggleButton,
    rich: &webkit6::WebView,
) -> TestResult {
    for text in ["first projection", "# Portable selection"] {
        source_mode.set_active(true);
        source.buffer().set_text(text);
        rich_mode.set_active(true);
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
    source_mode: &gtk::ToggleButton,
) -> TestResult {
    let directory = tempfile::tempdir()?;
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    std::fs::write(&first, b"first contents")?;
    std::fs::write(&second, b"second contents")?;
    source_mode.set_active(true);
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
    let source_mode = fixture.source_mode()?;
    let rich_mode = fixture.rich_mode()?;
    let find_bar = fixture.find_bar()?;
    let find_entry = fixture.find_entry()?;
    let find_count = fixture.find_count()?;
    let find_close = widget_as::<gtk::Button>(&root, "editor-find-close").ok_or("find close")?;
    source.buffer().set_text("rich find target");
    rich_mode.set_active(true);
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
    assert_native_file_drop_should_insert_an_ordered_batch(&source, &source_mode)?;
    source_mode.set_active(true);
    source
        .buffer()
        .set_text("![First](assets/first.png){width=\"50%\"}");
    rich_mode.set_active(true);
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
    assert_rich_selection_copy_should_publish_portable_content(
        &source,
        &source_mode,
        &rich_mode,
        &rich,
    )?;
    Ok(())
}
