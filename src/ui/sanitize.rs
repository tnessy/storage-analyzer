//! Names from the filesystem are raw bytes; this makes them safe to print (design §4 Security).

/// Bidirectional and format controls that can make a name display reversed or disguised.
fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

/// Escapes control characters (C0, DEL, C1, so ESC too) as `\xNN` and the Unicode bidi controls
/// as `\u{XXXX}`.
pub fn sanitize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() {
            out.push_str(&format!("\\x{:02x}", u32::from(c)));
        } else if is_bidi_control(c) {
            out.push_str(&format!("\\u{{{:04X}}}", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

/// A file name for display: lossy UTF-8, then sanitized. Filesystem calls keep using the bytes.
pub fn display_name(bytes: &[u8]) -> String {
    sanitize(&String::from_utf8_lossy(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_become_hex_escapes() {
        assert_eq!(sanitize("a\x1b[31mb"), "a\\x1b[31mb");
        assert_eq!(sanitize("tab\there\n"), "tab\\x09here\\x0a");
        assert_eq!(sanitize("del\x7f"), "del\\x7f");
        assert_eq!(sanitize("c1\u{85}\u{9b}"), "c1\\x85\\x9b");
        assert_eq!(sanitize("plain ünïcode 日本"), "plain ünïcode 日本");
    }

    #[test]
    fn bidi_and_format_controls_become_unicode_escapes() {
        let listed = [
            0x061c, 0x200e, 0x200f, 0x202a, 0x202b, 0x202c, 0x202d, 0x202e, 0x2066, 0x2067, 0x2068,
            0x2069,
        ];
        for cp in listed {
            let raw = char::from_u32(cp).unwrap();
            let shown = sanitize(&format!("a{raw}b"));
            assert_eq!(shown, format!("a\\u{{{cp:04X}}}b"), "U+{cp:04X}");
            assert!(!shown.contains(raw));
        }
        // Neighbours that are ordinary text stay as they are.
        assert_eq!(
            sanitize("\u{200d}\u{202f}\u{2065}"),
            "\u{200d}\u{202f}\u{2065}"
        );
    }

    #[test]
    fn invalid_utf8_is_decoded_lossily() {
        assert_eq!(display_name(b"ok\xff\xfeend"), "ok\u{fffd}\u{fffd}end");
        assert_eq!(display_name(b"a\x1b]0;x\x07"), "a\\x1b]0;x\\x07");
    }

    #[test]
    fn output_never_contains_a_control_character() {
        for byte in 0u8..=255 {
            let shown = display_name(&[b'a', byte, b'b']);
            assert!(
                !shown.chars().any(char::is_control),
                "byte {byte:#x}: {shown:?}"
            );
        }
    }
}
