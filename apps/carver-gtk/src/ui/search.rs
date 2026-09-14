//! Shared native search-control wiring for top-level list surfaces.

use gtk::prelude::*;

use crate::mvu::{AppDispatcher, AppMsg};

/// Native controls used to expose a contextual full-text search.
pub(crate) struct SearchControls {
    pub(crate) bar: gtk::SearchBar,
    pub(crate) entry: gtk::SearchEntry,
    pub(crate) toggle: gtk::ToggleButton,
}

/// Builds a search toggle and its matching native search bar.
pub(crate) fn build_search_controls(
    prefix: &str,
    placeholder: &str,
    tooltip: &str,
) -> SearchControls {
    let toggle = gtk::ToggleButton::new();
    toggle.set_widget_name(&format!("{prefix}-search-toggle"));
    toggle.set_icon_name("system-search-symbolic");
    toggle.set_tooltip_text(Some(tooltip));
    toggle.add_css_class("flat");
    let bar = gtk::SearchBar::new();
    bar.set_widget_name(&format!("{prefix}-search-bar"));
    bar.set_size_request(0, -1);
    bar.set_show_close_button(true);
    let entry = gtk::SearchEntry::new();
    entry.set_widget_name(&format!("{prefix}-search-entry"));
    entry.set_placeholder_text(Some(placeholder));
    entry.set_hexpand(true);
    bar.set_child(Some(&entry));
    bar.connect_entry(&entry);
    SearchControls { bar, entry, toggle }
}

/// Translates native search control signals into the owning surface's MVU messages.
pub(crate) fn connect_search_controls<Changed, Visibility>(
    dispatcher: &AppDispatcher,
    controls: &SearchControls,
    changed: Changed,
    opened: AppMsg,
    visibility: Visibility,
) where
    Changed: Fn(String) -> AppMsg + 'static,
    Visibility: Fn(bool) -> AppMsg + Clone + 'static,
{
    let dispatcher_for_entry = dispatcher.clone();
    controls.entry.connect_changed(move |entry| {
        let _ = dispatcher_for_entry.dispatch(changed(entry.text().to_string()));
    });
    let dispatcher_for_toggle = dispatcher.clone();
    let visibility_for_toggle = visibility.clone();
    controls.toggle.connect_toggled(move |toggle| {
        let message = if toggle.is_active() {
            opened.clone()
        } else {
            visibility_for_toggle(false)
        };
        let _ = dispatcher_for_toggle.dispatch(message);
    });
    let dispatcher_for_bar = dispatcher.clone();
    let visibility_for_bar = visibility.clone();
    controls.bar.connect_search_mode_enabled_notify(move |bar| {
        let _ = dispatcher_for_bar.dispatch(visibility_for_bar(bar.is_search_mode()));
    });
    let dispatcher_for_stop = dispatcher.clone();
    controls.entry.connect_stop_search(move |_| {
        let _ = dispatcher_for_stop.dispatch(visibility(false));
    });
}

/// Captures Ctrl+F before child widgets consume it and dispatches a contextual search message.
pub(crate) fn install_search_shortcut(
    widget: &impl IsA<gtk::Widget>,
    dispatcher: &AppDispatcher,
    name: &str,
    message: AppMsg,
) {
    let controller = gtk::EventControllerKey::new();
    controller.set_name(Some(name));
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let dispatcher = dispatcher.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        if key != gtk::gdk::Key::f
            || !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            || modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK)
        {
            return glib::Propagation::Proceed;
        }
        let _ = dispatcher.dispatch(message.clone());
        glib::Propagation::Stop
    });
    widget.add_controller(controller);
}
