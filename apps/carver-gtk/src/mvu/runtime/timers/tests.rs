use super::*;
use crate::ui::tests::support::run_main_context_until;
use std::cell::Cell;

pub(crate) fn timers_should_replace_pending_work_and_preserve_other_sessions() {
    let timers = Rc::new(Timers::default());
    let results = Rc::new(RefCell::new(Vec::new()));
    for value in 0..100 {
        let results = Rc::clone(&results);
        timers.schedule(1_u8, Duration::ZERO, move || {
            results.borrow_mut().push(value);
        });
    }
    let other_results = Rc::clone(&results);
    timers.schedule(2, Duration::ZERO, move || {
        other_results.borrow_mut().push(200);
    });
    assert_eq!(timers.len(), 2);
    assert!(run_main_context_until(|| results.borrow().len() == 2));
    assert_eq!(*results.borrow(), [99, 200]);
    assert_eq!(timers.len(), 0);
}

pub(crate) fn timer_callback_should_allow_rescheduling_its_own_key() {
    let timers = Rc::new(Timers::default());
    let completed = Rc::new(Cell::new(false));
    let next_timers = Rc::clone(&timers);
    let next_completed = Rc::clone(&completed);
    timers.schedule(1_u8, Duration::ZERO, move || {
        next_timers.schedule(1, Duration::ZERO, move || next_completed.set(true));
    });
    assert!(run_main_context_until(|| completed.get()));
    assert_eq!(timers.len(), 0);
}

pub(crate) fn dropping_timers_should_cancel_callbacks() {
    let timers = Rc::new(Timers::default());
    let cancelled = Rc::new(Cell::new(false));
    let cancelled_for_callback = Rc::clone(&cancelled);
    timers.schedule(1_u8, Duration::ZERO, move || {
        cancelled_for_callback.set(true);
    });
    drop(timers);
    let completed = Rc::new(Cell::new(false));
    let completed_for_callback = Rc::clone(&completed);
    glib::idle_add_local_once(move || completed_for_callback.set(true));
    assert!(run_main_context_until(|| completed.get()));
    assert!(!cancelled.get());
}
