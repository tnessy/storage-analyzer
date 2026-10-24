//! Pure text formatting: sizes, durations, truncation, sanitizing.

use std::time::Duration;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const UNITS: [&str; 7] = ["B", "K", "M", "G", "T", "P", "E"];

fn rounded_1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn scaled(bytes: u64) -> (f64, usize) {
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    // A value that would print as 1000.0 or more moves up one unit (UX §7.1).
    if unit > 0 && unit < UNITS.len() - 1 && rounded_1(value) >= 1000.0 {
        value /= 1024.0;
        unit += 1;
    }
    (value, unit)
}

/// `0B`, `1023B`, `812.0K`, `64.2G`: at most 6 cells (UX §7.1).
pub fn size_iec(bytes: u64) -> String {
    let (value, unit) = scaled(bytes);
    if unit == 0 {
        format!("{bytes}B")
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

/// `0 B`, `812.0 KiB`, `31.0 GiB`.
pub fn size_iec_long(bytes: u64) -> String {
    let (value, unit) = scaled(bytes);
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}iB", UNITS[unit])
    }
}

/// `812 B/s`, `812.0 KiB/s`, `64.2 MiB/s`: the long size with `/s`.
pub fn throughput(bytes_per_s: u64) -> String {
    format!("{}/s", size_iec_long(bytes_per_s))
}

/// Base 1000, three significant digits: `812`, `12.4k`, `1.24M`.
pub fn count_compact(n: u64) -> String {
    const SUFFIXES: [&str; 4] = ["", "k", "M", "G"];
    if n < 1000 {
        return n.to_string();
    }
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < SUFFIXES.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    for decimals in [2, 1, 0] {
        let text = format!("{value:.decimals$}");
        let rounded: f64 = text.parse().unwrap_or(f64::MAX);
        let limit = 10f64.powi(3 - decimals as i32);
        if rounded < limit {
            return format!("{text}{}", SUFFIXES[unit]);
        }
        if decimals == 0 && unit < SUFFIXES.len() - 1 {
            return format!("{:.2}{}", rounded / 1000.0, SUFFIXES[unit + 1]);
        }
    }
    format!("{value:.0}{}", SUFFIXES[unit])
}

/// Tree `%` column: one decimal, width 5.
pub fn percent_tree(fraction: f64) -> String {
    format!("{:>5.1}", fraction * 100.0)
}

/// Map tile label: whole percent, `<1%` for a nonzero share that rounds to 0.
/// `100%` only for an only child.
pub fn percent_tile(fraction: f64, only_child: bool) -> String {
    let pct = (fraction * 100.0).round();
    if fraction > 0.0 && pct < 1.0 {
        return "<1%".to_owned();
    }
    let pct = if pct >= 100.0 && !only_child {
        99.0
    } else {
        pct
    };
    format!("{pct:.0}%")
}

/// `~MM:SS`: an ETA always starts with `~`.
pub fn eta(d: Duration) -> String {
    format!("~{}", elapsed(d))
}

/// `MM:SS`, or `H:MM:SS` from one hour on.
pub fn elapsed(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// `1,204,331`.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The system's message for an errno, lowercase and without the `(os error N)` tail.
pub fn strerror(errno: i32) -> String {
    let text = std::io::Error::from_raw_os_error(errno).to_string();
    text.split(" (os error").next().unwrap_or("").to_lowercase()
}

/// Cuts `s` from the left to at most `max` cells, starting with `ellipsis` when it was cut.
pub fn truncate_left(s: &str, max: usize, ellipsis: &str) -> String {
    if width(s) <= max {
        return s.to_owned();
    }
    let mut tail: Vec<char> = Vec::new();
    let mut used = width(ellipsis);
    for c in s.chars().rev() {
        let w = width(&c.to_string());
        if used + w > max {
            break;
        }
        used += w;
        tail.push(c);
    }
    tail.reverse();
    format!("{ellipsis}{}", tail.into_iter().collect::<String>())
}

/// Cuts `s` in the middle to at most `max` cells, keeping the start and the end around `ellipsis`.
pub fn truncate_middle(s: &str, max: usize, ellipsis: &str) -> String {
    if width(s) <= max {
        return s.to_owned();
    }
    let room = max.saturating_sub(width(ellipsis));
    let (head_room, tail_room) = (room.div_ceil(2), room / 2);
    let mut head = String::new();
    for c in s.chars() {
        if width(&head) + width(&c.to_string()) > head_room {
            break;
        }
        head.push(c);
    }
    let tail = truncate_left(s, tail_room, "");
    format!("{head}{ellipsis}{tail}")
}

/// Display width, ignoring nothing: wide characters count 2 cells.
pub fn width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Cuts `s` to at most `max` cells, ending with `ellipsis` when it was cut.
pub fn truncate(s: &str, max: usize, ellipsis: &str) -> String {
    if width(s) <= max {
        return s.to_owned();
    }
    let budget = max.saturating_sub(width(ellipsis));
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        used += w;
        out.push(c);
    }
    if max >= width(ellipsis) {
        out.push_str(ellipsis);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn throughput_is_the_long_size_per_second() {
        assert_eq!(super::throughput(0), "0 B/s");
        assert_eq!(super::throughput(812), "812 B/s");
        assert_eq!(super::throughput(812 * 1024), "812.0 KiB/s");
        assert_eq!(super::throughput(67_318_580), "64.2 MiB/s");
        // The widest value fits the 11 cells reserved for it.
        assert_eq!(super::width(&super::throughput(1_048_470_000)), 11);
        assert!(super::width(&super::throughput(u64::MAX)) <= 12);
    }

    #[test]
    fn middle_truncation_keeps_both_ends_within_the_limit() {
        let cut = super::truncate_middle("abcdefghijklmnopqrstuvwxyz/", 15, "…");
        assert_eq!(cut, "abcdefg…uvwxyz/");
        assert_eq!(super::width(&cut), 15);
        assert_eq!(super::truncate_middle("short/", 15, "…"), "short/");
    }

    use super::*;

    #[test]
    fn compact_size_follows_spec_7_1() {
        for (bytes, want) in [
            (0, "0B"),
            (812, "812B"),
            (1023, "1023B"),
            (1024, "1.0K"),
            (831_488, "812.0K"),
            (1_048_535, "1.0M"),
            (68_932_140_237, "64.2G"),
            (1u64 << 60, "1.0E"),
        ] {
            assert_eq!(size_iec(bytes), want, "{bytes}");
        }
    }

    #[test]
    fn compact_size_never_shows_1000_or_more_in_a_unit() {
        // 1000 KiB rounds to 1000.0 and moves up a unit.
        assert_eq!(size_iec(1000 * 1024), "1.0M");
        assert_eq!(size_iec(999 * 1024), "999.0K");
    }

    #[test]
    fn compact_size_is_at_most_six_cells_for_any_u64() {
        let mut probes = vec![0, 1, 1023, 1024, u64::MAX, u64::MAX - 1];
        for shift in 0..64 {
            let base = 1u64 << shift;
            probes.extend([base - 1, base, base + 1, base / 2 * 3]);
        }
        for bytes in probes {
            assert!(
                size_iec(bytes).chars().count() <= 6,
                "{bytes} -> {}",
                size_iec(bytes)
            );
        }
    }

    #[test]
    fn long_size() {
        assert_eq!(size_iec_long(0), "0 B");
        assert_eq!(size_iec_long(812), "812 B");
        assert_eq!(size_iec_long(831_488), "812.0 KiB");
        assert_eq!(size_iec_long(33_285_996_544), "31.0 GiB");
        assert_eq!(size_iec_long(1_048_535), "1.0 MiB");
    }

    #[test]
    fn compact_count() {
        for (n, want) in [
            (0, "0"),
            (812, "812"),
            (999, "999"),
            (1_000, "1.00k"),
            (12_400, "12.4k"),
            (412_000, "412k"),
            (999_960, "1.00M"),
            (9_996, "10.0k"),
            (1_240_000, "1.24M"),
            (41_200, "41.2k"),
            (2_500_000_000, "2.50G"),
        ] {
            assert_eq!(count_compact(n), want, "{n}");
        }
    }

    #[test]
    fn percents() {
        assert_eq!(percent_tree(1.0), "100.0");
        assert_eq!(percent_tree(0.352), " 35.2");
        assert_eq!(percent_tile(0.35, false), "35%");
        assert_eq!(percent_tile(0.004, false), "<1%");
        assert_eq!(percent_tile(0.0, false), "0%");
        assert_eq!(percent_tile(0.999, false), "99%");
        assert_eq!(percent_tile(1.0, true), "100%");
    }

    #[test]
    fn durations() {
        assert_eq!(elapsed(Duration::ZERO), "00:00");
        assert_eq!(elapsed(Duration::from_millis(72_900)), "01:12");
        assert_eq!(elapsed(Duration::from_secs(3599)), "59:59");
        assert_eq!(elapsed(Duration::from_secs(3600 + 62)), "1:01:02");
        assert_eq!(eta(Duration::from_secs(104)), "~01:44");
    }

    #[test]
    fn grouping() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(1_204_331), "1,204,331");
    }

    #[test]
    fn left_truncation_keeps_the_end() {
        assert_eq!(truncate_left("/a/b/c.txt", 20, "…"), "/a/b/c.txt");
        assert_eq!(
            truncate_left("/very/long/path/file.txt", 12, "…"),
            "…th/file.txt"
        );
        assert_eq!(width(&truncate_left("/日本語/ファイル名.txt", 9, "…")), 9);
    }

    #[test]
    fn strerror_is_lowercase_without_the_os_error_tail() {
        assert_eq!(strerror(5), "input/output error");
    }

    #[test]
    fn truncation_respects_display_width() {
        assert_eq!(truncate("hello", 5, "…"), "hello");
        assert_eq!(truncate("hello world", 6, "…"), "hello…");
        assert_eq!(width(&truncate("日本語のファイル名", 7, "…")), 7);
        assert_eq!(truncate("abc", 0, "…"), "");
        assert_eq!(truncate("abc", 1, "…"), "…");
    }
}
