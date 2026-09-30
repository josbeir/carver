//! Template intents, validation, and pure dialog transitions.
use super::{AppModel, Effect, RequestId, UiError};
use carver_sdk::{Category, CategoryId, NoteId, NoteTemplate, Revision, TemplateId};
use gettextrs::gettext;

/// Purpose of loading the current template list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemplatePurpose {
    /// Manage reusable templates.
    Manage,
    /// Load choices for the shared creation form.
    NewCategory,
    /// Choose a template for a captured destination.
    Pick(CategoryId),
    /// Edit a category and its template assignment.
    Category(Category),
}
/// Template operations submitted by GTK or the SDK runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemplatesMsg {
    /// Open the template manager.
    Manage,
    /// Load choices for the Add dialog.
    NewCategory,
    /// Open a picker in the current category.
    Pick,
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
        TemplatesMsg::Manage => load(model, TemplatePurpose::Manage),
        TemplatesMsg::NewCategory => load(model, TemplatePurpose::NewCategory),
        TemplatesMsg::Pick => model
            .active_category_id()
            .map_or_else(Vec::new, |id| load(model, TemplatePurpose::Pick(id))),
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
                Ok(templates) => vec![Effect::ShowTemplates { templates, purpose }],
                Err(error) => {
                    model.set_notice(error);
                    Vec::new()
                }
            }
        }
        TemplatesMsg::Edit(original) => {
            let name = original
                .as_ref()
                .map_or_else(String::new, |t| t.name.clone());
            let source = original
                .as_ref()
                .map_or_else(String::new, |t| t.source.clone());
            edit(model, original, name, source)
        }
        TemplatesMsg::Duplicate(template) => edit(model, None, template.name, template.source),
        TemplatesMsg::FromEditor => {
            let Some(note) = &model.editor else {
                return Vec::new();
            };
            let source = note.source.clone();
            let name = carver_domain::derive_content(&source).title;
            edit(model, None, name, source)
        }
        TemplatesMsg::FromNote(id) => {
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
            load(model, TemplatePurpose::Manage)
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
                    effects.extend(load(model, TemplatePurpose::Manage));
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
    carver_domain::validate_template_source(source).map_err(|error| {
        UiError::new(match error {
            carver_domain::TemplateError::ManagedAssets => {
                gettext("Templates cannot contain managed images or attachments.")
            }
            _ => gettext("The template frontmatter is invalid or contains duplicate properties."),
        })
    })?;
    if let Some(document) = carver_domain::parse_frontmatter_document(source) {
        for field in document.fields {
            if let Some(default) = defaults.iter().find(|d| d.key == field.key) {
                let shape = default.resolved();
                let valid = if shape.field_type == carver_domain::PropertyType::List {
                    match &field.value {
                        carver_domain::FrontmatterValue::Null => true,
                        carver_domain::FrontmatterValue::Text(_) => !shape.multiple,
                        carver_domain::FrontmatterValue::List(values) => {
                            shape.multiple
                                && values
                                    .iter()
                                    .all(|v| matches!(v, carver_domain::FrontmatterValue::Text(_)))
                        }
                        _ => false,
                    }
                } else {
                    shape.field_type.accepts_value(&field.value)
                };
                if !valid {
                    return Err(UiError::new(tr_fmt!(
                        gettext("Property {key} does not match its configured type."),
                        key = field.key
                    )));
                }
            }
        }
    }
    Ok(())
}

// Templates do not modify existing notes. Record our own write before the next focus wakeup
// checks the shared library, without refreshing note content or browser pages.
fn acknowledge_mutation(model: &mut AppModel) -> Option<Effect> {
    super::update::request_library_revision(
        model,
        super::model::LibraryRevisionCheckReason::LocalTemplateMutation,
    )
}
