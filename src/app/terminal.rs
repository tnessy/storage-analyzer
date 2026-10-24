//! Raw mode and alternate screen, restored on drop, on panic and on signals.

use std::io::{self, stdout};
use std::panic;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Runs its restore action once, when dropped (also while unwinding).
pub struct Guard {
    restore: Option<Box<dyn FnOnce()>>,
}

impl Guard {
    pub fn new(restore: impl FnOnce() + 'static) -> Self {
        Self {
            restore: Some(Box::new(restore)),
        }
    }

    /// Enters raw mode and the alternate screen.
    pub fn enter_terminal() -> io::Result<Self> {
        enable_raw_mode()?;
        let guard = Self::new(restore_terminal);
        execute!(stdout(), EnterAlternateScreen)?;
        Ok(guard)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(restore) = self.restore.take() {
            restore();
        }
    }
}

/// Safe to call more than once.
pub fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, Show);
}

/// A panic that happened off the main thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerPanic {
    /// `None` for an unnamed thread.
    pub thread: Option<String>,
    pub message: String,
}

/// Panics from threads other than the main one, waiting for the main loop to look at them.
#[derive(Clone, Debug, Default)]
pub struct PanicSlot(Arc<Mutex<Vec<WorkerPanic>>>);

impl PanicSlot {
    pub fn record(&self, thread: Option<String>, message: String) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(WorkerPanic { thread, message });
    }

    pub fn take(&self) -> Vec<WorkerPanic> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Removes and returns the panics `wanted` accepts; the others stay for their own reader.
    pub fn take_if(&self, wanted: impl Fn(&WorkerPanic) -> bool) -> Vec<WorkerPanic> {
        let mut all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let (taken, kept) = std::mem::take(&mut *all).into_iter().partition(wanted);
        *all = kept;
        taken
    }
}

/// Restores the terminal before a main-thread panic message is printed, so it stays readable.
pub fn install_panic_hook(slot: PanicSlot) {
    install_panic_hook_with(restore_terminal, slot);
}

/// Must be called on the main thread. A panic there restores the terminal first. A panic on any
/// other thread must not touch the display while the UI keeps running: its message goes to
/// `slot` instead, and the main loop reports it.
pub fn install_panic_hook_with(restore: fn(), slot: PanicSlot) {
    let main = thread::current().id();
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if thread::current().id() == main {
            restore();
            previous(info);
        } else {
            slot.record(
                thread::current().name().map(str::to_owned),
                info.to_string(),
            );
        }
    }));
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    use super::*;

    #[test]
    fn guard_restores_once_on_drop() {
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        drop(Guard::new(move || c.set(c.get() + 1)));
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn guard_restores_while_unwinding() {
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _guard = Guard::new(move || c.set(1));
            panic!("boom");
        }));
        assert!(result.is_err());
        assert_eq!(count.get(), 1);
    }
}
