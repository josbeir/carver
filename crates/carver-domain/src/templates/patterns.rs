//! Bounded, non-recursive substitutions for reusable template source.
use crate::{FrontmatterValue, TemplateError, parse_frontmatter_document, replace_frontmatter};
use time::{OffsetDateTime, format_description};

/// One timestamp and destination shared by every pattern in a template copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateContext {
    /// Local timestamp captured by the host at use time.
    pub now: OffsetDateTime,
    /// Destination category name; an empty name means no category is available.
    pub category: String,
}

/// Expands body text and string property values without changing property keys or types.
///
/// Quote patterns in frontmatter. `\{{date}}` inserts a literal `{{date}}`.
/// Substitutions are single-pass, including text inside code blocks.
///
/// # Errors
/// Returns an error for unknown/unclosed patterns, invalid formats, or invalid frontmatter.
pub fn expand_template_source(
    source: &str,
    context: &TemplateContext,
) -> Result<String, TemplateError> {
    super::validate_template_structure(source)?;
    let mut document = parse_frontmatter_document(source);
    if let Some(document) = &mut document {
        if let Some(error) = &document.error {
            return Err(TemplateError::Frontmatter(error.clone()));
        }
        for field in &mut document.fields {
            expand_value(&mut field.value, context)?;
        }
    }
    // Work on the original header so comments and the exact body separator survive.
    let boundary = crate::frontmatter::template_body_start(source);
    let expanded = expand_text(&source[boundary..], context)?;
    let rewritten = if document.as_ref().is_some_and(|d| !d.fields.is_empty()) {
        replace_frontmatter(source, document.as_ref())?
    } else {
        source.to_owned()
    };
    let header_end = crate::frontmatter::template_body_start(&rewritten);
    let result = format!("{}{}", &rewritten[..header_end], expanded);
    super::validate_template_structure(&result)?;
    Ok(result)
}
fn expand_value(
    value: &mut FrontmatterValue,
    context: &TemplateContext,
) -> Result<(), TemplateError> {
    match value {
        FrontmatterValue::Text(text) => *text = expand_text(text, context)?,
        FrontmatterValue::List(values) => {
            for value in values {
                expand_value(value, context)?;
            }
        }
        FrontmatterValue::Object(entries) => {
            for (_, value) in entries {
                expand_value(value, context)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn expand_text(text: &str, context: &TemplateContext) -> Result<String, TemplateError> {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let prefix = &rest[..start];
        let escaped = prefix.ends_with('\\');
        output.push_str(if escaped {
            &prefix[..prefix.len() - 1]
        } else {
            prefix
        });
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| TemplateError::Pattern(after.to_owned()))?;
        let token = &after[..end];
        if escaped {
            output.push_str("{{");
            output.push_str(token);
            output.push_str("}}");
        } else {
            output.push_str(&resolve(token, context)?);
        }
        rest = &after[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}
fn resolve(token: &str, context: &TemplateContext) -> Result<String, TemplateError> {
    let error = || TemplateError::Pattern(token.to_owned());
    let now = context.now.replace_nanosecond(0).map_err(|_| error())?;
    match token {
        "date" => Ok(now.date().to_string()),
        "time" => now
            .format(&format_description::parse_strftime_borrowed("%H:%M").map_err(|_| error())?)
            .map_err(|_| error()),
        "datetime" => now
            .format(&format_description::well_known::Rfc3339)
            .map_err(|_| error()),
        "category" => Ok(context.category.clone()),
        _ => {
            let (kind, format) = token.split_once(':').ok_or_else(error)?;
            if !matches!(kind, "date" | "time" | "datetime") || format.is_empty() {
                return Err(error());
            }
            let description =
                format_description::parse_strftime_borrowed(format).map_err(|_| error())?;
            now.format(&description).map_err(|_| error())
        }
    }
}

#[cfg(test)]
mod tests;
