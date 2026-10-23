//! Key → action mapping. Only the keys that exist so far; later tasks extend it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    Yes,
    No,
    Esc,
    Enter,
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Right,
    Left,
    ZoomOut,
    /// `Tab` and `Shift-Tab` both switch panes.
    SwitchPane,
    /// `d` / `Delete`, `p`, `x`, `r`: wired by T020/T021; the `..` row intercepts them first.
    Delete,
    Prioritise,
    Skip,
    Resume,
    /// `1`..`4`: Tree, Types, Largest, Skipped.
    View(u8),
    Other,
}

pub fn action(key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => Action::Quit,
        KeyCode::Char(_) if ctrl => Action::Other,
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('y') => Action::Yes,
        KeyCode::Char('n') => Action::No,
        KeyCode::Esc => Action::Esc,
        KeyCode::Enter => Action::Enter,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Home | KeyCode::Char('g') => Action::Top,
        KeyCode::End | KeyCode::Char('G') => Action::Bottom,
        KeyCode::Right | KeyCode::Char('l') => Action::Right,
        KeyCode::Left | KeyCode::Char('h') => Action::Left,
        KeyCode::Char(c @ '1'..='4') => Action::View(c as u8 - b'0'),
        KeyCode::Char('d') | KeyCode::Delete => Action::Delete,
        KeyCode::Char('p') => Action::Prioritise,
        KeyCode::Char('x') => Action::Skip,
        KeyCode::Char('r') => Action::Resume,
        KeyCode::Tab | KeyCode::BackTab => Action::SwitchPane,
        KeyCode::Backspace | KeyCode::Char('-') => Action::ZoomOut,
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
    fn maps_tree_keys() {
        let plain = |c| action(key(c, KeyModifiers::NONE));
        assert_eq!(plain(KeyCode::Down), Action::Down);
        assert_eq!(plain(KeyCode::Char('j')), Action::Down);
        assert_eq!(plain(KeyCode::Char('k')), Action::Up);
        assert_eq!(plain(KeyCode::Char('l')), Action::Right);
        assert_eq!(plain(KeyCode::Char('h')), Action::Left);
        assert_eq!(plain(KeyCode::Char('g')), Action::Top);
        assert_eq!(
            action(key(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Action::Bottom
        );
        assert_eq!(plain(KeyCode::Backspace), Action::ZoomOut);
        assert_eq!(plain(KeyCode::Char('-')), Action::ZoomOut);
        assert_eq!(
            action(key(KeyCode::Char('j'), KeyModifiers::CONTROL)),
            Action::Other
        );
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
        assert_eq!(
            action(key(KeyCode::Char('n'), KeyModifiers::NONE)),
            Action::No
        );
        assert_eq!(action(key(KeyCode::Esc, KeyModifiers::NONE)), Action::Esc);
        assert_eq!(
            action(key(KeyCode::Enter, KeyModifiers::NONE)),
            Action::Enter
        );
        let plain = |c| action(key(KeyCode::Char(c), KeyModifiers::NONE));
        assert_eq!(plain('x'), Action::Skip);
        assert_eq!(plain('d'), Action::Delete);
        assert_eq!(plain('p'), Action::Prioritise);
        assert_eq!(plain('r'), Action::Resume);
        assert_eq!(plain('z'), Action::Other);
    }
}
