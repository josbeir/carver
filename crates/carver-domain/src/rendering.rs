//! Shared HTML presentation profiles; canonical source is never transformed in storage.

use carve::{Autolink, AutolinkOptions, CarveExtension, Details, Mode, Options, TocPlacement};

/// HTML behavior selected by the application for a render snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HtmlProfile {
    /// Core Carve behavior, without optional authoring helpers.
    #[default]
    Core,
    /// Author-placed contents, disclosures, and automatic HTTP(S) links.
    Enhanced,
}

impl HtmlProfile {
    /// Selects the profile from the global enhancement preference.
    #[must_use]
    pub const fn from_enabled(enabled: bool) -> Self {
        if enabled { Self::Enhanced } else { Self::Core }
    }

    /// Renders HTML with loss reporting and optional host annotations.
    ///
    /// Static mode opens disclosures for printing. Host extensions run before
    /// presentation transforms so authored headings retain their provenance.
    ///
    /// # Errors
    ///
    /// Returns Carve's checked-render error if rendering rejects a loss.
    pub fn render_html(
        self,
        source: &str,
        mode: Mode,
        annotations: &[&dyn CarveExtension],
    ) -> Result<carve::RenderResult<String>, carve::RenderLossError> {
        let toc = TocPlacement::new();
        let details = Details::new();
        // CONTEXT: Carver's desktop link handler supports HTTP(S); email links
        // need a separate navigation policy before automatic linking is enabled.
        let autolink = Autolink::with_options(AutolinkOptions {
            allowed_schemes: vec!["https".to_owned(), "http".to_owned()],
        });
        let mut options = Options::default().with_positions(true).with_mode(mode);
        options.extensions.extend_from_slice(annotations);
        if self == Self::Enhanced {
            options
                .extensions
                .extend([&toc as &dyn CarveExtension, &details, &autolink]);
        }
        carve::with_render_loss_report(
            carve::RenderTarget::Html,
            carve::CheckedRenderOptions::default(),
            || carve::to_html_with_options(source, &options),
        )
    }
}

#[cfg(test)]
mod tests;
