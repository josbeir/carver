//! Side effects requested by the pure reducer.

use carver_sdk::CategoryId;

use super::{RequestId, TimerId};

/// Work that the runtime performs after rendering an updated model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Wait before dispatching the current search timer identity.
    ScheduleSearch {
        /// Identity used to ignore a superseded debounce timer.
        timer_id: TimerId,
    },
    /// Load sidebar categories and active-note counts.
    LoadSidebar {
        /// Identity for stale-completion protection.
        request_id: RequestId,
    },
    /// Load browser note summaries for the selected category and query.
    LoadBrowser {
        /// Identity for stale-completion protection.
        request_id: RequestId,
        /// Category to restrict the listing to, if any.
        category_id: Option<CategoryId>,
        /// Search input to apply.
        query: String,
    },
    /// Load recoverable deleted content.
    LoadTrash {
        /// Identity for stale-completion protection.
        request_id: RequestId,
    },
}
