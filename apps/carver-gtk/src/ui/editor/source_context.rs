//! Cached Carve AST context for the source editor's transient controls.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use carver_domain::source_analysis::{SourceAnalysis, SourceContext};
use gtk::prelude::*;

use super::source_commands::selection_from_buffer;

/// Owns the current parse snapshot used by source toolbar and breadcrumb projections.
#[derive(Clone)]
pub(crate) struct SourceContextCache {
    buffer: glib::WeakRef<gtk::TextBuffer>,
    analysis: Rc<RefCell<Arc<SourceAnalysis>>>,
}

impl SourceContextCache {
    /// Creates a projection awaiting the first immutable MVU analysis snapshot.
    pub(crate) fn new(buffer: &gtk::TextBuffer) -> Self {
        Self {
            buffer: buffer.downgrade(),
            analysis: Rc::new(RefCell::new(Arc::new(SourceAnalysis::default()))),
        }
    }

    /// Projects the reducer's shared analysis instead of reparsing in GTK callbacks.
    pub(crate) fn set_analysis(&self, analysis: &Arc<SourceAnalysis>) {
        self.analysis.replace(Arc::clone(analysis));
    }

    /// Classifies input without holding a projection borrow during MVU dispatch.
    pub(crate) fn accepts(&self, input: crate::mvu::SourceInput) -> bool {
        let Some(buffer) = self.buffer.upgrade() else {
            return false;
        };
        let source = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        input.accepts(
            &source,
            &self.analysis.borrow(),
            selection_from_buffer(&buffer),
        )
    }

    /// Returns the context enclosing the current cursor or complete selection.
    pub(crate) fn context(&self) -> Option<SourceContext> {
        let buffer = self.buffer.upgrade()?;
        self.analysis
            .borrow()
            .context_for(selection_from_buffer(&buffer))
    }

    /// Projects guidance using the structural planner's unfinished-marker rules.
    pub(crate) fn placeholder(&self) -> Option<crate::mvu::SourcePlaceholder> {
        let buffer = self.buffer.upgrade()?;
        let source = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        crate::mvu::SourcePlaceholder::at(
            &source,
            &self.analysis.borrow(),
            selection_from_buffer(&buffer),
        )
    }
}
