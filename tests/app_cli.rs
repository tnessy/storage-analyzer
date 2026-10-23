//! The built binary's behavior before any TUI starts (UX §5.5, §11 check 14).

use assert_cmd::Command;
use storage_analyzer::APP_ID;

const ALT_SCREEN_ON: &str = "\x1b[?1049h";

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_storage-analyzer"))
        .args(args)
        .timeout(std::time::Duration::from_secs(10))
        .output()
        .unwrap()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn assert_no_tui(out: &std::process::Output) {
    assert!(
        out.stdout.is_empty(),
        "stdout: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains(ALT_SCREEN_ON));
}

#[test]
fn missing_path_is_reported_without_a_tui() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    let out = run(&[missing.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        stderr(&out),
        format!(
            "{APP_ID}: {}: no such file or directory\n",
            missing.display()
        )
    );
    assert_no_tui(&out);
}

#[test]
fn a_file_is_not_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    let out = run(&[file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        stderr(&out),
        format!("{APP_ID}: {}: not a directory\n", file.display())
    );
    assert_no_tui(&out);
}

#[test]
fn an_unreadable_directory_is_permission_denied() {
    if rustix::process::geteuid().is_root() {
        eprintln!("skipped: root can read a mode 000 directory");
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let out = run(&[locked.to_str().unwrap()]);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        stderr(&out),
        format!("{APP_ID}: {}: permission denied\n", locked.display())
    );
    assert_no_tui(&out);
}

#[test]
fn invalid_option_values_are_usage_errors() {
    let cases: [(&[&str], String); 8] = [
        (&["--cell-aspect", "0.49"], aspect_msg("0.49")),
        (&["--cell-aspect", "4.01"], aspect_msg("4.01")),
        (&["--cell-aspect", "NaN"], aspect_msg("NaN")),
        (&["--cell-aspect", "inf"], aspect_msg("inf")),
        (&["--cell-aspect", "abc"], aspect_msg("abc")),
        (&["--threads", "0"], count_msg("--threads", "0")),
        (&["--top", "0"], count_msg("--top", "0")),
        (&["--top", "-3"], count_msg("--top", "-3")),
    ];
    for (args, message) in cases {
        let out = run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(
            stderr(&out),
            format!("{message}\nTry '{APP_ID} --help'.\n"),
            "{args:?}"
        );
        assert_no_tui(&out);
    }
}

fn aspect_msg(got: &str) -> String {
    format!("{APP_ID}: --cell-aspect must be a number from 0.5 to 4.0 (got \"{got}\")")
}

fn count_msg(flag: &str, got: &str) -> String {
    format!("{APP_ID}: {flag} must be a whole number, 1 or more (got \"{got}\")")
}

#[test]
fn only_the_first_invalid_flag_is_reported() {
    let out = run(&["--top", "0", "--threads", "0"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        stderr(&out),
        format!("{}\nTry '{APP_ID} --help'.\n", count_msg("--top", "0"))
    );
}

#[test]
fn boundary_values_pass_validation() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    for args in [
        ["--cell-aspect", "0.5"],
        ["--cell-aspect", "4.0"],
        ["--threads", "1"],
        ["--top", "1"],
    ] {
        let mut all = args.to_vec();
        all.push(missing.to_str().unwrap());
        let out = run(&all);
        // Validation passed, so the run got as far as the path check.
        assert!(
            stderr(&out).ends_with("no such file or directory\n"),
            "{args:?}: {}",
            stderr(&out)
        );
    }
}

#[test]
fn help_names_the_binary_and_every_option() {
    let out = run(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let help = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(help.contains(APP_ID));
    for flag in [
        "--one-file-system",
        "--cross-filesystems",
        "--apparent",
        "--threads",
        "--top",
        "--history",
        "--no-animation",
        "--cell-aspect",
    ] {
        assert!(help.contains(flag), "{flag}");
    }
}
