//! HTML-aware managed image traversal shared by preview and clipboard rendering.

use lol_html::{
    RewriteStrSettings, element,
    errors::{AttributeNameError, RewritingError},
    html_content::Element,
    rewrite_str,
};

pub(super) fn rewrite_managed_images(
    html: &str,
    mut rewrite: impl FnMut(&mut Element<'_, '_>, &str) -> Result<(), AttributeNameError>,
) -> Result<String, RewritingError> {
    rewrite_str(
        html,
        RewriteStrSettings::new().append_element_content_handler(element!(
            "img[src]",
            move |image| {
                if let Some(source) = image.get_attribute("src") {
                    let source = html_escape::decode_html_entities(&source);
                    if source.starts_with("assets/") {
                        rewrite(image, &source)?;
                    }
                }
                Ok(())
            }
        )),
    )
}

#[cfg(test)]
mod tests;
