//! Cached Carve AST context for the source editor's transient controls.

use std::{cell::RefCell, rc::Rc};

use carver_domain::source_analysis::{SourceAnalysis, SourceContext};
use gtk::prelude::*;

use super::source_commands::selection_from_buffer;

/// Owns the current parse snapshot used by source toolbar and breadcrumb projections.
#[derive(Clone)]
pub(crate) struct SourceContextCache {
    buffer: gtk::TextBuffer,
    analysis: Rc<RefCell<SourceAnalysis>>,
}

impl SourceContextCache {
    /// Creates an empty cache and parses the source buffer's current canonical text.
    pub(crate) fn new(buffer: &gtk::TextBuffer) -> Self {
        let cache = Self {
            buffer: buffer.clone(),
            analysis: Rc::new(RefCell::new(SourceAnalysis::default())),
        };
        cache.refresh();
        cache
    }

    /// Replaces the snapshot after the source buffer changes.
    pub(crate) fn refresh(&self) {
        let source = self
            .buffer
            .text(&self.buffer.start_iter(), &self.buffer.end_iter(), false);
        self.analysis.replace(SourceAnalysis::parse(&source));
    }

    /// Returns the context enclosing the current cursor or complete selection.
    pub(crate) fn context(&self) -> Option<SourceContext> {
        self.analysis
            .borrow()
            .context_for(selection_from_buffer(&self.buffer))
    }
}
