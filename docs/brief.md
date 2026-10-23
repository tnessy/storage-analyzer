# Brief: storage-analyzer

Status: v0.5, 2026-10-23. Labels: **Fact** (user stated), **Assumption**, **Open question**.

## Problem
- **Fact (user):** Analyze, diagnose and clean up disk space starting from a root folder.
- **Fact (user):** Layout like WinDirStat/WizTree: directory tree breakdown plus a treemap-style visual of which directories take the most space.
- **Fact (user):** Existing tools fall short. ncdu lacks the desired layout. dust is closer, but its tree visualization is inverted from what they expect. baobab and filelight are closest but are GUI. Decision: build, not adopt (see Decisions).

## Users
- **Fact:** Technical user, Linux (CachyOS, fish shell).
- **Assumption:** Sole user. No distribution needs in v1.

## Goals
1. Pick a root folder and see where the space goes, with no blocking wait. (Fact)
2. Explore it in an interactive terminal UI: tree + treemap. (Fact)
3. Free space by deleting items, with confirmation. (Fact)

## Non-goals for v1
- **Fact:** Duplicate detection and old/unused-file filtering (deferred by user).
- **Fact:** Non-interactive/scriptable output (interactive only for now).
- **Assumption:** Windows/macOS, cloud/network storage, scheduled runs, history/growth over time, GUI/web UI, trash support, cleanup suggestions for known caches.

## Requirements

### Must
- **M1 Async live scan:** The scan runs in the background and the UI updates live as results arrive. The user never waits on a blank or frozen screen.
  - Given a large root, when the scan starts, then the UI is interactive within 1 second (**Assumption**: threshold) and sizes/tree/treemap update while scanning continues.
  - Given the scan is running, when I press navigation keys, then the UI responds without waiting for the scan.
- **M2 Progress with elapsed and ETA:** Visible progress indicator showing elapsed time and estimated time remaining.
  - Given a running scan, then elapsed time is shown and updates at least once per second.
  - Given a running scan, then an ETA is shown and is revised as the scan proceeds. When it can't be estimated yet, it says so instead of showing a made-up number.
  - Given the scan finishes, then the indicator shows done and total elapsed time.
  - **Open question (for architect):** A filesystem walk has no known total up front. How ETA is estimated (for example from used-space on the filesystem, or from a prior scan) is a design decision. The ETA's accuracy target is TBD; the user said benchmarks come later. The acceptance for ETA is that it is present, labeled as an estimate, and converges to 0 at completion.
- **M3 Tree view:** Navigable directory tree, largest first, with size and percent of parent. Orientation (**Fact**, confirmed by user): the root is at the top, children are listed beneath their parent, sorted by size descending by default.
  - Given the tree, when I expand/collapse or enter/leave a directory by keyboard, then sizes and percentages reflect that level.
- **M4 Treemap:** Terminal treemap of the current directory, area proportional to size.
  - Given a directory, then each child's area is proportional to its size within terminal-cell rounding, and labels fit or truncate.
  - Given the tree selection changes, then the treemap and tree stay in sync (**Assumption**).
- **M5 File-type breakdown:** Size grouped by file type/extension for the whole scanned root (**Fact**, confirmed: not scoped to the selected directory).
  - Given a completed or running scan, then I can view total size and count per type, sorted by size.
- **M6 Largest-files list:** Flat list of the largest files under the whole scanned root (**Fact**, confirmed).
  - Given a scan, then the top N files (N configurable or a sensible default) are listed with size and path, and update live during the scan.
- **M7 Delete with confirmation:** Delete a selected file or directory.
  - Given I select an item and trigger delete, then a confirmation shows full path, size, and file/dir count, and nothing is deleted until I confirm.
  - Given I decline, then nothing changes.
  - Given I confirm, then it is permanently removed (**Fact**, confirmed: no trash), sizes update, and any error (for example permissions) is shown and not silent.
  - **Fact (v0.5):** Delete is allowed on folders whose scan is unfinished. It is not blocked.
  - Given the target folder is still being scanned, when I trigger delete, then the confirmation shows size and counts as clearly labelled lower bounds ("at least N", with "≥"), and says the scan of that folder is incomplete.
  - Given the target is fully scanned, then exact values are shown with no "≥".
  - Given I confirm deletion of an unfinished folder, then everything under it is removed, including parts not yet scanned, and the scan of that subtree stops.
- **M11 Scan controls:** (**Fact**, v0.5)
  - Prioritise: zooming into a folder, or pressing `p` on it, moves that folder's scan to the front of the queue.
    - Given a folder not yet scanned, when I zoom into it or press `p`, then its contents begin appearing before other pending folders' scanning completes.
  - Skip/resume: `x` skips a folder's scan, `r` resumes it.
    - Given a folder, when I press `x`, then it is not scanned further and is shown as skipped with its size marked as a partial "≥" value. When I press `r`, scanning resumes and the mark clears on completion.
  - Shallow-first: scanning proceeds breadth-first so top-level sizes appear early.
    - Given a large root, then the first level's entries are listed before deeper levels are completed.
- **M12 Quit confirmation:** (**Fact**, v0.5)
  - Given the UI, when I press `q` or Ctrl-C, then it always asks for confirmation before quitting.
  - Given a delete is in progress, when I press `q` or Ctrl-C, then only `y` quits. Any other key returns to the UI and the delete continues.
  - Given a quit during a delete, then the UI says the delete may be left partial (**Assumption**: wording is for ux-designer).
- **M8 Accuracy:** Given a test tree, the total matches `du` in the same mode (disk usage), with hard links counted once and symlinks not followed by default.
- **M9 Safe by default:** Nothing is modified except through M7.
- **M10 Robustness:** Permission errors and files vanishing during the scan do not crash it. Skipped paths are reported.

### Should
- Option to cross filesystems (default is one filesystem; see Decisions 2026-10-23, v0.4). Promoted from Should to default behavior.
- Opt-in history file (off by default; see Decisions v0.4).
- Disk usage vs apparent size toggle.
- Stop/cancel a scan cleanly (partly covered by M11 skip).

### Could
- Export report. Rescan a subtree after a delete or on demand.

## Out of scope (v0.5)
- **Fact:** `--max-depth` option, rejected for now.

## Constraints
- **Fact:** Command name is a TBD placeholder.
- **Fact:** Linux, CachyOS, fish. Terminal tool. No language preference (architect decides).
- **Fact:** Performance benchmarks come later. No numeric scan-time target in v1. The binding performance requirement is non-blocking behavior (M1).
- **Assumption:** No root required. Works without network access or telemetry.

## Success metrics
- **Fact:** M1/M2 behaviors (live updates, elapsed and ETA) are the stated bar.
- **Open question:** Numeric targets (files/sec, scan time on the user's real data) to be set after benchmarking.

## Open questions
1. Benchmark dataset and numeric targets: later, by user.
2. ETA accuracy target and estimation method: architect proposes.

Closed in v0.3: tree orientation, whole-root scope for file types and largest files, permanent delete (see Decisions).

## Decisions

### 2026-10-23: Build a new tool rather than adopt an existing one
- Context: Existing tools were considered during discovery.
- Options: ncdu, dust, baobab/filelight, build new.
- Decision: Build.
- Evidence: User statement. ncdu lacks the layout, dust's tree is inverted from expectation, baobab/filelight are GUI only. I haven't verified these claims against the tools.
- Revisit if: A small config change or flag in dust or gdu is found to deliver the desired layout.
- Update v0.4: User noted `dust -r` (reverse) and still chose to build. Decision stands.

### 2026-10-23: v0.4 defaults (history, filesystem boundary)
- Context: Questions raised during design.
- Options: history file on/off by default; cross mount points or stay on one filesystem by default.
- Decision: History file is off by default and opt-in. The scan stays on one filesystem by default, with an option to cross mount points.
- Evidence: User decision.
- Revisit if: ETA quality needs prior-scan history to be useful (the architect may argue for it), or the user's typical roots span mounts.

### 2026-10-23: v0.5 layout and scan behavior
- Context: Direction selection and design questions.
- Options: Layouts A/B/C; block vs allow delete during scan; `--max-depth`.
- Decision: Layout A chosen. Delete allowed during scan with "≥" lower-bound counts. Zoom and `p` both prioritise a folder's scan. Scan controls `p`/`x`/`r`, shallow-first scanning, and the quit confirmation are in scope (M11, M12). `--max-depth` rejected for now. Command name is a TBD placeholder.
- Evidence: User decisions.
- Revisit if: Users delete the wrong thing because of partial counts, or large roots make shallow-first scanning too slow to show useful top-level sizes.

## Pending user input (as of v0.5)
- Confirmation of mockup A1 (still unconfirmed in what I've been told).

### 2026-10-23: Tree orientation, breakdown scope, delete mode
- Context: Open questions from v0.2.
- Options: tree orientation (root-top vs other); breakdown scope (whole root vs selected dir); delete (permanent vs trash).
- Decision: Root at top, children below, largest first by default. File-type breakdown and largest-files list cover the whole root. Permanent delete with confirmation, no trash.
- Evidence: User confirmation.
- Revisit if: User finds the orientation still wrong in the UX directions, or wants per-directory breakdowns or trash.

### 2026-10-23: v1 scope
- Context: Discovery answers.
- Decision: v1 includes live async scan, progress/ETA, tree, treemap, file-type breakdown, largest-files list, delete with confirmation. Duplicates and old files deferred. Interactive only.
- Evidence: User answers.
- Revisit if: Benchmarks show M1 can't be met, or the user adds scriptable output.
