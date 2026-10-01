//! Pure template insertion preparation and guarded editor confirmation.
use super::{AppModel, Effect, InsertTarget, TemplatePreview, UiError};
use carver_config::DocumentPropertiesConfig;
use carver_domain::{
    FrontmatterDocument, FrontmatterField, parse_frontmatter_document, replace_frontmatter,
};
use gettextrs::gettext;

fn missing_fields(template: &str, source: &str) -> Vec<FrontmatterField> {
    let existing = parse_frontmatter_document(source).map_or_else(Vec::new, |d| d.fields);
    parse_frontmatter_document(template)
        .map_or_else(Vec::new, |d| d.fields)
        .into_iter()
        .filter(|field| !existing.iter().any(|own| own.key == field.key))
        .collect()
}

pub(super) fn preview(
    template: &str,
    target: &InsertTarget,
    config: &DocumentPropertiesConfig,
) -> Result<TemplatePreview, UiError> {
    let mut config = config.clone();
    config.enabled = false;
    let mut preview = super::preview::source_preview(template, &config)?;
    let missing = missing_fields(template, &target.source);
    preview
        .properties
        .retain(|p| missing.iter().any(|field| field.key == p.key));
    Ok(preview)
}

fn prepare(
    template: &str,
    source: &str,
    config: &DocumentPropertiesConfig,
) -> Result<(String, Option<FrontmatterDocument>), UiError> {
    super::validate(template, &config.entries)?;
    let error = || {
        UiError::new(gettext(
            "The template could not be inserted. Check the note’s properties.",
        ))
    };
    let body = replace_frontmatter(template, None).map_err(|_| error())?;
    let missing = missing_fields(template, source);
    if missing.is_empty() {
        return Ok((body, None));
    }
    let mut document = parse_frontmatter_document(source).unwrap_or(FrontmatterDocument {
        format: config.format,
        fields: Vec::new(),
        error: None,
    });
    if document.error.is_some() {
        return Err(error());
    }
    document.fields.extend(missing);
    Ok((body, Some(document)))
}

pub(super) fn confirm(model: &mut AppModel) -> Vec<Effect> {
    let Some(picker) = &model.template_picker else {
        return Vec::new();
    };
    let Some(target) = picker.insertion.clone() else {
        return Vec::new();
    };
    let Some(template) = picker
        .templates
        .iter()
        .find(|t| Some(t.id) == picker.selected)
    else {
        return Vec::new();
    };
    let Some(document) = model.editor.as_ref().filter(|d| {
        d.session == target.session
            && d.source == target.source
            && d.mode == target.mode
            && d.external_change.is_none()
            && d.mode != carver_config::EditorMode::Rendered
    }) else {
        return Vec::new();
    };
    let mode = document.mode;
    let (body, properties) = match prepare(
        &template.source,
        &target.source,
        &model.config.document_properties,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            model.set_notice(error);
            return Vec::new();
        }
    };
    if mode == carver_config::EditorMode::Rich {
        let prefix = properties
            .as_ref()
            .and_then(|d| replace_frontmatter("", Some(d)).ok());
        return vec![Effect::ApplyRichEditorCommand {
            command: carver_editor_protocol::EditorCommand::InsertSource {
                source: body,
                prefix,
            },
        }];
    }
    let note_body =
        replace_frontmatter(&target.source, None).unwrap_or_else(|_| target.source.clone());
    let prefix_len = target
        .source
        .chars()
        .count()
        .saturating_sub(note_body.chars().count());
    let start = target.selection.start.max(prefix_len);
    let end = target.selection.end.max(start);
    let edit = super::super::SourceEdit::apply(
        target.source,
        start..end,
        super::super::SourceCommand::InsertText(body),
    );
    let updated = match properties.as_ref() {
        Some(properties) => replace_frontmatter(edit.source(), Some(properties)),
        None => Ok(edit.source().to_owned()),
    };
    let Ok(updated) = updated else {
        return Vec::new();
    };
    let selection = edit.selection();
    let remaining_start = edit
        .source()
        .chars()
        .count()
        .saturating_sub(selection.start);
    let remaining_end = edit.source().chars().count().saturating_sub(selection.end);
    let updated_len = updated.chars().count();
    let selection =
        updated_len.saturating_sub(remaining_start)..updated_len.saturating_sub(remaining_end);
    if !model
        .editor
        .as_mut()
        .is_some_and(|d| d.source_changed(updated))
    {
        return Vec::new();
    }
    let mut effects: Vec<_> = [
        super::super::update::schedule_preview(model),
        super::super::update::schedule_editor_save(model),
    ]
    .into_iter()
    .flatten()
    .collect();
    effects.push(Effect::SelectEditorSource {
        session: target.session,
        selection,
    });
    effects
}

#[cfg(test)]
mod tests;
