//! Names from the filesystem are raw bytes; this makes them safe to print (design §4 Security).

/// Escapes control characters (C0, DEL, C1, so ESC too) as `\xNN`.
pub fn sanitize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() {
            out.push_str(&format!("\\x{:02x}", u32::from(c)));
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
