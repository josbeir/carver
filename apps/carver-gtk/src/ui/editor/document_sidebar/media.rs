//! Media-specific cards and file actions.
use crate::mvu::{AppDispatcher, AppMsg, EditorMsg};
use gtk::prelude::*;
use std::path::Path;

pub(super) fn render_media_list(
    list: &gtk::ListBox,
    media: &[carver_domain::source_analysis::MediaOccurrence],
    dispatcher: &AppDispatcher,
    files: &std::collections::BTreeMap<String, Option<crate::mvu::MediaFile>>,
    thumbnails: &std::collections::BTreeMap<String, gtk::gdk::Texture>,
    session: crate::mvu::EditorSessionId,
    generation: u64,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for (index, item) in media.iter().enumerate() {
        let row = gtk::ListBoxRow::new();
        row.set_activatable(false);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(8);
        content.set_margin_end(8);
        let icon = gtk::Image::new();
        icon.set_pixel_size(48);
        icon.set_size_request(48, 48);
        let (content_type, _) = gtk::gio::content_type_guess(Some(Path::new(&item.path)), None);
        icon.set_from_gicon(&gtk::gio::content_type_get_symbolic_icon(&content_type));
        let file = files.get(&item.path).and_then(Option::as_ref);
        if item.kind == carver_domain::source_analysis::MediaKind::Image
            && let Some(texture) = thumbnails.get(&item.path)
        {
            icon.set_paintable(Some(texture));
            let thumbnail = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            thumbnail.add_css_class("media-thumbnail");
            thumbnail.set_overflow(gtk::Overflow::Hidden);
            thumbnail.set_valign(gtk::Align::Center);
            thumbnail.append(&icon);
            content.append(&thumbnail);
        } else {
            content.append(&icon);
        }
        let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
        labels.set_valign(gtk::Align::Center);
        labels.set_hexpand(true);
        let title = gtk::Label::new(Some(&item.label));
        title.add_css_class("media-filename");
        title.set_halign(gtk::Align::Start);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_hexpand(true);
        let size = file.map_or_else(
            || String::from("Unavailable"),
            |file| glib::format_size(file.size).to_string(),
        );
        let subtitle = gtk::Label::new(Some(&size));
        subtitle.set_widget_name("editor-media-size");
        subtitle.add_css_class("dim-label");
        subtitle.set_halign(gtk::Align::Start);
        subtitle.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        labels.append(&title);
        labels.append(&subtitle);
        content.append(&labels);
        let button = gtk::Button::new();
        button.set_widget_name("editor-media-item");
        button.add_css_class("flat");
        button.set_child(Some(&content));
        button.set_hexpand(true);
        let preview = gtk::Button::from_icon_name("view-reveal-symbolic");
        preview.set_widget_name("editor-media-preview");
        preview.add_css_class("flat");
        preview.set_valign(gtk::Align::Center);
        preview.set_tooltip_text(Some("Preview file"));
        preview.update_property(&[gtk::accessible::Property::Label("Preview file")]);
        preview.set_sensitive(item.path.starts_with("assets/"));
        let preview_dispatcher = dispatcher.clone();
        let preview_selection = item.range.clone();
        preview.connect_clicked(move |_| {
            let _ = preview_dispatcher.dispatch(AppMsg::Editor(EditorMsg::PreviewMedia {
                selection: preview_selection.clone(),
            }));
        });
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        actions.append(&button);
        actions.append(&preview);
        row.set_child(Some(&actions));
        let target = carver_editor_protocol::DocumentTarget::Media {
            path: item.path.clone(),
            occurrence: media[..index]
                .iter()
                .filter(|previous| previous.path == item.path)
                .count(),
        };
        super::connect_activation(&button, dispatcher, session, generation, target);
        list.append(&row);
    }
}
