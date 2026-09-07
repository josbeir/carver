//! Read-only preview selection messages for the Media sidebar.
use crate::mvu::{AppDispatcher, AppMsg, EditorMsg, EditorSessionId};
use webkit6::prelude::*;

pub(super) fn connect(
    view: &webkit6::WebView,
    dispatcher: &AppDispatcher,
    mode: carver_config::EditorMode,
) {
    let Some(manager) = view.user_content_manager() else {
        return;
    };
    manager.register_script_message_handler("mediaSelection", None);
    let dispatcher = dispatcher.clone();
    manager.connect_script_message_received(Some("mediaSelection"), move |_, value| {
        let Some(bytes) = value.to_string_as_bytes() else {
            return;
        };
        if let Ok(carver_editor_protocol::EditorEvent::Selection { session, state }) =
            serde_json::from_slice(bytes.as_ref())
        {
            let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::MediaSelected {
                session: EditorSessionId(session),
                mode,
                media: state.media,
            }));
        }
    });
}

pub(super) fn set_session(view: &webkit6::WebView, session: EditorSessionId) {
    let Some(manager) = view.user_content_manager() else {
        return;
    };
    manager.remove_all_scripts();
    let script = format!(
        r"
        (() => {{
            const pathFor = node => (node.getAttribute('src') ?? node.getAttribute('href') ?? '')
                .replace(/^carver-asset:\/\/\//, '');
            const report = event => {{
                const node = event.target.closest?.('img,a[href]');
                const path = node ? pathFor(node) : '';
                let media = null;
                if (node && (node.tagName === 'IMG' || path.startsWith('assets/'))) {{
                    const matches = [...document.querySelectorAll('img,a[href]')]
                        .filter(item => pathFor(item) === path &&
                            (item.tagName === 'IMG' || path.startsWith('assets/')));
                    media = {{path, occurrence: matches.indexOf(node)}};
                }}
                window.webkit.messageHandlers.mediaSelection.postMessage(JSON.stringify({{
                    type: 'selection', session: {},
                    state: {{active: [], heading: 0, image_width: null, media}}
                }}));
            }};
            document.addEventListener('click', report);
            document.addEventListener('focusin', report);
        }})();
    ",
        session.0
    );
    manager.add_script(&webkit6::UserScript::new(
        &script,
        webkit6::UserContentInjectedFrames::TopFrame,
        webkit6::UserScriptInjectionTime::End,
        &[],
        &[],
    ));
}
