//! Source-editor image paste support.

use super::source_commands;
use gtk::prelude::*;

use super::super::formatting;
use crate::mvu::{AppDispatcher, AppMsg, EditorMsg};

/// Installs Ctrl+V image paste support for the Carve source editor.
///
/// The browser-backed rich editor owns its image paste integration. Keeping
/// this handler source-only prevents a GTK `TextBuffer` from acting as a
/// second, lossy rich-text document model.
pub(crate) fn install_image_paste(
    view: &gtk::TextView,
    dispatcher: &AppDispatcher,
    rich: &super::RichEditor,
) -> gtk::EventControllerKey {
    let controller = gtk::EventControllerKey::new();
    let dispatcher = dispatcher.clone();
    let clipboard = view.display().clipboard();
    let source_buffer = view.buffer();
    let rich = rich.clone();
    controller.connect_key_pressed(move |_controller, key, _keycode, modifiers| {
        if key != gtk::gdk::Key::v || !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let Some(session) = rich.document_session() else {
            return glib::Propagation::Proceed;
        };
        let dispatcher = dispatcher.clone();
        let source_target = source_commands::image_target_from_buffer(&source_buffer);
        clipboard.read_texture_async(None::<&gtk::gio::Cancellable>, move |result| {
            let Ok(Some(texture)) = result else {
                return;
            };
            let bytes = texture.save_to_png_bytes().as_ref().to_vec();
            let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ImportImageRead {
                bytes,
                target: crate::mvu::ImportTarget {
                    session,
                    source: Some(source_target),
                },
            }));
        });
        glib::Propagation::Proceed
    });
    view.add_controller(controller.clone());
    controller
}

/// Installs managed file drag and drop for an editing surface.
///
/// GTK owns native file drops, including `WebKit` drops that only expose a URI
/// to JavaScript. Routing both source and rich editors through this handler
/// keeps local paths out of the persisted document.
pub(crate) fn install_image_drop(
    view: &impl IsA<gtk::Widget>,
    dispatcher: &AppDispatcher,
    rich: &super::RichEditor,
) -> gtk::DropTarget {
    use glib::types::StaticType;

    let target = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY,
    );
    let dispatcher = dispatcher.clone();
    let rich = rich.clone();
    let source_view = view.as_ref().downcast_ref::<gtk::TextView>().cloned();
    target.connect_drop(move |_target, value, _x, _y| {
        let Ok(files) = value.get::<gtk::gdk::FileList>() else {
            return false;
        };
        let files = files.files();
        if files.is_empty() {
            return false;
        }
        let Some(session) = rich.document_session() else {
            return false;
        };
        let source = source_view
            .as_ref()
            .map(|view| source_commands::image_target_from_buffer(&view.buffer()));
        formatting::import_managed_files(
            &files,
            &dispatcher,
            crate::mvu::ImportTarget { session, source },
        );
        true
    });
    view.add_controller(target.clone());
    target
}
