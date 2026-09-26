//! Display-backed export dialogs, PDF export, and copy-note coverage.
use super::*;

pub(super) fn export_dialogs_should_validate_and_print(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let gtk_window = fixture.window.clone().upcast::<gtk::Window>();
    let root = fixture.root()?;
    let source = fixture.source()?;
    let editor_surface =
        widget_as::<adw::ToolbarView>(&root, "editor-surface").ok_or("editor surface")?;
    let export_request = crate::mvu::EditorExportDialogRequest {
        html_profile: carver_domain::rendering::HtmlProfile::Enhanced,
        request_id: 91,
        session: crate::mvu::EditorSessionId(1),
        note_id: note.id,
        source: "# Exported note".to_owned(),
        filename_stem: "Exported note".to_owned(),
    };
    let export_options = crate::ui::editor::show_export_options_dialog(
        export_request,
        Some(&gtk_window),
        crate::mvu::AppDispatcher::default(),
    );
    let export_format =
        widget_as::<adw::ComboRow>(export_options.upcast_ref(), "export-format-setting")
            .ok_or("export format")?;
    let export_assets =
        widget_as::<adw::SwitchRow>(export_options.upcast_ref(), "export-assets-setting")
            .ok_or("export assets")?;
    assert_eq!(export_format.title(), "Format");
    assert_eq!(export_format.model().map(|model| model.n_items()), Some(4));
    assert_eq!(export_assets.title(), "Include managed files");
    export_format.set_selected(2);
    export_assets.set_active(true);
    assert!(export_assets.is_sensitive());
    export_format.set_selected(3);
    assert!(!export_assets.is_sensitive());
    assert!(!export_assets.is_active());
    export_options.emit_by_name::<()>("response", &[&"cancel"]);
    let export_warning = crate::ui::editor::show_export_warning_dialog(
        &crate::mvu::EditorExportWarningRequest {
            request_id: 92,
            session: crate::mvu::EditorSessionId(1),
            warnings: vec!["Markdown cannot preserve this construct".to_owned()],
        },
        Some(&gtk_window),
        crate::mvu::AppDispatcher::default(),
    );
    export_warning.emit_by_name::<()>("response", &[&"cancel"]);
    let export_directory = tempfile::tempdir()?;
    let pdf_path = export_directory.path().join("exported-note.pdf");
    let pdf_uri = gtk::gio::File::for_path(&pdf_path).uri().to_string();
    crate::ui::editor::export_rendered_snapshot(
        "::: toc\n:::\n\n# Exported note\n\n::: details \"More\"\nPDF body\n:::",
        false,
        false,
        &pdf_uri,
        Some(&gtk_window),
        None,
        carver_sdk::NoteId::new(),
        crate::mvu::AppDispatcher::default(),
        93,
        carver_domain::rendering::HtmlProfile::Enhanced,
    );
    let print_preview = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
        .filter(|window| window.transient_for().as_ref() == Some(&gtk_window))
        .find_map(|window| window.child().and_downcast::<webkit6::WebView>())
        .ok_or("PDF preview")?;
    assert_web_script_should_be_true(
        &print_preview,
        "Boolean(document.querySelector('nav.toc')) && document.querySelector('details')?.open === true && document.body.textContent.includes('PDF body')",
    );
    assert!(run_main_context_until(|| {
        std::fs::read(&pdf_path).is_ok_and(|bytes| bytes.starts_with(b"%PDF"))
    }));
    assert_native_print_dialog_cancels_without_invalid_window(&gtk_window)?;
    source.buffer().set_text("# Copied note");
    assert!(
        editor_surface
            .activate_action("editor.copy-note", None::<&glib::Variant>)
            .is_ok(),
        "the editor should expose a copy action without a header button"
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
    let copied_text = std::rc::Rc::new(std::cell::RefCell::new(None));
    let copied_text_for_callback = std::rc::Rc::clone(&copied_text);
    clipboard.read_text_async(None::<&gtk::gio::Cancellable>, move |result| {
        *copied_text_for_callback.borrow_mut() = result.ok().flatten().map(|text| text.to_string());
    });
    assert!(run_main_context_until(|| copied_text.borrow().is_some()));
    assert_eq!(copied_text.borrow().as_deref(), Some("Copied note\n"));
    Ok(())
}
