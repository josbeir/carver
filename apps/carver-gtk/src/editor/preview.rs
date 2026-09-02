//! Sandboxed full-Carve HTML preview used by rendered and split modes.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use webkit6::prelude::*;

/// Builds a non-editable `WebKitGTK` view for trusted Carve renderer output.
pub(super) fn build_preview(assets_dir: Option<&Path>) -> webkit6::WebView {
    let context = webkit6::WebContext::new();
    install_asset_scheme(&context, assets_dir.map(Path::to_path_buf));
    let settings = webkit6::Settings::new();
    settings.set_enable_javascript(false);
    settings.set_enable_javascript_markup(false);
    settings.set_enable_media(false);
    settings.set_enable_html5_database(false);
    settings.set_enable_html5_local_storage(false);
    settings.set_auto_load_images(true);
    let view = webkit6::WebView::builder()
        .web_context(&context)
        .settings(&settings)
        .build();
    view.set_editable(false);
    view.set_widget_name("rendered-preview");
    view
}

fn install_asset_scheme(context: &webkit6::WebContext, assets_dir: Option<PathBuf>) {
    context.register_uri_scheme("carver-asset", move |request| {
        let bytes = request
            .path()
            .as_deref()
            .and_then(asset_filename)
            .and_then(|filename| {
                assets_dir
                    .as_ref()
                    .map(|directory| directory.join(filename))
            })
            .and_then(|path| fs::read(&path).ok())
            .unwrap_or_default();
        let content_type = request
            .path()
            .as_deref()
            .and_then(asset_filename)
            .map_or("application/octet-stream", mime_type);
        let length = i64::try_from(bytes.len()).unwrap_or(0);
        let bytes = glib::Bytes::from_owned(bytes);
        let stream = gtk::gio::MemoryInputStream::from_bytes(&bytes);
        request.finish(&stream, length, Some(content_type));
    });
}

fn asset_filename(path: &str) -> Option<&str> {
    let relative = path.strip_prefix("/assets/")?;
    let candidate = Path::new(relative);
    if candidate
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        Some(relative)
    } else {
        None
    }
}

fn mime_type(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// Renders source using Carve's full HTML renderer under a restrictive CSP.
pub(crate) fn rendered_document(source: &str, allow_remote_images: bool) -> String {
    rendered_document_for_theme(
        source,
        allow_remote_images,
        gtk::is_initialized() && libadwaita::StyleManager::default().is_dark(),
    )
}

fn rendered_document_for_theme(source: &str, allow_remote_images: bool, dark: bool) -> String {
    let image_sources = if allow_remote_images {
        "img-src data: https: http: carver-asset:"
    } else {
        "img-src data: carver-asset:"
    };
    let body = carver_richtext::render_html(source)
        .replace("src=\"assets/", "src=\"carver-asset:///assets/");
    let (background, foreground, code_background, border) = if dark {
        ("#1e1e1e", "#f6f5f4", "#303036", "#5b5b63")
    } else {
        ("#fafafa", "#242424", "#ededf1", "#b7b7c0")
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; {image_sources}; font-src 'none'; script-src 'none'; connect-src 'none'; frame-src 'none'\"><style>:root{{color-scheme:{color_scheme}}}html,body{{min-height:100%;margin:0;background:{background};color:{foreground}}}body{{font:12pt Cantarell,sans-serif;line-height:1.55;padding:24px;box-sizing:border-box}}img{{max-width:100%;height:auto}}pre{{overflow:auto;padding:12px;border-radius:8px;background:{code_background}}}table{{border-collapse:collapse;max-width:100%}}th,td{{padding:6px;border:1px solid {border}}}a{{color:inherit}}</style></head><body>{body}</body></html>",
        color_scheme = if dark { "dark" } else { "light" },
    )
}

/// Loads source into a preview while keeping the caller's UI state intact.
pub(super) fn load_preview(view: &webkit6::WebView, source: &str, allow_remote_images: bool) {
    view.load_html(
        &rendered_document(source, allow_remote_images),
        Some("carver-preview://document/"),
    );
}

#[cfg(test)]
mod tests;
