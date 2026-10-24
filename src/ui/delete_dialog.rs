//! The delete confirmation (UX §5.3): the exact form and the "scan not finished" form.

use ratatui::Frame;

use super::{copy, dialog, format, sanitize};
use crate::app::delete_flow::DeletePrompt;
use crate::app::state::AppState;
use crate::types::{Kind, SizeMode};

fn plural(n: u64, one: &str, many: &str) -> String {
    format!(
        "{} {}",
        format::thousands(n),
        if n == 1 { one } else { many }
    )
}

fn summary(state: &AppState, prompt: &DeletePrompt) -> String {
    let check = &prompt.check;
    let what = if state.model.kind(prompt.target) == Kind::Dir {
        "Folder"
    } else {
        "File"
    };
    let mode = match state.size_mode {
        SizeMode::Disk => "disk usage",
        SizeMode::Apparent => "apparent size",
    };
    let at_least = if check.exact {
        String::new()
    } else {
        format!("{} ", state.glyphs.lower_bound)
    };
    let mut line = format!(
        "{what} · {at_least}{} ({mode})",
        format::size_iec_long(check.size_bytes)
    );
    if what == "Folder" {
        line.push_str(&format!(
            " · {at_least}{} · {at_least}{}",
            plural(check.files, "file", "files"),
            plural(check.dirs, "folder", "folders")
        ));
    }
    if !check.exact {
        line.push(' ');
        line.push_str(copy::DELETE_AT_LEAST);
    }
    line
}

fn largest_known(state: &AppState, prompt: &DeletePrompt) -> Vec<String> {
    let known: Vec<_> = prompt.check.largest_known.iter().flatten().collect();
    if known.is_empty() {
        return vec![copy::DELETE_NOTHING_LISTED.to_owned()];
    }
    let mut lines = vec![copy::DELETE_LARGEST.to_owned()];
    for &&id in &known {
        let slash = if state.model.kind(id) == Kind::Dir {
            "/"
        } else {
            ""
        };
        lines.push(format!(
            "  {:>9}  {}{slash}",
            format::size_iec_long(state.model.size(id, state.size_mode)),
            sanitize::display_name(state.model.name(id))
        ));
    }
    lines
}

fn title_and_lines(state: &AppState, prompt: &DeletePrompt) -> (&'static str, Vec<String>) {
    let check = &prompt.check;
    let mut lines = vec![
        sanitize::sanitize(&state.model.path(prompt.target).display().to_string()),
        String::new(),
        summary(state, prompt),
    ];
    if !check.exact {
        lines.push(String::new());
        lines.extend(largest_known(state, prompt));
    }
    if check.exact {
        if check.outside_links > 0 {
            lines.push(String::new());
            lines.push(copy::delete_hard_links(
                check.outside_links,
                &format::size_iec_long(check.freed_bytes_upper),
            ));
        }
        lines.extend([
            String::new(),
            copy::DELETE_WARNING.to_owned(),
            String::new(),
            copy::DELETE_ACTIONS.to_owned(),
        ]);
        return (copy::DELETE_TITLE, lines);
    }
    lines.extend([
        String::new(),
        copy::DELETE_UNFINISHED_WARNING.to_owned(),
        String::new(),
        copy::DELETE_HARD_LINKS_UNKNOWN.to_owned(),
        String::new(),
    ]);
    if prompt.prioritising {
        lines.push(format!(
            "{} {}",
            state.glyphs.prioritised,
            copy::DELETE_PRIORITISING
        ));
        lines.push(copy::DELETE_ACTIONS_PRIORITISING.to_owned());
    } else {
        lines.push(copy::DELETE_ACTIONS_UNFINISHED.to_owned());
    }
    (copy::DELETE_TITLE_UNFINISHED, lines)
}

pub fn render(frame: &mut Frame, state: &AppState, prompt: &DeletePrompt) {
    let (title, lines) = title_and_lines(state, prompt);
    dialog::render(frame, frame.area(), &state.glyphs, title, &lines);
}
