//! Key → action mapping. Only the keys that exist so far; later tasks extend it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    Yes,
    No,
    Other,
}

pub fn action(key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => Action::Quit,
        KeyCode::Char('q') if !ctrl => Action::Quit,
        KeyCode::Char('y') if !ctrl => Action::Yes,
        KeyCode::Char('n') if !ctrl => Action::No,
        KeyCode::Esc | KeyCode::Enter => Action::No,
        _ => Action::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn maps_quit_keys() {
        assert_eq!(
            action(key(KeyCode::Char('q'), KeyModifiers::NONE)),
            Action::Quit
        );
        assert_eq!(
            action(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Action::Quit
        );
        assert_eq!(
            action(key(KeyCode::Char('c'), KeyModifiers::NONE)),
            Action::Other
        );
        assert_eq!(
            action(key(KeyCode::Char('y'), KeyModifiers::NONE)),
            Action::Yes
        );
        for code in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Enter] {
            assert_eq!(action(key(code, KeyModifiers::NONE)), Action::No);
        }
        assert_eq!(
            action(key(KeyCode::Char('x'), KeyModifiers::NONE)),
            Action::Other
        );
    }
}
