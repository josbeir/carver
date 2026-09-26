//! Sandboxed full-Carve HTML preview used by rendered and split modes.

use std::{
    cell::RefCell,
    fs,
    path::{Component, Path, PathBuf},
    rc::Rc,
};

use carver_domain::rendering::HtmlProfile;
use gettextrs::gettext;
use webkit6::prelude::*;

mod heading_provenance;

/// Tracks the note whose private asset directory a `carver-asset` scheme handler should read.
///
/// The note id is an internal storage detail: it never appears in canonical Carve source, and
/// the scheme resolves the note-relative `assets/<filename>` markup against this scope.
pub(super) type AssetScope = Rc<RefCell<Option<carver_sdk::NoteId>>>;

/// Creates an empty managed-asset scope.
pub(super) fn asset_scope() -> AssetScope {
    Rc::new(RefCell::new(None))
}

const PREVIEW_STYLESHEET: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/web/dist/preview.css"));
const PREVIEW_HIGHLIGHTING: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/web/dist/preview-highlighting.js"
));

/// Builds a non-editable `WebKitGTK` view for trusted Carve renderer output.
pub(super) fn build_preview(
    assets_dir: Option<&Path>,
    scope: &AssetScope,
    toast_overlay: &libadwaita::ToastOverlay,
) -> webkit6::WebView {
    let context = webkit6::WebContext::new();
    install_editor_asset_scheme(
        &context,
        assets_dir.map(Path::to_path_buf),
        Rc::clone(scope),
    );
    let manager = webkit6::UserContentManager::new();
    manager.add_script(&webkit6::UserScript::new(
        PREVIEW_HIGHLIGHTING,
        webkit6::UserContentInjectedFrames::TopFrame,
        webkit6::UserScriptInjectionTime::End,
        &[],
        &[],
    ));
    let settings = webkit6::Settings::new();
    // The preview document's CSP keeps document markup scriptless. JavaScript
    // stays enabled solely for the native split-preview scroll bridge, which
    // invokes a fixed host script through `WebView::evaluate_javascript`.
    settings.set_enable_javascript(true);
    settings.set_enable_javascript_markup(false);
    settings.set_enable_developer_extras(cfg!(debug_assertions));
    settings.set_enable_media(false);
    settings.set_enable_html5_database(false);
    settings.set_enable_html5_local_storage(false);
    settings.set_enable_smooth_scrolling(false);
    // Preview content is replaced as the user edits. Retaining replaced pages
    // in WebKit's back/forward cache makes long editing sessions grow without
    // bound even though only the current snapshot is relevant.
    settings.set_enable_page_cache(false);
    settings.set_auto_load_images(true);
    settings.set_print_backgrounds(false);
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

fn preview_document_style(
    theme: &super::web::EditorTheme,
    appearance: &super::web::DocumentAppearance,
) -> String {
    format!(
        "{PREVIEW_STYLESHEET}\n:root {{ --accent-color: {}; --selection-background: {}; --selection-foreground: {}; --preview-accent-color: {}; --preview-selection-background: {}; --preview-selection-foreground: {}; {} }}",
        theme.selection.accent,
        theme.selection.background,
        theme.selection.foreground,
        theme.selection.accent,
        theme.selection.background,
        theme.selection.foreground,
        super::web::appearance_style(appearance),
    )
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
                    toast_overlay.add_toast(libadwaita::Toast::new(&gettext(
                        "Could not open the link in your default browser.",
                    )));
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
///
/// The requested `assets/<filename>` is resolved against the note recorded in `scope`, so the
/// document markup never has to name the note.
pub(super) fn install_editor_asset_scheme(
    context: &webkit6::WebContext,
    assets_dir: Option<PathBuf>,
    scope: AssetScope,
) {
    context.register_uri_scheme("carver-asset", move |request| {
        let note = *scope.borrow();
        let bytes = request
            .path()
            .as_deref()
            .and_then(asset_filename)
            .and_then(|filename| {
                let note = note?;
                let directory = assets_dir.as_ref()?;
                Some(directory.join(note.to_string()).join(filename))
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
    let mut components = Path::new(relative).components();
    if matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none() {
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
#[cfg(test)]
pub(crate) fn rendered_document(source: &str, allow_remote_images: bool) -> String {
    let (dark, accent) = if gtk::is_initialized() {
        let style_manager = libadwaita::StyleManager::default();
        let dark = style_manager.is_dark();
        (dark, style_manager.accent_color().to_standalone_rgba(dark))
    } else {
        (false, gtk::gdk::RGBA::new(0.208, 0.557, 0.271, 1.0))
    };
    let theme = super::web::editor_theme(dark, &accent);
    rendered_document_with_theme(source, allow_remote_images, &theme, &default_appearance())
}

#[cfg(test)]
fn rendered_document_for_theme(source: &str, allow_remote_images: bool, dark: bool) -> String {
    let accent = gtk::gdk::RGBA::new(0.208, 0.557, 0.271, 1.0);
    let theme = super::web::editor_theme(dark, &accent);
    rendered_document_with_theme(source, allow_remote_images, &theme, &default_appearance())
}

fn default_appearance() -> super::web::DocumentAppearance {
    super::web::document_appearance(&crate::mvu::DocumentPreferences {
        font: None,
        line_height_percent: 155,
        width: carver_config::DocumentWidth::Comfortable,
    })
}

#[cfg(test)]
fn rendered_document_with_theme(
    source: &str,
    allow_remote_images: bool,
    theme: &super::web::EditorTheme,
    appearance: &super::web::DocumentAppearance,
) -> String {
    rendered_document_with_profile(
        source,
        allow_remote_images,
        theme,
        appearance,
        HtmlProfile::Enhanced,
        carve::Mode::Interactive,
    )
}

pub(super) fn rendered_document_with_profile(
    source: &str,
    allow_remote_images: bool,
    theme: &super::web::EditorTheme,
    appearance: &super::web::DocumentAppearance,
    profile: HtmlProfile,
    mode: carve::Mode,
) -> String {
    let image_sources = if allow_remote_images {
        "img-src data: https: http: carver-asset:"
    } else {
        "img-src data: carver-asset:"
    };
    let provenance = heading_provenance::HeadingProvenance(uuid::Uuid::now_v7().to_string());
    let body = profile
        .render_html(source, mode, &[&provenance])
        .map_or_else(
            |error| {
                glib::g_warning!("carver", "Could not render note: {error}");
                String::from("<p>Could not render the note preview.</p>")
            },
            |result| result.value,
        );
    let body = rewrite_preview_images(&body).unwrap_or_else(|error| {
        glib::g_warning!("carver", "Could not rewrite preview images: {error}");
        String::from("<p>Could not render the note preview.</p>")
    });
    let body = rewrite_preview_diff_blocks(&body);
    let stylesheet = preview_document_style(theme, appearance);
    format!(
        "<!doctype html><html data-theme=\"{color_scheme}\"><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; {image_sources}; font-src 'none'; script-src 'none'; connect-src 'none'; frame-src 'none'\"><style>{stylesheet}</style></head><body data-preview data-carver-heading-token=\"{heading_token}\">{body}</body></html>",
        heading_token = provenance.0,
        color_scheme = if theme.dark { "dark" } else { "light" },
    )
}

fn rewrite_preview_images(html: &str) -> Result<String, lol_html::errors::RewritingError> {
    super::html::rewrite_managed_images(html, |image, source| {
        if managed_asset_filename(source).is_some() {
            let uri = format!("carver-asset:///{source}");
            image.set_attribute("src", &html_escape::encode_double_quoted_attribute(&uri))?;
        }
        Ok(())
    })
}

/// Wraps unified-diff lines in colored spans for the rendered preview.
///
/// The preview view also injects `preview-highlighting.js`, which re-renders
/// diff fences with syntax token colors and supersedes this pass. This native
/// rewrite is the no-script fallback (and the shape the injected script expects
/// on load), so it mirrors that output: every line is a `carver-diff-line`
/// block span without its trailing newline, and `+++`/`---` headers stay plain.
fn rewrite_preview_diff_blocks(html: &str) -> String {
    const PRE_START: &str = "<pre";
    const CODE_START: &str = "<code";
    const CODE_END: &str = "</code></pre>";

    let mut rewritten = String::with_capacity(html.len());
    let mut remaining = html;
    while let Some(start) = remaining.find(PRE_START) {
        let (before, candidate) = remaining.split_at(start);
        rewritten.push_str(before);
        let Some(pre_end) = candidate.find('>') else {
            rewritten.push_str(candidate);
            break;
        };
        let (pre_tag, code_and_end) = candidate.split_at(pre_end + 1);
        let Some(code_and_end) = code_and_end.strip_prefix(CODE_START) else {
            rewritten.push_str(pre_tag);
            remaining = code_and_end;
            continue;
        };
        let Some(code_end) = code_and_end.find('>') else {
            rewritten.push_str(candidate);
            break;
        };
        let (code_attributes, content_and_end) = code_and_end.split_at(code_end + 1);
        let Some(end) = content_and_end.find(CODE_END) else {
            rewritten.push_str(candidate);
            break;
        };
        let (content, closing_tag) = content_and_end.split_at(end);
        rewritten.push_str(pre_tag);
        rewritten.push_str(CODE_START);
        rewritten.push_str(code_attributes);
        if has_html_class(pre_tag, "diff") || has_html_class(code_attributes, "language-diff") {
            rewritten.push_str(&render_diff_lines(content));
        } else {
            rewritten.push_str(content);
        }
        rewritten.push_str(CODE_END);
        remaining = &closing_tag[CODE_END.len()..];
    }
    rewritten.push_str(remaining);
    rewritten
}

fn has_html_class(opening_tag: &str, expected: &str) -> bool {
    // The Carve renderer emits canonical double-quoted, space-separated
    // attributes, so requiring the leading space keeps `data-class="..."` from
    // being mistaken for the element's class.
    opening_tag
        .split_once(" class=\"")
        .and_then(|(_, value)| value.split_once('"'))
        .is_some_and(|(classes, _)| {
            classes
                .split_ascii_whitespace()
                .any(|class| class == expected)
        })
}

fn render_diff_lines(content: &str) -> String {
    content
        .split('\n')
        .map(|line| match diff_line_class(line) {
            Some(class) => format!("<span class=\"carver-diff-line {class}\">{line}</span>"),
            None => format!("<span class=\"carver-diff-line\">{line}</span>"),
        })
        .collect()
}

fn diff_line_class(line: &str) -> Option<&'static str> {
    if line.starts_with("+++") || line.starts_with("---") {
        None
    } else if line.starts_with('+') {
        Some("carver-diff-add")
    } else if line.starts_with('-') {
        Some("carver-diff-remove")
    } else if line.starts_with("@@") {
        Some("carver-diff-hunk")
    } else {
        None
    }
}

/// Loads a static print snapshot with disclosure contents expanded.
pub(super) fn load_preview(
    view: &webkit6::WebView,
    source: &str,
    allow_remote_images: bool,
    profile: HtmlProfile,
) {
    view.load_html(
        &rendered_document_with_profile(
            source,
            allow_remote_images,
            &super::editor_theme(),
            &default_appearance(),
            profile,
            carve::Mode::Static,
        ),
        Some("carver-preview://document/"),
    );
}

/// Loads source into a preview using Adwaita's selected color scheme.
pub(super) fn load_preview_with_theme(
    view: &webkit6::WebView,
    source: &str,
    allow_remote_images: bool,
    theme: &super::web::EditorTheme,
    appearance: &super::web::DocumentAppearance,
    profile: HtmlProfile,
) {
    view.load_html(
        &rendered_document_with_profile(
            source,
            allow_remote_images,
            theme,
            appearance,
            profile,
            carve::Mode::Interactive,
        ),
        Some("carver-preview://document/"),
    );
}

#[cfg(test)]
mod tests;
