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
            dialog.close();
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
