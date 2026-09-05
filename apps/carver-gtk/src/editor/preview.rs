//! Sandboxed full-Carve HTML preview used by rendered and split modes.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use webkit6::prelude::*;

const PREVIEW_STYLESHEET: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/web/dist/preview.css"));

/// Builds a non-editable `WebKitGTK` view for trusted Carve renderer output.
pub(super) fn build_preview(
    assets_dir: Option<&Path>,
    toast_overlay: &libadwaita::ToastOverlay,
) -> webkit6::WebView {
    let context = webkit6::WebContext::new();
    install_editor_asset_scheme(&context, assets_dir.map(Path::to_path_buf));
    let manager = webkit6::UserContentManager::new();
    manager.add_style_sheet(&webkit6::UserStyleSheet::new(
        PREVIEW_STYLESHEET,
        webkit6::UserContentInjectedFrames::TopFrame,
        webkit6::UserStyleLevel::User,
        &[],
        &[],
    ));
    let settings = webkit6::Settings::new();
    // The preview document's CSP keeps document markup scriptless. JavaScript
    // stays enabled solely for the native split-preview scroll bridge, which
    // invokes a fixed host script through `WebView::evaluate_javascript`.
    settings.set_enable_javascript(true);
    settings.set_enable_javascript_markup(false);
    settings.set_enable_media(false);
    settings.set_enable_html5_database(false);
    settings.set_enable_html5_local_storage(false);
    settings.set_auto_load_images(true);
    let view = webkit6::WebView::builder()
        .web_context(&context)
        .user_content_manager(&manager)
        .settings(&settings)
        .build();
    view.set_editable(false);
    view.set_widget_name("rendered-preview");
    connect_external_link_handler(&view, toast_overlay);
    view
}

/// Sends user-activated web links to the desktop browser instead of navigating
/// the sandboxed preview view away from its current document.
fn connect_external_link_handler(
    view: &webkit6::WebView,
    toast_overlay: &libadwaita::ToastOverlay,
) {
    let toast_overlay = toast_overlay.clone();
    view.connect_decide_policy(move |_, decision, decision_type| {
        if !matches!(
            decision_type,
            webkit6::PolicyDecisionType::NavigationAction
                | webkit6::PolicyDecisionType::NewWindowAction
        ) {
            return false;
        }
        let Some(navigation) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() else {
            return false;
        };
        let Some(uri) = navigation
            .navigation_action()
            .and_then(|action| action.request())
            .and_then(|request| request.uri())
            .filter(|uri| is_external_link(uri))
        else {
            return false;
        };

        decision.ignore();
        let toast_overlay = toast_overlay.clone();
        gtk::gio::AppInfo::launch_default_for_uri_async(
            uri.as_str(),
            None::<&gtk::gio::AppLaunchContext>,
            None::<&gtk::gio::Cancellable>,
            move |result| {
                if result.is_err() {
                    toast_overlay.add_toast(libadwaita::Toast::new(
                        "Could not open the link in your default browser.",
                    ));
                }
            },
        );
        true
    });
}

fn is_external_link(uri: &str) -> bool {
    matches!(uri.split_once(':'), Some(("http" | "https", _)))
}

/// Installs the managed-asset scheme shared by read-only previews and the editor.
pub(super) fn install_editor_asset_scheme(
    context: &webkit6::WebContext,
    assets_dir: Option<PathBuf>,
) {
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
    valid_asset_filename(path.strip_prefix("/assets/")?)
}

/// Returns a validated filename for a source-relative managed asset path.
pub(super) fn managed_asset_filename(path: &str) -> Option<&str> {
    valid_asset_filename(path.strip_prefix("assets/")?)
}

fn valid_asset_filename(relative: &str) -> Option<&str> {
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

/// Returns the MIME type that Carver supports for a managed image filename.
pub(super) fn mime_type(path: &str) -> &'static str {
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
    let (dark, theme) = if gtk::is_initialized() {
        let style_manager = libadwaita::StyleManager::default();
        let dark = style_manager.is_dark();
        let theme = super::web::selection_theme(
            dark,
            &style_manager.accent_color().to_standalone_rgba(dark),
        );
        (dark, theme)
    } else {
        let dark = false;
        let theme =
            super::web::selection_theme(dark, &gtk::gdk::RGBA::new(0.208, 0.557, 0.271, 1.0));
        (dark, theme)
    };
    rendered_document_with_selection(source, allow_remote_images, dark, &theme)
}

#[cfg(test)]
fn rendered_document_for_theme(source: &str, allow_remote_images: bool, dark: bool) -> String {
    let theme = super::web::selection_theme(dark, &gtk::gdk::RGBA::new(0.208, 0.557, 0.271, 1.0));
    rendered_document_with_selection(source, allow_remote_images, dark, &theme)
}

fn rendered_document_with_selection(
    source: &str,
    allow_remote_images: bool,
    dark: bool,
    selection: &super::web::SelectionTheme,
) -> String {
    let image_sources = if allow_remote_images {
        "img-src data: https: http: carver-asset:"
    } else {
        "img-src data: carver-asset:"
    };
    let body = carve::to_html(source).replace("src=\"assets/", "src=\"carver-asset:///assets/");
    let selection_style = format!(
        "--preview-accent-color: {} !important; --preview-selection-background: {} !important; --preview-selection-foreground: {} !important;",
        selection.accent, selection.background, selection.foreground,
    );
    format!(
        "<!doctype html><html data-theme=\"{color_scheme}\" style=\"{selection_style}\"><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; {image_sources}; font-src 'none'; script-src 'none'; connect-src 'none'; frame-src 'none'\"></head><body data-preview>{body}</body></html>",
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
