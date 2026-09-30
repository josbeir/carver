//! SDK work for reusable templates, never capturing GTK objects.
use super::{AppMsg, AppRuntime, Effect, LibraryBackend, LibraryClient, LibraryReply, UiError};
use crate::mvu::TemplatesMsg;
use gettextrs::gettext;

impl<B: LibraryBackend> AppRuntime<B> {
    pub(super) fn run_template_effect(&self, effect: Effect) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        let config = self.inner.model.borrow().config.document_properties.clone();
        glib::spawn_future_local(async move {
            let message = match effect {
                Effect::LoadTemplates {
                    request_id,
                    purpose,
                } => TemplatesMsg::Loaded {
                    request_id,
                    purpose,
                    result: client.templates_async().await.map_err(template_error),
                },
                Effect::LoadTemplateNote {
                    request_id,
                    note_id,
                } => TemplatesMsg::NoteLoaded {
                    request_id,
                    result: client
                        .note_async(note_id)
                        .await
                        .map_err(template_error)
                        .and_then(|n| {
                            n.ok_or_else(|| {
                                UiError::new(gettext("The note is no longer available."))
                            })
                        }),
                },
                Effect::SaveTemplate {
                    request_id,
                    original,
                    name,
                    source,
                } => {
                    let result = match original {
                        Some(t) => {
                            client
                                .save_template_async(t.id, t.revision, name, source)
                                .await
                        }
                        None => client.create_template_async(name, source).await,
                    }
                    .map(|_| ())
                    .map_err(template_error);
                    TemplatesMsg::Saved { request_id, result }
                }
                Effect::DeleteTemplate { id, revision } => TemplatesMsg::Changed(
                    client
                        .delete_template_async(id, revision)
                        .await
                        .map_err(template_error),
                ),
                Effect::SaveTemplateCategory {
                    category_id,
                    name,
                    appearance,
                    template_id,
                } => TemplatesMsg::CategorySaved(
                    client
                        .update_category_with_template_async(
                            category_id,
                            name,
                            appearance,
                            template_id,
                        )
                        .await
                        .map_err(template_error),
                ),
                Effect::CreateCategoryNote { category_id } => {
                    let result = match client.categories_async().await {
                        Ok(categories) => match categories.iter().find(|c| c.id == category_id) {
                            Some(category) => {
                                create(&client, category_id, category.default_template_id, &config)
                                    .await
                            }
                            None => Err(UiError::new(gettext(
                                "The category is no longer available.",
                            ))),
                        },
                        Err(error) => Err(template_error(error)),
                    };
                    runtime.dispatch(AppMsg::Library(LibraryReply::NoteCreated { result }));
                    return;
                }
                Effect::CreateTemplateNote {
                    category_id,
                    template_id,
                } => {
                    let result = create(&client, category_id, Some(template_id), &config).await;
                    runtime.dispatch(AppMsg::Library(LibraryReply::NoteCreated { result }));
                    return;
                }
                _ => return,
            };
            runtime.dispatch(AppMsg::Templates(message));
        });
    }
}
async fn create<B: LibraryBackend>(
    client: &LibraryClient<B>,
    category_id: carver_sdk::CategoryId,
    template_id: Option<carver_sdk::TemplateId>,
    config: &carver_config::DocumentPropertiesConfig,
) -> Result<carver_sdk::Note, UiError> {
    let source = if let Some(id) = template_id {
        let template = client
            .template_async(id)
            .await
            .map_err(template_error)?
            .ok_or_else(|| UiError::new(gettext("The template is no longer available.")))?;
        crate::mvu::templates::validate(&template.source, &config.entries)?;
        // Resolve values and empty-field filtering exactly as ordinary blank-note creation does.
        let defaults = carver_domain::parse_frontmatter_document(&config.default_source())
            .map_or_else(Vec::new, |document| document.fields);
        carver_domain::merge_template_source(&template.source, &defaults, config.format)
            .map_err(|_| UiError::new(gettext("The template properties could not be merged.")))?
    } else {
        config.default_source()
    };
    client
        .create_note_with_source_async(category_id, source)
        .await
        .map_err(template_error)
}

fn template_error(error: impl std::fmt::Display) -> UiError {
    UiError::new(tr_fmt!(
        gettext("Could not complete the template action: {error}"),
        error = error.to_string()
    ))
}
