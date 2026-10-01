//! Pure effective-property snapshots shared by the picker and template editor.
use super::{TemplatePreview, TemplateProperty, TemplatePropertyOrigin, UiError};
use carver_config::DocumentPropertiesConfig;
use carver_domain::{FrontmatterField, FrontmatterValue, parse_frontmatter_document};
use gettextrs::gettext;

#[cfg(test)]
pub(super) fn source_preview(
    source: &str,
    config: &DocumentPropertiesConfig,
) -> Result<TemplatePreview, UiError> {
    source_preview_at(
        source,
        config,
        &carver_domain::TemplateContext {
            now: time::OffsetDateTime::UNIX_EPOCH,
            category: String::new(),
        },
    )
}
pub(super) fn source_preview_at(
    source: &str,
    config: &DocumentPropertiesConfig,
    context: &carver_domain::TemplateContext,
) -> Result<TemplatePreview, UiError> {
    super::validate(source, &config.entries)?;
    let expanded = carver_domain::expand_template_source(source, context)
        .map_err(|error| UiError::new(error.to_string()))?;
    let source = expanded.as_str();
    let authored = parse_frontmatter_document(source).map_or_else(Vec::new, |d| d.fields);
    let defaults = parse_frontmatter_document(&config.default_source_at(context.now))
        .map_or_else(Vec::new, |d| d.fields);
    let merged = carver_domain::merge_template_source(source, &defaults, config.format)
        .map_err(|_| UiError::new(gettext("The template properties could not be merged.")))?;
    let fields = parse_frontmatter_document(&merged).map_or_else(Vec::new, |d| d.fields);
    let properties = fields
        .into_iter()
        .map(|field| {
            let own = authored.iter().any(|f| f.key == field.key);
            let default = defaults.iter().find(|f| f.key == field.key);
            let origin = match (own, default) {
                (true, Some(default)) if default.value != field.value => {
                    TemplatePropertyOrigin::Override(value_text(default))
                }
                (true, _) => TemplatePropertyOrigin::Template,
                (false, _) => TemplatePropertyOrigin::Default,
            };
            TemplateProperty {
                value: value_text(&field),
                key: field.key,
                origin,
            }
        })
        .collect();
    Ok(TemplatePreview {
        source: source.to_owned(),
        properties,
    })
}
fn value_text(field: &FrontmatterField) -> String {
    match &field.value {
        FrontmatterValue::Text(text) if !text.is_empty() => text.clone(),
        _ => field.value.to_json().to_string(),
    }
}

#[cfg(test)]
mod tests;
