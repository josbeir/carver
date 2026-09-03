//! Source-editor image paste support.

use std::rc::Rc;

use gtk::prelude::*;
use libadwaita as adw;

use crate::{controller::AppState, formatting};

/// Installs Ctrl+V image paste support for the Carve source editor.
///
/// The browser-backed rich editor owns its image paste integration. Keeping
/// this handler source-only prevents a GTK `TextBuffer` from acting as a
/// second, lossy rich-text document model.
pub(crate) fn install_image_paste(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::EventControllerKey {
    let controller = gtk::EventControllerKey::new();
    let state = Rc::clone(state);
    let buffer = buffer.clone();
    let toast_overlay = toast_overlay.clone();
    let clipboard = view.display().clipboard();
    controller.connect_key_pressed(move |_controller, key, _keycode, modifiers| {
        if key != gtk::gdk::Key::v || !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let state = Rc::clone(&state);
        let buffer = buffer.clone();
        let toast_overlay = toast_overlay.clone();
        clipboard.read_texture_async(None::<&gtk::gio::Cancellable>, move |result| {
            let Ok(Some(texture)) = result else {
                return;
            };
            let Some(note) = state.current_note.borrow().clone() else {
                return;
            };
            let client = state.client.clone();
            let bytes = texture.save_to_png_bytes().as_ref().to_vec();
            glib::spawn_future_local(async move {
                match client
                    .store_asset_async(note.id, String::from("png"), bytes)
                    .await
                {
                    Ok(path) => buffer.insert_at_cursor(&format!("\n![Pasted image]({path})\n")),
                    Err(error) => toast_overlay.add_toast(adw::Toast::new(&format!(
                        "Could not store pasted image: {error}"
                    ))),
                }
            });
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
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
    on_insert: impl Fn(String, String) + 'static,
) -> gtk::DropTarget {
    use glib::types::StaticType;

    let target = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY,
    );
    let state = Rc::clone(state);
    let toast_overlay = toast_overlay.clone();
    let on_insert: Rc<dyn Fn(String, String)> = Rc::new(on_insert);
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
                &state,
                &toast_overlay,
                Rc::clone(&on_insert),
                extension,
            );
        }
        true
    });
    view.add_controller(target.clone());
    target
}
