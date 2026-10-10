//! Cancellable main-thread debouncing using the existing `GLib` dependency.
//!
//! `GLib` already owns the application's event loop and supplies cancellable sources. A separate
//! debounce crate would add an executor/dependency without improving this GTK-local boundary.

use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};

pub(super) struct Timers<K: Ord> {
    sources: RefCell<BTreeMap<K, glib::SourceId>>,
}

impl<K: Ord> Default for Timers<K> {
    fn default() -> Self {
        Self {
            sources: RefCell::new(BTreeMap::new()),
        }
    }
}

impl<K: Copy + Ord + 'static> Timers<K> {
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.sources.borrow().len()
    }
    pub(super) fn schedule(
        self: &Rc<Self>,
        key: K,
        delay: Duration,
        callback: impl FnOnce() + 'static,
    ) {
        let previous = self.sources.borrow_mut().remove(&key);
        if let Some(source) = previous {
            source.remove();
        }
        let timers = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(delay, move || {
            let Some(timers) = timers.upgrade() else {
                return;
            };
            // Forget the live source before dispatch, which can schedule another timer at this key.
            timers.sources.borrow_mut().remove(&key);
            callback();
        });
        self.sources.borrow_mut().insert(key, source);
    }
}

#[cfg(test)]
pub(crate) mod tests;

impl<K: Ord> Drop for Timers<K> {
    fn drop(&mut self) {
        for (_, source) in std::mem::take(self.sources.get_mut()) {
            source.remove();
        }
    }
}
