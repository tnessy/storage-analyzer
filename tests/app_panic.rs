//! The panic hook is process-global, so this file holds a single test.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use storage_analyzer::app::terminal::{PanicSlot, install_panic_hook_with};

static RESTORED: AtomicBool = AtomicBool::new(false);

fn fake_restore() {
    RESTORED.store(true, Ordering::SeqCst);
}

#[test]
fn only_a_main_thread_panic_restores_the_terminal() {
    let slot = PanicSlot::default();
    install_panic_hook_with(fake_restore, slot.clone());

    // A caught panic on another thread (a worker, the delete executor) must leave the
    // alternate screen and raw mode alone, and be reported through the slot.
    let worker = thread::spawn(|| panic!("worker went wrong"));
    assert!(worker.join().is_err());
    assert!(
        !RESTORED.load(Ordering::SeqCst),
        "terminal was restored for a worker panic"
    );
    let messages = slot.take();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        messages[0].message.contains("worker went wrong"),
        "{messages:?}"
    );
    assert_eq!(messages[0].thread, None);

    // Named threads are tagged, so the app can tell scan workers from other helpers.
    let named = thread::Builder::new()
        .name("scan-7".into())
        .spawn(|| panic!("named"))
        .unwrap();
    assert!(named.join().is_err());
    let messages = slot.take();
    assert_eq!(
        messages[0].thread.as_deref(),
        Some("scan-7"),
        "{messages:?}"
    );
    assert!(slot.take().is_empty());

    // A panic on the thread that installed the hook restores the terminal.
    let caught = std::panic::catch_unwind(|| panic!("main went wrong"));
    assert!(caught.is_err());
    assert!(
        RESTORED.load(Ordering::SeqCst),
        "main-thread panic did not restore the terminal"
    );
    assert!(slot.take().is_empty());
}
