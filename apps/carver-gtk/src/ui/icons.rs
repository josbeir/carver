//! Shared icon-theme fallbacks for native formatting controls.

use gtk::prelude::*;

pub(crate) fn formatting_glyph(icon: &str) -> Option<&'static str> {
    match icon {
        "format-text-highlight-symbolic" => Some("H"),
        "format-text-quote-symbolic" => Some("❝"),
        "format-text-superscript-symbolic" => Some("Aˣ"),
        "format-text-subscript-symbolic" => Some("Aₓ"),
        _ => None,
    }
}

pub(crate) fn available(icon: &str) -> bool {
    gtk::gdk::Display::default()
        .is_some_and(|display| gtk::IconTheme::for_display(&display).has_icon(icon))
}

pub(crate) fn palette_icon(icon: &str) -> gtk::Widget {
    let widget = if available(icon) {
        gtk::Image::from_icon_name(icon).upcast::<gtk::Widget>()
    } else if let Some(glyph) = formatting_glyph(icon) {
        let label = gtk::Label::new(Some(glyph));
        label.add_css_class("format-fallback-glyph");
        label.upcast::<gtk::Widget>()
    } else {
        gtk::Image::from_icon_name("document-edit-symbolic").upcast::<gtk::Widget>()
    };
    widget.set_size_request(24, 24);
    widget.set_valign(gtk::Align::Center);
    widget
}
