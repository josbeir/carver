//! Source-editor image paste support.

use super::source_commands;
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    formatting,
    mvu::{AppDispatcher, AppMsg, EditorMsg},
};

/// Installs Ctrl+V image paste support for the Carve source editor.
///
/// The browser-backed rich editor owns its image paste integration. Keeping
/// this handler source-only prevents a GTK `TextBuffer` from acting as a
/// second, lossy rich-text document model.
pub(crate) fn install_image_paste(
    view: &gtk::TextView,
    dispatcher: &AppDispatcher,
) -> gtk::EventControllerKey {
    let controller = gtk::EventControllerKey::new();
    let dispatcher = dispatcher.clone();
    let clipboard = view.display().clipboard();
    let source_buffer = view.buffer();
    controller.connect_key_pressed(move |_controller, key, _keycode, modifiers| {
        if key != gtk::gdk::Key::v || !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let dispatcher = dispatcher.clone();
        let source_selection = source_commands::selection_from_buffer(&source_buffer);
        clipboard.read_texture_async(None::<&gtk::gio::Cancellable>, move |result| {
            let Ok(Some(texture)) = result else {
                return;
            };
            let bytes = texture.save_to_png_bytes().as_ref().to_vec();
            let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ImportImage {
                extension: String::from("png"),
                bytes,
                alt: String::from("Pasted image"),
                source_selection: Some(source_selection),
            }));
        });
        glib::Propagation::Proceed
    });
    view.add_controller(controller.clone());
    controller
}

/// Installs managed image-file drag and drop for an editing surface.
///
/// GTK owns native file drops, including `WebKit` drops that only expose a URI
/// to JavaScript. Routing both source and rich editors through this handler
/// keeps local paths out of the persisted document.
pub(crate) fn install_image_drop(
    view: &impl IsA<gtk::Widget>,
    dispatcher: &AppDispatcher,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::DropTarget {
    use glib::types::StaticType;

    let target = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY,
    );
    let dispatcher = dispatcher.clone();
    let toast_overlay = toast_overlay.clone();
    target.connect_drop(move |_target, value, _x, _y| {
        let Ok(files) = value.get::<gtk::gdk::FileList>() else {
            return false;
        };
        let files = files.files();
        if files.is_empty() {
            return false;
        }
        for file in files {
            let Some(extension) = formatting::image_extension_for_file(&file) else {
                continue;
            };
            let alt = formatting::image_alt_for_file(&file);
            formatting::import_managed_image_file(
                &file,
                &alt,
                &dispatcher,
                &toast_overlay,
                extension,
            );
        }
        true
    });
    view.add_controller(target.clone());
    target
}
