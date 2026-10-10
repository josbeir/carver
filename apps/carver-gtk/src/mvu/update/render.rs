//! Render invalidation for frequent editor messages, without copying document text.

use super::super::{AppModel, AppMsg, EditorMsg, Effect};

#[derive(PartialEq)]
struct Selection {
    heading: Option<usize>,
    media: Option<std::ops::Range<usize>>,
    formatting: carver_editor_protocol::SelectionState,
}

impl Selection {
    fn of(model: &AppModel) -> Self {
        let mut formatting = model.rich_selection.clone();
        // These identities reject stale web events but do not change any visible controls.
        formatting.revision = 0;
        formatting.navigation_epoch = 0;
        Self {
            heading: model
                .editor
                .as_ref()
                .and_then(|document| document.selected_heading),
            media: model
                .editor
                .as_ref()
                .and_then(|document| document.selected_media.clone()),
            formatting,
        }
    }
}

/// Reduces a message and identifies whether the runtime needs a new view snapshot.
pub(crate) fn update_for_view(model: &mut AppModel, message: AppMsg) -> (Vec<Effect>, bool) {
    let selection = matches!(
        &message,
        AppMsg::Editor(
            EditorMsg::SourceSelectionChanged { .. } | EditorMsg::DocumentSelectionChanged { .. }
        )
    )
    .then(|| Selection::of(model));
    let visible = match &message {
        // Timer bookkeeping needs no view pass; a SaveNote effect below renders the saving state.
        AppMsg::Editor(EditorMsg::AutosaveRequested | EditorMsg::AutosaveElapsed { .. }) => false,
        AppMsg::Editor(EditorMsg::PreviewElapsed { session, timer_id }) => {
            model.preview_timer == Some((*session, *timer_id))
                && model.editor.as_ref().is_some_and(|document| {
                    document.session == *session
                        && (document.mode == carver_config::EditorMode::Rendered
                            || (document.mode == carver_config::EditorMode::Source
                                && model.preferences.source_split_view))
                })
        }
        _ => true,
    };
    let effects = super::update(model, message);
    let changed = selection.map_or(visible, |before| before != Selection::of(model));
    // Post-reducer work can reveal media or deferred refresh state. Keep its normal render path.
    let other_work = effects
        .iter()
        .any(|effect| !matches!(effect, Effect::ScheduleEditorSave { .. }));
    (effects, changed || other_work)
}

#[cfg(test)]
mod tests;
