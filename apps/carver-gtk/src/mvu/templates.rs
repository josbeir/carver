//! Template intents, validation, and pure dialog transitions.
use super::{AppModel, Effect, RequestId, UiError};
use carver_sdk::{Category, CategoryId, NoteId, NoteTemplate, Revision, TemplateId};
use gettextrs::gettext;

mod insertion;
mod preview;

/// Origin of an effective property for a new note.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemplatePropertyOrigin {
    /// Explicitly authored by the template.
    Template,
    /// Supplied by enabled default properties.
    Default,
    /// Replaces this default value, including explicit empty values.
    Override(String),
}
/// One effective new-note property with its source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateProperty {
    /// User-authored property key.
    pub key: String,
    /// Canonical display value.
    pub value: String,
    /// Source of this value.
    pub origin: TemplatePropertyOrigin,
}
/// Read-only content and resolved properties shown before creation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplatePreview {
    /// Canonical template source rendered read-only.
    pub source: String,
    /// Effective values using the same merge rules as note creation.
    pub properties: Vec<TemplateProperty>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PickerState {
    pub(crate) request_id: RequestId,
    pub(crate) category_id: CategoryId,
    pub(crate) templates: Vec<NoteTemplate>,
    pub(crate) selected: Option<TemplateId>,
    pub(crate) insertion: Option<InsertTarget>,
}

/// Editor snapshot captured before opening the insertion picker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsertTarget {
    /// Editor lifetime receiving the insert.
    pub session: super::EditorSessionId,
    /// Canonical source used to reject stale confirmations.
    pub source: String,
    /// Projection that owned the captured insertion point.
    pub mode: carver_config::EditorMode,
    /// Source-mode character selection; rich mode retains its projection selection.
    pub selection: std::ops::Range<usize>,
}

/// Purpose of loading the current template list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemplatePurpose {
    /// Manage reusable templates.
    Manage,
    /// Load choices for the shared creation form.
    NewCategory,
    /// Choose a template for a captured destination.
    Pick(CategoryId),
    /// Insert into the captured editor rather than creating a note.
    Insert(InsertTarget),
    /// Edit a category and its template assignment.
    Category(Category),
}
/// Template operations submitted by GTK or the SDK runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemplatesMsg {
    /// Refresh the timestamp at the host boundary, keeping the reducer deterministic.
    PatternClock(time::OffsetDateTime),
    /// Refresh default-template names independently of dialog requests.
    RefreshCatalog,
    /// A default-template name refresh completed.
    CatalogLoaded {
        /// Captured refresh identity.
        request_id: RequestId,
        /// Completed SDK result.
        result: Result<Vec<NoteTemplate>, UiError>,
    },
    /// Select a template for read-only preview, or clear selection.
    SelectPreview {
        /// Picker lifetime.
        request_id: RequestId,
        /// Chosen template.
        id: Option<TemplateId>,
    },
    /// Create the currently previewed template in the captured category.
    CreateSelected(RequestId),
    /// Refresh effective properties for an unsaved source draft.
    PreviewDraft {
        /// Editor lifetime.
        request_id: RequestId,
        /// Unsaved canonical source.
        source: String,
    },
    /// The picker closed.
    PickerClosed(RequestId),
    /// Open the template manager.
    Manage,
    /// Load choices for the Add dialog.
    NewCategory,
    /// Open a picker in the current category.
    Pick,
    /// Capture the current insertion point at the GTK boundary.
    Insert,
    /// Source selection captured before the template picker opens.
    InsertCaptured(std::ops::Range<usize>),
    /// Open a category's editing dialog.
    EditCategory(Category),
    /// Templates loaded for a dialog.
    Loaded {
        /// Identity guarding asynchronous completions.
        request_id: RequestId,
        /// Captured dialog destination and purpose.
        purpose: TemplatePurpose,
        /// Completed SDK operation result.
        result: Result<Vec<NoteTemplate>, UiError>,
    },
    /// Edit a template or start a blank one.
    Edit(Option<NoteTemplate>),
    /// Create a template from the current note's unsaved source.
    FromEditor,
    /// Create a template from an arbitrary note, preferring open editor content.
    FromNote(NoteId),
    /// A note loaded to seed a template.
    NoteLoaded {
        /// Identity guarding asynchronous completions.
        request_id: RequestId,
        /// Completed SDK operation result.
        result: Result<carver_sdk::Note, UiError>,
    },
    /// Duplicate a template into a new unsaved draft.
    Duplicate(NoteTemplate),
    /// A draft dialog closed without saving.
    EditorClosed(RequestId),
    /// Save the current draft.
    Save {
        /// Identity guarding asynchronous completions.
        request_id: RequestId,
        /// Original template and revision, or a new draft.
        original: Option<NoteTemplate>,
        /// User-entered template or category name.
        name: String,
        /// Canonical Carve source.
        source: String,
    },
    /// The source editor could not be opened.
    OpenFailed {
        /// Requested editor lifetime.
        request_id: RequestId,
        /// Localized opening failure.
        error: UiError,
    },
    /// Save completed.
    Saved {
        /// Identity guarding asynchronous completions.
        request_id: RequestId,
        /// Completed SDK operation result.
        result: Result<(), UiError>,
    },
    /// Delete a template after confirmation.
    Delete {
        /// Template identity.
        id: TemplateId,
        /// Expected persisted revision.
        revision: Revision,
    },
    /// Category update completed.
    CategorySaved(Result<(), UiError>),
    /// A delete completed.
    Changed {
        /// Deleted template identity for clearing category assignments in the snapshot.
        id: TemplateId,
        /// Completed persistence result.
        result: Result<(), UiError>,
    },
    /// Create an independent note from a chosen template.
    Create {
        /// Captured destination category.
        category_id: CategoryId,
        /// Selected template identity.
        template_id: TemplateId,
    },
    /// Save category fields and template assignment together.
    SaveCategory {
        /// Category being edited.
        category: Category,
        /// User-entered template or category name.
        name: String,
        /// Selected category appearance.
        appearance: carver_sdk::CategoryAppearance,
        /// Selected template identity.
        template_id: Option<TemplateId>,
    },
}

// CONTEXT: Keep template intent and completion routing together for stale-session guards.
#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive template message dispatch table"
)]
pub(super) fn update(model: &mut AppModel, message: TemplatesMsg) -> Vec<Effect> {
    match message {
        TemplatesMsg::PatternClock(now) => {
            model.template_pattern_time = now;
            Vec::new()
        }
        TemplatesMsg::RefreshCatalog => refresh_catalog(model).into_iter().collect(),
        TemplatesMsg::CatalogLoaded { request_id, result } => {
            if model.template_catalog.finish(request_id, result) {
                refresh_catalog(model).into_iter().collect()
            } else {
                Vec::new()
            }
        }
        TemplatesMsg::SelectPreview { request_id, id } => select_preview(model, request_id, id),
        TemplatesMsg::CreateSelected(request_id) => {
            if model
                .template_picker
                .as_ref()
                .is_some_and(|p| p.request_id == request_id && p.insertion.is_some())
            {
                return insertion::confirm(model);
            }
            let Some(picker) = &model.template_picker else {
                return Vec::new();
            };
            if picker.request_id != request_id {
                return Vec::new();
            }
            picker.selected.map_or_else(Vec::new, |template_id| {
                vec![Effect::CreateTemplateNote {
                    category_id: picker.category_id,
                    template_id,
                }]
            })
        }
        TemplatesMsg::PreviewDraft { request_id, source } => {
            if model.template_editor != Some(request_id) {
                return Vec::new();
            }
            vec![Effect::ShowDraftProperties {
                request_id,
                preview: preview::source_preview_at(
                    &source,
                    &model.config.document_properties,
                    &pattern_context(model, model.active_category_id()),
                ),
            }]
        }
        TemplatesMsg::PickerClosed(request_id) => {
            if model
                .template_picker
                .as_ref()
                .is_some_and(|p| p.request_id == request_id)
            {
                model.template_picker = None;
            }
            Vec::new()
        }
        TemplatesMsg::Manage => load(model, TemplatePurpose::Manage),
        TemplatesMsg::NewCategory => load(model, TemplatePurpose::NewCategory),
        TemplatesMsg::Pick => model
            .active_category_id()
            .map_or_else(Vec::new, |id| load(model, TemplatePurpose::Pick(id))),
        TemplatesMsg::Insert => {
            if model.editor.as_ref().is_some_and(|d| {
                d.mode != carver_config::EditorMode::Rendered && d.external_change.is_none()
            }) {
                vec![Effect::CaptureTemplateInsert]
            } else {
                Vec::new()
            }
        }
        TemplatesMsg::InsertCaptured(selection) => {
            let Some(document) = model
                .editor
                .as_ref()
                .filter(|d| d.mode != carver_config::EditorMode::Rendered)
            else {
                return Vec::new();
            };
            let target = InsertTarget {
                session: document.session,
                source: document.source.clone(),
                mode: document.mode,
                selection,
            };
            load(model, TemplatePurpose::Insert(target))
        }
        TemplatesMsg::EditCategory(category) => load(model, TemplatePurpose::Category(category)),
        TemplatesMsg::Loaded {
            request_id,
            purpose,
            result,
        } => {
            if model.template_request != Some(request_id) {
                return Vec::new();
            }
            match result {
                Ok(templates) => {
                    model.template_catalog.state = super::LoadState::Ready(templates.clone());
                    let mut initial_preview = None;
                    let mut selected_id = None;
                    if let TemplatePurpose::Pick(category_id) = &purpose {
                        let default_id = match &model.sidebar.state {
                            super::LoadState::Ready(categories) => categories
                                .iter()
                                .find(|c| c.category.id == *category_id)
                                .and_then(|c| c.category.default_template_id),
                            _ => None,
                        };
                        let selected = templates
                            .iter()
                            .find(|t| Some(t.id) == default_id)
                            .or_else(|| templates.first())
                            .map(|t| t.id);
                        selected_id = selected;
                        model.template_picker = Some(PickerState {
                            request_id,
                            category_id: *category_id,
                            insertion: None,
                            templates: templates.clone(),
                            selected,
                        });
                        initial_preview = selected
                            .and_then(|id| templates.iter().find(|t| t.id == id))
                            .map(|t| {
                                preview::source_preview_at(
                                    &t.source,
                                    &model.config.document_properties,
                                    &pattern_context(model, Some(*category_id)),
                                )
                            });
                    }
                    if let TemplatePurpose::Insert(target) = &purpose {
                        let selected = templates.first().map(|t| t.id);
                        selected_id = selected;
                        initial_preview = templates.first().map(|t| {
                            insertion::preview_at(
                                &t.source,
                                target,
                                &model.config.document_properties,
                                &pattern_context(
                                    model,
                                    model.editor.as_ref().map(|d| d.category_id),
                                ),
                            )
                        });
                        let Some(document) = model.editor.as_ref().filter(|d| {
                            d.session == target.session
                                && d.source == target.source
                                && d.mode == target.mode
                        }) else {
                            return Vec::new();
                        };
                        model.template_picker = Some(PickerState {
                            request_id,
                            category_id: document.category_id,
                            templates: templates.clone(),
                            selected,
                            insertion: Some(target.clone()),
                        });
                    }
                    vec![Effect::ShowTemplates {
                        selected: selected_id,
                        request_id,
                        templates,
                        purpose,
                        initial_preview,
                    }]
                }
                Err(error) => {
                    model.set_notice(error);
                    Vec::new()
                }
            }
        }
        TemplatesMsg::Edit(original) => {
            model.template_editor_from_note = false;
            let name = original
                .as_ref()
                .map_or_else(String::new, |t| t.name.clone());
            let source = original
                .as_ref()
                .map_or_else(String::new, |t| t.source.clone());
            edit(model, original, name, source)
        }
        TemplatesMsg::Duplicate(template) => {
            model.template_editor_from_note = false;
            edit(model, None, template.name, template.source)
        }
        TemplatesMsg::FromEditor => {
            model.template_editor_from_note = true;
            let Some(note) = &model.editor else {
                return Vec::new();
            };
            let source = note.source.clone();
            let name = carver_domain::derive_content(&source).title;
            edit(model, None, name, source)
        }
        TemplatesMsg::FromNote(id) => {
            model.template_editor_from_note = true;
            if let Some(note) = model
                .editor
                .as_ref()
                .filter(|n| n.note_id == id)
                .or_else(|| model.tabs.background.values().find(|n| n.note_id == id))
            {
                let source = note.source.clone();
                let name = carver_domain::derive_content(&source).title;
                return edit(model, None, name, source);
            }
            let request_id = model.next_request_id();
            model.template_request = Some(request_id);
            vec![Effect::LoadTemplateNote {
                request_id,
                note_id: id,
            }]
        }
        TemplatesMsg::NoteLoaded { request_id, result } => {
            if model.template_request != Some(request_id) {
                return Vec::new();
            }
            match result {
                Ok(note) => edit(model, None, note.title, note.source),
                Err(error) => {
                    model.set_notice(error);
                    Vec::new()
                }
            }
        }
        TemplatesMsg::EditorClosed(id) => {
            if model.template_editor != Some(id) {
                return Vec::new();
            }
            model.template_editor = None;
            if model.template_editor_from_note {
                Vec::new()
            } else {
                load(model, TemplatePurpose::Manage)
            }
        }
        TemplatesMsg::Save {
            request_id,
            original,
            name,
            source,
        } => {
            if model.template_editor != Some(request_id) {
                return Vec::new();
            }
            let result = if name.trim().is_empty() {
                Err(UiError::new(gettext("Enter a template name.")))
            } else {
                validate(&source, &model.config.document_properties.entries)
            };
            if let Err(error) = result {
                return vec![Effect::FinishTemplateEdit {
                    request_id,
                    error: Some(error),
                }];
            }
            vec![Effect::SaveTemplate {
                request_id,
                original,
                name: name.trim().to_owned(),
                source,
            }]
        }
        TemplatesMsg::OpenFailed { request_id, error } => {
            if model.template_editor == Some(request_id) {
                model.template_editor = None;
                model.set_notice(error);
            }
            Vec::new()
        }
        TemplatesMsg::Saved { request_id, result } => {
            if model.template_editor != Some(request_id) {
                return Vec::new();
            }
            match result {
                Ok(()) => {
                    model.template_editor = None;
                    let mut effects = vec![Effect::FinishTemplateEdit {
                        request_id,
                        error: None,
                    }];
                    if !model.template_editor_from_note {
                        effects.extend(load(model, TemplatePurpose::Manage));
                    }
                    effects.extend(refresh_catalog(model));
                    effects.extend(acknowledge_mutation(model));
                    effects
                }
                Err(error) => vec![Effect::FinishTemplateEdit {
                    request_id,
                    error: Some(error),
                }],
            }
        }
        TemplatesMsg::Delete { id, revision } => vec![Effect::DeleteTemplate { id, revision }],
        TemplatesMsg::Changed { id, result } => {
            if let Err(error) = result {
                model.set_notice(error);
                return Vec::new();
            }
            let mut effects = load(model, TemplatePurpose::Manage);
            if let super::LoadState::Ready(categories) = &mut model.sidebar.state {
                for summary in categories {
                    if summary.category.default_template_id == Some(id) {
                        summary.category.default_template_id = None;
                    }
                }
            } else {
                // Coalesce a follow-up if an earlier category read was already in flight.
                effects.extend(super::update::reload_sidebar(model));
            }
            effects.extend(acknowledge_mutation(model));
            effects
        }
        TemplatesMsg::CategorySaved(result) => {
            if let Err(error) = result {
                model.set_notice(error);
                return Vec::new();
            }
            let mut effects: Vec<_> = super::update::reload_sidebar(model).into_iter().collect();
            effects.extend(super::update::reload_browser(model));
            effects.extend(acknowledge_mutation(model));
            effects
        }
        TemplatesMsg::Create {
            category_id,
            template_id,
        } => vec![Effect::CreateTemplateNote {
            category_id,
            template_id,
        }],
        TemplatesMsg::SaveCategory {
            category,
            name,
            appearance,
            template_id,
        } => vec![Effect::SaveTemplateCategory {
            category_id: category.id,
            name,
            appearance,
            template_id,
        }],
    }
}
fn load(model: &mut AppModel, purpose: TemplatePurpose) -> Vec<Effect> {
    let request_id = model.next_request_id();
    model.template_request = Some(request_id);
    vec![Effect::LoadTemplates {
        request_id,
        purpose,
    }]
}
fn edit(
    model: &mut AppModel,
    original: Option<NoteTemplate>,
    name: String,
    source: String,
) -> Vec<Effect> {
    let request_id = model.next_request_id();
    model.template_editor = Some(request_id);
    vec![Effect::ShowTemplateEditor {
        properties: preview::source_preview_at(
            &source,
            &model.config.document_properties,
            &pattern_context(model, model.active_category_id()),
        ),
        preferences: model.preferences.source_editor.clone(),
        request_id,
        original,
        name,
        source,
    }]
}
pub(super) fn validate(
    source: &str,
    defaults: &[carver_config::DocumentProperty],
) -> Result<(), UiError> {
    carver_sdk::validate_configured_template(source, defaults).map_err(|error| {
        UiError::new(match error {
            carver_sdk::ConfiguredTemplateError::Source(carver_domain::TemplateError::Pattern(
                pattern,
            )) => tr_fmt!(
                gettext("Invalid template pattern: {pattern}"),
                pattern = pattern
            ),
            carver_sdk::ConfiguredTemplateError::Source(
                carver_domain::TemplateError::ManagedAssets,
            ) => gettext("Templates cannot contain managed images or attachments."),
            carver_sdk::ConfiguredTemplateError::Source(_) => {
                gettext("The template frontmatter is invalid or contains duplicate properties.")
            }
            carver_sdk::ConfiguredTemplateError::PropertyType(key) => tr_fmt!(
                gettext("Property {key} does not match its configured type."),
                key = key
            ),
        })
    })
}

// Catalog mutations do not modify notes. Record our own write before the next focus wakeup
// checks the shared library, without refreshing note content or browser pages.
fn acknowledge_mutation(model: &mut AppModel) -> Option<Effect> {
    super::update::request_library_revision(
        model,
        super::model::LibraryRevisionCheckReason::LocalTemplateMutation,
    )
}

pub(super) fn refresh_catalog(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .template_catalog
        .begin_reload(request_id)
        .then_some(Effect::LoadTemplateCatalog { request_id })
}

fn select_preview(
    model: &mut AppModel,
    request_id: RequestId,
    id: Option<TemplateId>,
) -> Vec<Effect> {
    let context = pattern_context(model, model.template_picker.as_ref().map(|p| p.category_id));
    let Some(picker) = &mut model.template_picker else {
        return Vec::new();
    };
    if picker.request_id != request_id {
        return Vec::new();
    }
    let template = id.and_then(|id| picker.templates.iter().find(|t| t.id == id));
    picker.selected = template.map(|t| t.id);
    vec![Effect::ShowTemplatePreview {
        request_id,
        preview: template.map(|t| {
            if let Some(target) = &picker.insertion {
                insertion::preview_at(
                    &t.source,
                    target,
                    &model.config.document_properties,
                    &context,
                )
            } else {
                preview::source_preview_at(&t.source, &model.config.document_properties, &context)
            }
        }),
    }]
}

fn pattern_context(
    model: &AppModel,
    category_id: Option<CategoryId>,
) -> carver_domain::TemplateContext {
    let category = match &model.sidebar.state {
        super::LoadState::Ready(categories) => categories
            .iter()
            .find(|c| Some(c.category.id) == category_id)
            .map(|c| c.category.name.clone())
            .unwrap_or_default(),
        _ => String::new(),
    };
    carver_domain::TemplateContext {
        now: model.template_pattern_time,
        category,
    }
}
