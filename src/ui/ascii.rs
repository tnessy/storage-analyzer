//! The ASCII fallback's last step: fold every non-ASCII symbol left in the drawn buffer (box
//! drawing, arrows, the ellipsis in copy text) to a plain character.

use ratatui::buffer::Buffer;

use super::theme::Glyphs;

/// User-facing copy for the active glyph set: in ASCII mode key names are spelled out
/// (`⏎` → `Enter`, `⌫` → `Bksp`) and `…`/`·` become `...`/`-` (UX §7).
pub fn text(glyphs: &Glyphs, s: &str) -> String {
    if !glyphs.ascii {
        return s.to_owned();
    }
    s.replace('⏎', "Enter")
        .replace('⌫', "Bksp")
        .replace('…', "...")
        .replace('·', "-")
}

fn fold_char(c: char) -> char {
    match c {
        '─' => '-',
        '│' => '|',
        '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' => '+',
        '═' | '║' | '╔' | '╗' | '╚' | '╝' => '#',
        '↑' => '^',
        '↓' => 'v',
        '←' => '<',
        '→' | '›' => '>',
        '⏎' => 'E',
        '⌫' => 'B',
        '…' => '.',
        '·' => '-',
        '✓' => '*',
        '■' | '█' | '▉' | '▊' | '▋' | '▌' | '▍' | '▎' | '▏' => '#',
        '░' => '.',
        '◌' | '◜' | '◝' | '◞' | '◟' => '~',
        '▲' => '^',
        '⊘' => '-',
        '▾' => '-',
        '▸' => '+',
        '≥' => '>',
        '×' => 'x',
        '↔' => '-',
        c if c.is_ascii() => c,
        _ => '?',
    }
}

/// Replaces every non-ASCII symbol with an ASCII one, cell by cell.
pub fn fold(buf: &mut Buffer) {
    let area = buf.area;
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            if !cell.symbol().is_ascii() {
                let folded: String = cell.symbol().chars().map(fold_char).collect();
                cell.set_symbol(&folded);
            }
        }
    }
}
