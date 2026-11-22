//! Command line (design §4 CLI, UX §5.5 usage errors).

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser};

use crate::scan::ScanOptions;

const DEFAULT_TOP: usize = 100;
const CELL_ASPECT_RANGE: std::ops::RangeInclusive<f32> = 0.5..=4.0;

#[derive(Parser, Debug)]
#[command(about = "Disk-usage TUI: tree plus treemap, live while it scans")]
struct RawArgs {
    /// Directory to analyze
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Stay on the root's filesystem (default: also enter other local ones, not proc/sys/dev or network)
    #[arg(short = 'x', long = "one-file-system")]
    one_file_system: bool,
    /// Descend into every filesystem, including proc, sys and network mounts
    #[arg(long, conflicts_with = "one_file_system")]
    cross_filesystems: bool,
    /// Show apparent size instead of disk usage
    #[arg(long)]
    apparent: bool,
    /// Scan threads (default: min(cores, 8))
    #[arg(long, value_name = "N", allow_hyphen_values = true)]
    threads: Option<String>,
    /// Number of entries in the Largest view
    #[arg(long, value_name = "N", allow_hyphen_values = true)]
    top: Option<String>,
    /// Remember scans between sessions (writes one small file under ~/.local/state)
    #[arg(long)]
    history: bool,
    /// Freeze the spinner
    #[arg(long)]
    no_animation: bool,
    /// Terminal cell height/width, 0.5 to 4.0
    #[arg(
        long,
        value_name = "F",
        default_value = "2.0",
        allow_hyphen_values = true
    )]
    cell_aspect: String,
}

/// Validated settings every later task reads.
#[derive(Clone, Debug, PartialEq)]
pub struct AppConfig {
    pub path: PathBuf,
    /// `-x`: never leave the root's filesystem.
    pub one_filesystem: bool,
    /// Default: skip pseudo and network filesystems (off with `--cross-filesystems`).
    pub skip_pseudo: bool,
    pub apparent: bool,
    pub threads: usize,
    pub top: usize,
    pub history: bool,
    pub no_animation: bool,
    pub cell_aspect: f32,
}

impl AppConfig {
    pub fn scan_options(&self) -> ScanOptions {
        ScanOptions {
            threads: self.threads,
            one_filesystem: self.one_filesystem,
            skip_pseudo: self.skip_pseudo,
        }
    }
}

#[derive(Debug)]
pub enum CliError {
    /// A value outside its allowed range; the message is the first line of the usage error.
    Usage(String),
    /// Unknown flag, `--help` and the like; clap prints it.
    Clap(clap::Error),
}

pub fn parse<I, T>(bin: &'static str, argv: I) -> Result<AppConfig, CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cmd = RawArgs::command().name(bin).bin_name(bin);
    let matches = cmd.try_get_matches_from(argv).map_err(CliError::Clap)?;
    let raw = RawArgs::from_arg_matches(&matches).map_err(CliError::Clap)?;
    validate(bin, &matches, raw).map_err(CliError::Usage)
}

fn validate(bin: &str, matches: &ArgMatches, raw: RawArgs) -> Result<AppConfig, String> {
    let mut checks: Vec<(usize, Result<(), String>)> = Vec::new();
    let mut threads = ScanOptions::default_threads();
    let mut top = DEFAULT_TOP;
    if let Some(v) = &raw.threads {
        let parsed = parse_count(bin, "--threads", v);
        if let Ok(n) = parsed {
            threads = n;
        }
        checks.push((index(matches, "threads"), parsed.map(drop)));
    }
    if let Some(v) = &raw.top {
        let parsed = parse_count(bin, "--top", v);
        if let Ok(n) = parsed {
            top = n;
        }
        checks.push((index(matches, "top"), parsed.map(drop)));
    }
    let aspect = parse_aspect(bin, &raw.cell_aspect);
    let cell_aspect = *aspect.as_ref().unwrap_or(&2.0);
    if matches.value_source("cell_aspect") == Some(clap::parser::ValueSource::CommandLine) {
        checks.push((index(matches, "cell_aspect"), aspect.map(drop)));
    }
    checks.sort_by_key(|(i, _)| *i);
    if let Some((_, Err(msg))) = checks.into_iter().find(|(_, r)| r.is_err()) {
        return Err(msg);
    }
    Ok(AppConfig {
        path: raw.path,
        one_filesystem: raw.one_file_system,
        skip_pseudo: !raw.cross_filesystems,
        apparent: raw.apparent,
        threads,
        top,
        history: raw.history,
        no_animation: raw.no_animation,
        cell_aspect,
    })
}

fn index(matches: &ArgMatches, id: &str) -> usize {
    matches.index_of(id).unwrap_or(usize::MAX)
}

fn parse_count(bin: &str, flag: &str, value: &str) -> Result<usize, String> {
    match value.parse::<usize>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(format!(
            "{bin}: {flag} must be a whole number, 1 or more (got \"{value}\")"
        )),
    }
}

fn parse_aspect(bin: &str, value: &str) -> Result<f32, String> {
    match value.parse::<f32>() {
        Ok(f) if f.is_finite() && CELL_ASPECT_RANGE.contains(&f) => Ok(f),
        _ => Err(format!(
            "{bin}: --cell-aspect must be a number from 0.5 to 4.0 (got \"{value}\")"
        )),
    }
}

/// The two-line usage error printed to stderr (exit code 2).
pub fn usage_error_text(bin: &str, message: &str) -> String {
    format!("{message}\nTry '{bin} --help'.")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIN: &str = "prog";

    fn parse_args(args: &[&str]) -> Result<AppConfig, String> {
        let argv = std::iter::once("prog").chain(args.iter().copied());
        match parse(BIN, argv) {
            Ok(c) => Ok(c),
            Err(CliError::Usage(m)) => Err(m),
            Err(CliError::Clap(e)) => Err(format!("clap: {e}")),
        }
    }

    #[test]
    fn defaults() {
        let c = parse_args(&[]).unwrap();
        assert_eq!(c.path, PathBuf::from("."));
        assert!(!c.one_filesystem && c.skip_pseudo && !c.apparent && !c.history && !c.no_animation);
        assert_eq!((c.top, c.cell_aspect), (100, 2.0));
        assert!((1..=8).contains(&c.threads));
    }

    #[test]
    fn all_flags_parse() {
        let c = parse_args(&[
            "/tmp",
            "--cross-filesystems",
            "--apparent",
            "--threads",
            "3",
            "--top",
            "7",
            "--history",
            "--no-animation",
            "--cell-aspect",
            "1.5",
        ])
        .unwrap();
        assert!(!c.one_filesystem && !c.skip_pseudo && c.apparent && c.history && c.no_animation);
        assert_eq!(
            (c.path, c.threads, c.top, c.cell_aspect),
            ("/tmp".into(), 3, 7, 1.5)
        );
        assert!(parse_args(&["-x"]).unwrap().one_filesystem);
    }

    #[test]
    fn one_filesystem_and_cross_filesystems_conflict() {
        assert!(
            parse_args(&["-x", "--cross-filesystems"])
                .unwrap_err()
                .starts_with("clap:")
        );
    }

    #[test]
    fn boundary_values_are_accepted() {
        for args in [
            ["--cell-aspect", "0.5"],
            ["--cell-aspect", "4.0"],
            ["--threads", "1"],
            ["--top", "1"],
        ] {
            assert!(parse_args(&args).is_ok(), "{args:?}");
        }
    }

    #[test]
    fn invalid_values_name_the_flag_and_value() {
        for v in ["0.49", "4.01", "NaN", "inf", "abc"] {
            assert_eq!(
                parse_args(&["--cell-aspect", v]).unwrap_err(),
                format!("prog: --cell-aspect must be a number from 0.5 to 4.0 (got \"{v}\")")
            );
        }
        assert_eq!(
            parse_args(&["--threads", "0"]).unwrap_err(),
            "prog: --threads must be a whole number, 1 or more (got \"0\")"
        );
        assert_eq!(
            parse_args(&["--top", "-3"]).unwrap_err(),
            "prog: --top must be a whole number, 1 or more (got \"-3\")"
        );
    }

    #[test]
    fn only_the_first_invalid_flag_in_command_line_order_is_reported() {
        let e = parse_args(&["--top", "0", "--threads", "0"]).unwrap_err();
        assert!(e.contains("--top"), "{e}");
        let e = parse_args(&["--cell-aspect", "9", "--top", "0"]).unwrap_err();
        assert!(e.contains("--cell-aspect"), "{e}");
    }

    #[test]
    fn usage_error_has_a_second_help_line() {
        assert_eq!(
            usage_error_text("prog", "prog: x"),
            "prog: x\nTry 'prog --help'."
        );
    }
}
