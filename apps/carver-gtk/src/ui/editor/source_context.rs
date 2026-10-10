//! Cached Carve AST context for the source editor's transient controls.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use carver_domain::source_analysis::{SourceAnalysis, SourceContext};
use gtk::prelude::*;

use super::source_commands::selection_from_buffer;

/// Owns the current parse snapshot used by source toolbar and breadcrumb projections.
#[derive(Clone)]
pub(crate) struct SourceContextCache {
    buffer: gtk::TextBuffer,
    analysis: Rc<RefCell<Arc<SourceAnalysis>>>,
}

impl SourceContextCache {
    /// Creates a projection awaiting the first immutable MVU analysis snapshot.
    pub(crate) fn new(buffer: &gtk::TextBuffer) -> Self {
        Self {
            buffer: buffer.clone(),
            analysis: Rc::new(RefCell::new(Arc::new(SourceAnalysis::default()))),
        }
    }

    /// Projects the reducer's shared analysis instead of reparsing in GTK callbacks.
    pub(crate) fn set_analysis(&self, analysis: &Arc<SourceAnalysis>) {
        self.analysis.replace(Arc::clone(analysis));
    }

    /// Classifies input without holding a projection borrow during MVU dispatch.
    pub(crate) fn accepts(&self, input: crate::mvu::SourceInput) -> bool {
        let source = self
            .buffer
            .text(&self.buffer.start_iter(), &self.buffer.end_iter(), false);
        input.accepts(
            &source,
            &self.analysis.borrow(),
            selection_from_buffer(&self.buffer),
        )
    }

    /// Returns the context enclosing the current cursor or complete selection.
    pub(crate) fn context(&self) -> Option<SourceContext> {
        self.analysis
            .borrow()
            .context_for(selection_from_buffer(&self.buffer))
    }
}
