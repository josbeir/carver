//! Read-only preview occurrence navigation with per-load snapshot validation.
use crate::mvu::{AppDispatcher, AppMsg, EditorMsg, EditorSessionId};
use carver_editor_protocol::{DocumentTarget, EditorEvent};
use gtk::prelude::*;
use std::{cell::RefCell, rc::Rc};
use webkit6::prelude::*;

struct Snapshot {
    load: u64,
    navigation_epoch: u64,
    session: EditorSessionId,
    source: Rc<str>,
}

#[derive(Clone)]
pub(super) struct PreviewNavigation {
    view: webkit6::WebView,
    snapshot: Rc<RefCell<Snapshot>>,
}

impl PreviewNavigation {
    pub fn new(
        view: &webkit6::WebView,
        dispatcher: &AppDispatcher,
        mode: carver_config::EditorMode,
    ) -> Self {
        let snapshot = Rc::new(RefCell::new(Snapshot {
            load: 0,
            navigation_epoch: 0,
            session: EditorSessionId(0),
            source: Rc::from(""),
        }));
        if let Some(manager) = view.user_content_manager() {
            manager.register_script_message_handler("documentSelection", None);
            let current = Rc::clone(&snapshot);
            let dispatcher = dispatcher.clone();
            manager.connect_script_message_received(Some("documentSelection"), move |_, value| {
                let Some(bytes) = value.to_string_as_bytes() else {
                    return;
                };
                let Ok(EditorEvent::Selection { session, state }) =
                    serde_json::from_slice(bytes.as_ref())
                else {
                    return;
                };
                let source = {
                    let snapshot = current.borrow();
                    if snapshot.session.0 != session
                        || snapshot.load != state.revision
                        || snapshot.navigation_epoch != state.navigation_epoch
                    {
                        return;
                    }
                    Rc::clone(&snapshot.source)
                };
                let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::DocumentSelectionChanged {
                    session: EditorSessionId(session),
                    mode,
                    source,
                    media: state.media,
                    heading: state.heading_occurrence,
                }));
            });
        }
        Self {
            view: view.clone(),
            snapshot,
        }
    }

    pub fn set_document(&self, session: EditorSessionId, source: &str) {
        let load = {
            let mut snapshot = self.snapshot.borrow_mut();
            snapshot.load = snapshot.load.wrapping_add(1);
            snapshot.session = session;
            snapshot.navigation_epoch = 0;
            snapshot.source = Rc::from(source);
            snapshot.load
        };
        let Some(manager) = self.view.user_content_manager() else {
            return;
        };
        manager.remove_all_scripts();
        let script = include_str!("document_navigation.js")
            .replace("__SESSION__", &session.0.to_string())
            .replace("__LOAD__", &load.to_string());
        manager.add_script(&webkit6::UserScript::new(
            &script,
            webkit6::UserContentInjectedFrames::TopFrame,
            webkit6::UserScriptInjectionTime::End,
            &[],
            &[],
        ));
    }

    pub fn focus(&self, target: &DocumentTarget, source: &str, focus: bool) {
        let (load, epoch) = {
            let mut snapshot = self.snapshot.borrow_mut();
            if snapshot.source.as_ref() != source {
                return;
            }
            snapshot.navigation_epoch = snapshot.navigation_epoch.wrapping_add(1);
            (snapshot.load, snapshot.navigation_epoch)
        };
        let Ok(target) = serde_json::to_string(target) else {
            return;
        };
        let snapshot = Rc::clone(&self.snapshot);
        let view = self.view.clone();
        self.view.evaluate_javascript(
            &format!("window.carverDocumentNavigation?.focus({target}, {load}, {focus}, {epoch});"),
            None,
            Some("carver-editor:///bridge"),
            None::<&gtk::gio::Cancellable>,
            move |result| {
                let current = {
                    let current = snapshot.borrow();
                    current.load == load && current.navigation_epoch == epoch
                };
                if focus
                    && current
                    && view.is_mapped()
                    && result.is_ok_and(|value| value.to_boolean())
                {
                    view.grab_focus();
                    if let Some(root) = view.root() {
                        root.set_focus(Some(&view));
                    }
                }
            },
        );
    }
}
