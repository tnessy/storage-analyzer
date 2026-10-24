//! Status line text (UX §5.2).

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

use super::theme::Glyphs;
use super::{ascii, copy, format, sanitize};
use crate::app::state::AppState;
use crate::eta::{EtaSource, EtaView};
use crate::types::{ScanState, SizeMode};

/// `▲ First: .cache/` before the scanning text while a folder has the priority.
fn with_priority(state: &AppState, width: usize) -> String {
    let Some(id) = state.priority.filter(|&p| state.model.exists(p)) else {
        return scanning_text(state, width);
    };
    let name = sanitize::display_name(state.model.name(id));
    let prefix = format!(
        "{} {} {name}/  ",
        state.glyphs.prioritised,
        copy::FIRST_PREFIX
    );
    // The bar goes first when space runs out: the scanning text is built for what is left.
    format!(
        "{prefix}{}",
        scanning_text(state, width.saturating_sub(format::width(&prefix)))
    )
}

/// Cells reserved for the throughput value (`999.9 MiB/s`).
const READ_VALUE_CELLS: usize = 11;

/// With no source the line says nothing about ETA for the first two seconds (ADR-010).
const ETA_TEXT_AFTER: Duration = Duration::from_secs(2);

/// While scanning: the text for whatever the estimator says (`width` decides what is dropped).
fn scanning_text(state: &AppState, width: usize) -> String {
    let glyph = state.spinner_glyph();
    let scan = state.phase();
    let word = if scan == ScanState::Starting {
        copy::STARTING
    } else {
        copy::SCANNING
    };
    let totals = state.model.totals();
    // `read_speed` is `None` outside a real first scan: never in Starting or a subtree rescan.
    let read_bps = state.read_speed();
    match state.eta_view() {
        EtaView::Estimating => estimating_line(glyph, word, state.elapsed, read_bps),
        EtaView::Eta {
            remaining,
            fraction,
            source,
        } => {
            let p = EtaProgress {
                glyph: glyph.to_owned(),
                word: word.to_owned(),
                elapsed: state.elapsed,
                fraction,
                source,
                read_bps,
            };
            eta_line(&p, Some(remaining), width)
        }
        EtaView::OverEstimate => {
            let p = EtaProgress {
                glyph: glyph.to_owned(),
                word: word.to_owned(),
                elapsed: state.elapsed,
                fraction: 0.99,
                source: EtaSource::History,
                read_bps,
            };
            eta_line(&p, None, width)
        }
        EtaView::Indeterminate {
            rate_eps,
            pending_dirs,
            top_done,
            top_total,
        } => {
            if state.elapsed < ETA_TEXT_AFTER {
                return format!(
                    "{glyph} {word}  {} {}",
                    format::elapsed(state.elapsed),
                    copy::ELAPSED
                );
            }
            let bytes = match state.size_mode {
                SizeMode::Disk => totals.disk_bytes,
                SizeMode::Apparent => totals.apparent_bytes,
            };
            indeterminate_line(
                &Progress {
                    glyph: glyph.to_owned(),
                    word: word.to_owned(),
                    elapsed: state.elapsed,
                    top: (top_total > 0).then_some((top_done, top_total)),
                    queued: pending_dirs,
                    entries: totals.entries_seen,
                    bytes,
                    rate_eps,
                    read_bps,
                },
                width,
            )
        }
        // The estimator only returns these once the scan has ended.
        EtaView::Done { .. } | EtaView::Stopped { .. } => String::new(),
    }
}

/// A leading symbol and its space; nothing at all when the glyph set has none (ASCII).
fn lead(glyph: &str) -> String {
    if glyph.is_empty() {
        String::new()
    } else {
        format!("{glyph} ")
    }
}

/// The status line: a transient message, or the state text with the other-filesystem hint.
pub fn text(state: &AppState, width: usize) -> String {
    if let Some(t) = &state.transient {
        return t.text.clone();
    }
    if let Some(run) = &state.delete_run {
        let path = sanitize::sanitize(&run.path.display().to_string());
        return copy::deleting(
            state.spinner_glyph(),
            &path,
            run.removed_files,
            run.total_files,
        );
    }
    match other_fs_hint(state) {
        Some((full, short)) => fit_hint(&|room| state_text(state, room), &full, &short, width),
        None => state_text(state, width),
    }
}

/// ` · not scanned (other fs): /home +2`, and the short ` · 3 other fs not scanned`.
fn other_fs_hint(state: &AppState) -> Option<(String, String)> {
    use std::os::unix::ffi::OsStrExt;
    if !state.config.one_filesystem {
        return None;
    }
    let mut skipped: Vec<(u32, Vec<u8>)> = state
        .model
        .issues()
        .iter()
        .filter(|i| {
            i.kind == crate::types::IssueKind::OtherFilesystem && state.model.exists(i.node)
        })
        .map(|i| {
            let path = state.model.path(i.node);
            (
                state.model.depth(i.node),
                path.as_os_str().as_bytes().to_vec(),
            )
        })
        .collect();
    skipped.sort();
    let (_, shallowest) = skipped.first()?;
    let path = sanitize::display_name(shallowest);
    let more = if skipped.len() > 1 {
        format!(" +{}", skipped.len() - 1)
    } else {
        String::new()
    };
    let full = copy::OTHER_FS_HINT
        .replace("{path}", &path)
        .replace("{more}", &more);
    let short = copy::OTHER_FS_HINT_SHORT.replace("{count}", &skipped.len().to_string());
    Some((full, short))
}

fn state_text(state: &AppState, width: usize) -> String {
    let g = &state.glyphs;
    let sep = g.separator;
    let elapsed = format::elapsed(state.elapsed);
    match state.phase() {
        ScanState::Starting | ScanState::Scanning if state.is_stopping() => {
            format!(
                "{}{}  {elapsed} {}",
                lead(g.stopped),
                copy::STOPPING,
                copy::ELAPSED
            )
        }
        ScanState::Starting | ScanState::Scanning => with_priority(state, width),
        ScanState::Done => {
            let t = state.model.totals();
            let bytes = match state.size_mode {
                crate::types::SizeMode::Disk => t.disk_bytes,
                crate::types::SizeMode::Apparent => t.apparent_bytes,
            };
            let mut line = format!(
                "{}Done in {elapsed} {sep} {} files {sep} {}",
                lead(g.done),
                format::thousands(t.files),
                format::size_iec_long(bytes)
            );
            let skipped = state.model.issues().len();
            if skipped > 0 {
                line.push_str(&format!(" {sep} {skipped} skipped (press 4)"));
            }
            line
        }
        ScanState::Stopped => {
            format!(
                "{}Stopped at {elapsed} {sep} {}",
                lead(g.stopped),
                copy::STOPPED_TAIL
            )
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let line = format::truncate(
        &sanitize::sanitize(&ascii::text(
            &state.glyphs,
            &text(state, usize::from(area.width)),
        )),
        area.width as usize,
        state.glyphs.ellipsis,
    );
    frame.render_widget(Paragraph::new(line), area);
}

/// What the no-source (`Indeterminate`) line is made of: true quantities only.
#[derive(Clone, Debug)]
pub struct Progress {
    pub glyph: String,
    pub word: String,
    pub elapsed: Duration,
    /// `(done, total)` of the scan root's folders; `None` until the root is listed.
    pub top: Option<(u32, u32)>,
    pub queued: u64,
    pub entries: u64,
    pub bytes: u64,
    pub rate_eps: Option<f64>,
    /// Disk read speed in bytes/s; `None` hides the field.
    pub read_bps: Option<u64>,
}

/// Which optional fields of the `Indeterminate` line are still shown (UX §5.2 drop order).
#[derive(Clone, Copy)]
struct Shown {
    rate: bool,
    bytes: bool,
    entries: bool,
    qualifier: bool,
    short_queued: bool,
    short_top: bool,
    queued: bool,
    /// `12/31 top` (only used while the read field is shown).
    tiny_top: bool,
    read: bool,
    top: bool,
}

const FULL: Shown = Shown {
    rate: true,
    bytes: true,
    entries: true,
    qualifier: true,
    short_queued: false,
    short_top: false,
    queued: true,
    tiny_top: false,
    read: true,
    top: true,
};

/// The states between full and nothing optional left, in the spec's order. The read field adds
/// one shortening (`top-level done` -> `top`) before it goes; without it the order is unchanged.
fn drop_steps(with_read: bool) -> Vec<Shown> {
    let mut edits: Vec<fn(&mut Shown)> = vec![
        |s| s.rate = false,
        |s| s.bytes = false,
        |s| s.entries = false,
        |s| s.qualifier = false,
        |s| s.short_queued = true,
        |s| s.short_top = true,
        |s| s.queued = false,
    ];
    if with_read {
        edits.push(|s| s.tiny_top = true);
        edits.push(|s| s.read = false);
    }
    edits.push(|s| s.top = false);
    let mut steps = vec![FULL];
    for edit in edits {
        let mut next = *steps.last().expect("steps start with FULL");
        edit(&mut next);
        steps.push(next);
    }
    steps
}

fn indeterminate_with(p: &Progress, shown: Shown) -> String {
    let sep = Glyphs::unicode().separator;
    let mut line = format!(
        "{} {}  {} {}",
        p.glyph,
        p.word,
        format::elapsed(p.elapsed),
        copy::ELAPSED
    );
    let mut add = |field: String| {
        line.push_str(&format!(" {sep} {field}"));
    };
    if let (true, Some((done, total))) = (shown.top, p.top) {
        add(if shown.tiny_top {
            format!("{done}/{total} {}", copy::TOP_LEVEL_SHORT)
        } else if shown.short_top {
            format!("{done}/{total} {}", copy::TOP_LEVEL_DONE)
        } else {
            format!("{done} {} {total} {}", copy::OF, copy::TOP_LEVEL_DONE)
        });
    }
    if shown.queued && p.queued > 0 {
        let word = if shown.short_queued {
            copy::QUEUED
        } else {
            copy::FOLDERS_QUEUED
        };
        add(format!(
            "{}{} {word}",
            Glyphs::unicode().lower_bound,
            format::count_compact(p.queued)
        ));
    }
    if shown.entries && p.entries > 0 {
        add(format!(
            "{} {}",
            format::count_compact(p.entries),
            copy::ENTRIES
        ));
    }
    if shown.bytes && p.entries > 0 {
        add(format::size_iec_long(p.bytes));
    }
    if let (true, Some(rate)) = (shown.rate, p.rate_eps) {
        add(format!("{}/s", format::count_compact(rate as u64)));
    }
    if let (true, Some(read)) = (shown.read, p.read_bps) {
        add(read_field(read));
    }
    add(if shown.qualifier {
        copy::ETA_UNKNOWN.to_owned()
    } else {
        copy::ETA_UNKNOWN_SHORT.to_owned()
    });
    line
}

/// The `Indeterminate` line, dropping fields in the order of UX §5.2 until it fits `width`.
/// The glyph and word, elapsed and `ETA unknown` are never dropped.
pub fn indeterminate_line(p: &Progress, width: usize) -> String {
    let steps = drop_steps(p.read_bps.is_some());
    steps
        .iter()
        .map(|&s| indeterminate_with(p, s))
        .find(|l| format::width(l) <= width)
        .unwrap_or_else(|| indeterminate_with(p, steps[steps.len() - 1]))
}

/// `disk  64.2 MiB/s`: the label and the value right-aligned in the width of `999.9 MiB/s`, so
/// nothing after it moves when the number changes.
fn read_field(bytes_per_s: u64) -> String {
    format!(
        "{} {:>width$}",
        copy::DISK_LABEL,
        format::throughput(bytes_per_s),
        width = READ_VALUE_CELLS
    )
}

/// What the with-source lines are made of.
#[derive(Clone, Debug)]
pub struct EtaProgress {
    pub glyph: String,
    pub word: String,
    pub elapsed: Duration,
    pub fraction: f32,
    pub source: EtaSource,
    /// Disk read speed in bytes/s; `None` hides the field.
    pub read_bps: Option<u64>,
}

fn bar(fraction: f32, cells: usize, g: &Glyphs) -> String {
    let eighths = (f64::from(fraction).clamp(0.0, 1.0) * (cells * 8) as f64).round() as usize;
    let (full, part) = (eighths / 8, eighths % 8);
    let mut out = g.bar_full.repeat(full.min(cells));
    if part > 0 && full < cells {
        out.push_str(g.bar_eighths[part]);
    }
    let used = out.chars().count();
    out.push_str(&g.bar_empty.repeat(cells.saturating_sub(used)));
    out
}

/// `ETA ~01:44 (estimate, last scan)`, or `ETA unknown (past estimate)` when `remaining` is `None`.
/// Narrow widths: the bar shrinks, `elapsed` goes, then the bar goes.
pub fn eta_line(p: &EtaProgress, remaining: Option<Duration>, width: usize) -> String {
    let g = Glyphs::unicode();
    let sep = g.separator;
    let eta = match remaining {
        Some(r) => format!(
            "{} {} ({}, {})",
            copy::ETA_WORD,
            format::eta(r),
            copy::ESTIMATE_WORD,
            match p.source {
                EtaSource::FsInodes => copy::SOURCE_FILESYSTEM,
                EtaSource::History | EtaSource::SessionPrevious => copy::SOURCE_LAST_SCAN,
            }
        ),
        None => copy::ETA_PAST_ESTIMATE.to_owned(),
    };
    let pct = (f64::from(p.fraction) * 100.0).round().min(99.0) as u32;
    // When past the estimate the bar is full but the label still says 99.
    let shown = if remaining.is_none() { 1.0 } else { p.fraction };
    let elapsed = format::elapsed(p.elapsed);
    let head = format!("{} {}", p.glyph, p.word);
    // Placed just before `ETA`, separated by two spaces; the bar goes before the read field.
    let read = p
        .read_bps
        .map(|r| format!("{}  ", read_field(r)))
        .unwrap_or_default();
    let wide = format!(
        "{head} {} ~{pct}%  {elapsed} {}  {read}{eta}",
        bar(shown, 16, &g),
        copy::ELAPSED
    );
    let narrow = format!(
        "{head} {} ~{pct}%  {elapsed} {}  {read}{eta}",
        bar(shown, 10, &g),
        copy::ELAPSED
    );
    let short = format!(
        "{head} {} ~{pct}% {} {elapsed}  {read}{eta}",
        bar(shown, 10, &g),
        sep
    );
    let bare = format!("{head} ~{pct}% {} {elapsed}  {read}{eta}", sep);
    let no_read = format!("{head} ~{pct}% {} {elapsed}  {eta}", sep);
    let mut all = vec![wide, narrow, short, bare];
    if p.read_bps.is_some() {
        all.push(no_read);
    }
    let last = all[all.len() - 1].clone();
    all.into_iter()
        .find(|l| format::width(l) <= width)
        .unwrap_or(last)
}

/// `◌ Scanning  01:12 elapsed  ETA estimating…`: a source exists but the rate has not settled.
pub fn estimating_line(
    glyph: &str,
    word: &str,
    elapsed: Duration,
    read_bps: Option<u64>,
) -> String {
    let read = read_bps
        .map(|r| format!("{}  ", read_field(r)))
        .unwrap_or_default();
    format!(
        "{glyph} {word}  {} {}  {read}{}",
        format::elapsed(elapsed),
        copy::ELAPSED,
        copy::ETA_ESTIMATING
    )
}

/// Appends the other-filesystem hint to a status line (UX §5.2): the full hint when it fits, with
/// the bar dropped first (the line is built for the room that is left); otherwise the short hint.
pub fn fit_hint(
    line_for: &dyn Fn(usize) -> String,
    full: &str,
    short: &str,
    width: usize,
) -> String {
    let line = line_for(width.saturating_sub(format::width(full)));
    if format::width(&line) + format::width(full) <= width {
        return format!("{line}{full}");
    }
    format!(
        "{}{short}",
        line_for(width.saturating_sub(format::width(short)))
    )
}
