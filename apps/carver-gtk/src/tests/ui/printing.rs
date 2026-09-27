//! Display-backed PDF export and native print dialog coverage.
use super::*;

pub(super) fn assert_pdf_page_setup() -> TestResult {
    let page_setup = crate::ui::editor::pdf_page_setup();

    if page_setup.orientation() != gtk::PageOrientation::Portrait
        || (page_setup.paper_width(gtk::Unit::Mm) - 210.0).abs() >= f64::EPSILON
        || (page_setup.paper_height(gtk::Unit::Mm) - 297.0).abs() >= f64::EPSILON
        || (page_setup.top_margin(gtk::Unit::Mm) - 18.0).abs() >= f64::EPSILON
        || (page_setup.bottom_margin(gtk::Unit::Mm) - 18.0).abs() >= f64::EPSILON
        || (page_setup.left_margin(gtk::Unit::Mm) - 18.0).abs() >= f64::EPSILON
        || (page_setup.right_margin(gtk::Unit::Mm) - 18.0).abs() >= f64::EPSILON
    {
        return Err("PDF export should use A4 portrait paper with 18 mm margins".into());
    }

    Ok(())
}

/// The file printer is registered under a localized name, so export must resolve it by
/// capability rather than the English "Print to File" string.
#[cfg(target_os = "linux")]
pub(super) fn assert_print_to_file_printer_resolves() -> TestResult {
    if crate::ui::editor::print_to_file_printer_name().is_none() {
        return Err("GTK file print backend printer should be discoverable".into());
    }
    Ok(())
}

/// Accepts the native print dialog for the file backend and asserts a PDF is written.
#[cfg(target_os = "linux")]
pub(super) fn assert_native_print_dialog_should_print_to_file(parent: &gtk::Window) -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("native-print.pdf");
    let uri = gtk::gio::File::for_path(&path).uri().to_string();
    let printer =
        crate::ui::editor::print_to_file_printer_name().ok_or("file print backend printer")?;

    let parent_weak = parent.downgrade();
    let attempts = Rc::new(Cell::new(0_u8));
    let attempts_for_timeout = Rc::clone(&attempts);
    glib::timeout_add_local(Duration::from_millis(20), move || {
        let Some(parent) = parent_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let dialog = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .find(|candidate| {
                candidate.transient_for().is_some_and(|host| {
                    host.transient_for()
                        .is_some_and(|ancestor| ancestor == parent)
                })
            });
        if let Some(dialog) = dialog {
            if let Ok(print_dialog) = dialog.clone().downcast::<gtk::PrintUnixDialog>() {
                let settings = gtk::PrintSettings::new();
                settings.set(gtk::PRINT_SETTINGS_PRINTER, Some(printer.as_str()));
                settings.set(gtk::PRINT_SETTINGS_OUTPUT_URI, Some(uri.as_str()));
                settings.set(gtk::PRINT_SETTINGS_OUTPUT_FILE_FORMAT, Some("pdf"));
                print_dialog.set_settings(Some(&settings));
                print_dialog.emit_by_name::<()>("response", &[&i32::from(gtk::ResponseType::Ok)]);
            } else {
                dialog.close();
            }
            return glib::ControlFlow::Break;
        }
        if attempts_for_timeout.get() == 50 {
            return glib::ControlFlow::Break;
        }
        attempts_for_timeout.update(|attempt| attempt + 1);
        glib::ControlFlow::Continue
    });

    crate::ui::editor::export_rendered_snapshot(
        "# Native print\n\nBody",
        false,
        true,
        "",
        Some(parent),
        None,
        carver_sdk::NoteId::new(),
        crate::mvu::AppDispatcher::default(),
        95,
        carver_domain::rendering::HtmlProfile::Enhanced,
    );

    if !run_main_context_until_for(Duration::from_secs(10), || {
        std::fs::read(&path).is_ok_and(|bytes| bytes.starts_with(b"%PDF"))
    }) {
        return Err(format!("native print did not write a PDF to {}", path.display()).into());
    }
    Ok(())
}

pub(super) fn assert_native_print_dialog_cancels_without_invalid_window(
    parent: &gtk::Window,
) -> TestResult {
    let cancelled = Rc::new(Cell::new(false));
    let cancelled_for_timeout = Rc::clone(&cancelled);
    let parent_weak = parent.downgrade();
    let attempts = Rc::new(Cell::new(0_u8));
    let attempts_for_timeout = Rc::clone(&attempts);
    glib::timeout_add_local(Duration::from_millis(20), move || {
        let Some(parent) = parent_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let dialog = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .find(|candidate| {
                candidate.transient_for().is_some_and(|host| {
                    host.transient_for()
                        .is_some_and(|ancestor| ancestor == parent)
                })
            });
        if let Some(dialog) = dialog {
            cancelled_for_timeout.set(true);
            if let Ok(print_dialog) = dialog.clone().downcast::<gtk::PrintUnixDialog>() {
                print_dialog
                    .emit_by_name::<()>("response", &[&i32::from(gtk::ResponseType::Cancel)]);
            } else {
                dialog.close();
            }
            return glib::ControlFlow::Break;
        }
        if attempts_for_timeout.get() == 50 {
            return glib::ControlFlow::Break;
        }
        attempts_for_timeout.update(|attempt| attempt + 1);
        glib::ControlFlow::Continue
    });
    crate::ui::editor::export_rendered_snapshot(
        "# Printable note\n\nBody",
        false,
        true,
        "",
        Some(parent),
        None,
        carver_sdk::NoteId::new(),
        crate::mvu::AppDispatcher::default(),
        94,
        carver_domain::rendering::HtmlProfile::Enhanced,
    );
    if !run_main_context_until(|| cancelled.get()) {
        return Err("native print dialog did not appear".into());
    }
    Ok(())
}
