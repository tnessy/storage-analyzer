# Acceptance runbook: brief v0.6 M1–M12, the built "Should" items and the UX §11 checks

Written 2026-10-23 for the project owner and the user (T034, engineer-2). Docs only: no source or test was changed.
Every test name below was checked against `cargo test --lib --tests -- --list` at the time of writing.
A name has the form `file::test_name` (an integration test in `tests/file.rs`); `lib::path::test` is a unit test inside `src/`.

## How to run things

| What | Command |
|---|---|
| One integration test | `cargo test --test FILE -- TEST_NAME` (add `--exact` for an exact match) |
| One unit test | `cargo test --lib -- PATH::TEST_NAME` |
| Everything, as the README does | the five commands in "Full run" below |
| Show the measured `TARGET:` lines | `cargo test --release --all-targets -- --nocapture 2>&1 \| grep TARGET:` |

### Scratch tree for the manual checks (the only place anything is ever deleted)
```
rm -rf /tmp/sa-accept && mkdir -p /tmp/sa-accept/{media,docs,cache/sub,empty} && cd /tmp/sa-accept
head -c 40000000 /dev/urandom > media/movie.mkv
head -c 12000000 /dev/urandom > media/clip.mp4
head -c 9000000  /dev/urandom > cache/sub/blob.bin
head -c 3000000  /dev/urandom > docs/archive.zip
for i in $(seq 1 40); do head -c 20000 /dev/urandom > docs/note$i.txt; done
ln media/clip.mp4 cache/clip-link.mp4        # a hard link
ln -s /usr /tmp/sa-accept/link-to-usr         # a symlink that must NOT be followed
mkdir -p locked && head -c 100000 /dev/urandom > locked/secret.bin && chmod 000 locked
```
Clean up afterwards: `chmod 700 /tmp/sa-accept/locked; rm -rf /tmp/sa-accept`.
Run the program as `cargo run --release -- /tmp/sa-accept` (or `./target/release/storage-analyzer /tmp/sa-accept`).
Terminal: at least 80×24.
**Never press `y` in a delete dialog unless the path shown starts with `/tmp/sa-accept`.**

## Traceability

| Req | Automated evidence (section) | Manual |
|---|---|---|
| M1 | [M1](#m1) | MC-1 |
| M2 | [M2](#m2) | MC-2 |
| M3 | [M3](#m3) | MC-3 |
| M4 | [M4](#m4) | MC-4 |
| M5 | [M5](#m5) | MC-5 |
| M6 | [M6](#m6) | MC-6 |
| M7 | [M7](#m7) | MC-7 |
| M8 | [M8](#m8) | MC-8 |
| M9 | [M9](#m9) | MC-9 |
| M10 | [M10](#m10) | MC-10 |
| M11 | [M11](#m11) | MC-11 |
| M12 | [M12](#m12) | MC-12 |
| Should: one filesystem, history, size toggle, stop | [Should](#should) | MC-13 to MC-16 |
| UX §11 checks 1–22 | [UX §11](#ux-11-checks) | (covered above) |

---

## M1
**Requirement:** async live scan; the UI never waits on a blank or frozen screen; interactive within 1 s; keys respond during the scan.

**Automated**
- `app_loop::m1_frame_zero_is_drawn_before_the_scanner_starts_and_within_one_second` (the one timing gate: first frame ≤ 1 s)
- `app_loop::ui_never_waits_for_a_silent_scanner`
- `app_loop::input_is_handled_before_a_message_flood_is_applied`
- `app_loop::data_redraws_are_capped_at_ten_per_second_under_a_flood`
- `app_loop::input_latency_during_a_flood_is_measured` (prints `TARGET: input_p95_ms`, not a gate)
- `scan_walker::single_thread_lists_shallow_directories_first`, `scan_walker::listings_follow_the_ordering_contract_and_cover_the_tree`

Run: `cargo test --test app_loop` (add `-- --nocapture` to see the `TARGET:` lines).

**MC-1 (manual)**
1. `cargo run --release -- /usr` (read only; nothing is deleted).
2. A frame with the header and `Starting`/`Scanning` appears at once (well under a second), not a blank screen.
3. While the status line still says `Scanning`, press `↓`, `→`, `Tab`, `2`, `1`: the screen responds immediately.
4. Sizes in the tree and the map keep growing until `Done`.

## M2
**Requirement:** elapsed and ETA; no made-up numbers; ETA only when a source exists (history, same-session rescan, filesystem inode count); otherwise indeterminate with `ETA unknown`.

**Automated**
- Estimator: `lib::eta::tests::no_source_is_indeterminate_never_estimating`, `lib::eta::tests::settled_source_gives_eta`, `lib::eta::tests::fraction_is_clamped_below_one_before_done`, `lib::eta::tests::never_non_finite_and_fraction_capped`, `lib::eta::tests::done_and_stopped_win_over_everything`, `lib::eta::tests::constant_rate_scan_with_exact_source_converges_to_done`, `lib::eta::tests::source_arriving_mid_scan_never_goes_back_to_indeterminate`, `lib::eta::sources::tests::the_best_source_follows_the_priority_order`, `lib::eta::sources::tests::inode_counts_need_the_mount_root_and_a_filesystem_that_counts`
- Status line: `app_eta::no_source_means_no_percent_bar_or_eta_digits_at_any_width`, `app_eta::with_no_source_the_line_carries_true_quantities_and_no_estimate`, `app_eta::the_estimate_always_carries_a_tilde_and_never_reads_one_hundred_before_done`, `app_eta::the_eta_line_has_a_bar_percent_elapsed_and_a_labelled_estimate`, `app_eta::glyph_word_elapsed_and_eta_unknown_are_never_dropped`, `app_eta::at_80_columns_it_is_the_specs_example_line`, `app_eta::the_inode_count_is_the_source_when_nothing_better_exists`, `app_eta::a_finished_root_scan_becomes_the_in_session_source_for_a_rescan`
- Elapsed and Done: `app_loop::elapsed_text_changes_in_every_second_of_a_ten_second_run`, `app_loop::elapsed_freezes_when_the_scan_finishes`, `app_loop::done_and_stopped_status_text`, `app_loop::eta_text_appears_only_after_two_seconds_without_a_source`

Run: `cargo test --test app_eta`, `cargo test --test app_loop`, `cargo test --lib -- eta::`.

**MC-2 (manual)**
1. `cargo run --release -- /usr`. With no history and no source (on btrfs there is no inode count) the line shows `Scanning`, an elapsed clock that moves every second, counts, a rate and `ETA unknown`; no `%`, no bar.
2. Let it finish: the line says `Done` with the total time.
3. Press `r` on the root after `Done`: this second scan in the same session shows an estimate marked `~` and `estimate`, with the source `last scan`, reaching 00:00 at Done.
4. Optional: `cargo run --release -- --history /usr` twice; the second run shows an estimate from the first one.

**Known limit (ADR-010, M2 revision):** on a first scan with no source (for example btrfs, history off) the answer is `ETA unknown`; this is by design. The amendment to M2 is with the project owner.

## M3
**Requirement:** navigable tree, largest first, size and percent of parent; expand/collapse/enter/leave.

**Automated:** `ui_tree::children_are_sorted_by_size_then_name`, `ui_tree::percent_is_relative_to_each_rows_parent_and_zoom_rebases_it`, `ui_tree::keys_expand_collapse_and_move`, `ui_tree::sizes_percentages_and_order_follow_the_scan`, `ui_tree::selection_follows_the_item_through_live_resorting`, `ui_tree::page_keys_move_by_a_page_and_the_viewport_follows`, `ui_tree::zoomed_header_shows_a_breadcrumb`, `ui_tree::backspace_from_a_deep_zoom_selects_the_folder_left_at_each_level`, `ui_tree::scanning_screen_layout_snapshot`, `ui_tree::a_big_folder_shows_fifty_rows_and_an_aggregate_that_reveals_more`

Run: `cargo test --test ui_tree`.

**MC-3 (manual)**
1. `cargo run --release -- /tmp/sa-accept`. The root is the first row; `media/` is listed first (largest), with sizes and percentages.
2. `→` on `media/` expands it; percentages are relative to `media/`. `←` collapses.
3. `⏎` on `media/` zooms in (a `..` row appears on top); `⌫` goes back up.

## M4
**Requirement:** treemap of the current directory, area proportional to size; tree and map in sync.

**Automated:** `ui_treemap::tile_areas_are_proportional_within_cell_rounding`, `lib::layout::squarify::tests::proportional_within_rounding`, `lib::layout::squarify::tests::layout_contract`, `ui_treemap::tree_selection_moves_the_double_border_and_scope_follows_the_parent`, `ui_treemap::map_arrows_move_the_one_shared_selection_and_scroll_the_tree`, `ui_treemap::long_labels_are_truncated_inside_their_tile`, `ui_treemap::tile_label_has_name_size_percent_and_category_tag`, `ui_treemap::enter_and_backspace_zoom_from_the_map_like_the_tree`, `ui_treemap::map_snapshots_for_each_state`

Run: `cargo test --test ui_treemap`; `cargo test --lib -- layout::`.

**MC-4 (manual)**
1. In the map pane (bottom), `media/` has the largest tile (about 70 % of the area).
2. Move the tree selection with `↑`/`↓`: the double border in the map follows. `Tab` switches to the map; arrows move the same selection.
3. Make the terminal narrower: labels shorten, nothing wraps outside its tile.

## M5
**Requirement:** size and count per file type for the whole root, sorted by size.

**Automated:** `ui_lists::types_view_matches_a_brute_force_recount_of_the_whole_root`, `ui_lists::types_change_live_while_scanning`, `ui_lists::types_ignore_the_tree_selection`, `ui_lists::expanding_a_category_lists_its_extensions_and_collapsing_hides_them`, `model_indexes::totals_and_top_lists_are_correct_after_every_message`, `model_indexes::category_table_covers_the_documented_groups`, `model_indexes::ext_of_follows_the_rules`

Run: `cargo test --test ui_lists`, `cargo test --test model_indexes`.

**MC-5 (manual):** press `2` (Types). Rows for Video (`.mkv`, `.mp4`), Archives, Code & text and so on, sorted by size, each with a count. `→` on a category lists its extensions. Select a different folder in the tree (press `1`, move, press `2`): the list does not change.

## M6
**Requirement:** top N largest files for the whole root, live.

**Automated:** `ui_lists::largest_is_live_whole_root_and_limited_to_top_n`, `ui_lists::enter_on_largest_shows_the_file_in_the_tree`, `ui_lists::long_paths_are_cut_from_the_left_so_the_file_name_stays`, `model_indexes::top_n_is_honored`, `model_indexes::ties_break_by_node_id`, `model_indexes::recompute_top_matches_the_incremental_lists`, `top_reserve::the_lists_always_match_a_recount`, `top_reserve::the_reserve_absorbs_a_delete_that_takes_some_of_the_top`, `top_reserve::a_delete_that_empties_the_reserve_rebuilds_and_ends_correct`

Run: `cargo test --test ui_lists`, `cargo test --test top_reserve`, `cargo test --test model_indexes`.

**MC-6 (manual):** `cargo run --release -- --top 3 /tmp/sa-accept`, press `3`: exactly three files (`movie.mkv`, `clip.mp4`, `blob.bin`), with sizes and paths. `⏎` on one selects it in the tree.

## M7
**Requirement:** delete with confirmation (path, size, counts); nothing happens until confirmed; permanent; errors shown; allowed on unfinished folders with `≥` lower bounds; the scan of that subtree stops.

**Automated**
- Dialog and keys: `app_delete::exact_dialog_shows_the_numbers_and_only_y_deletes`, `app_delete::a_file_gets_the_file_variant`, `app_delete::unfinished_target_shows_lower_bounds_and_p_asks_for_the_scan_first`, `app_delete::hard_link_line_appears_only_when_links_leave_the_folder`, `app_delete::hard_link_line_counts_files_in_the_plural`, `app_delete::dialogs::every_delete_dialog_in_unicode_and_ascii`, `app_delete::dialogs::ascii_unfinished_dialog_spells_the_lower_bound_out`
- Refusals: `app_delete::the_scan_root_cannot_be_deleted`, `app_delete::a_second_delete_waits_for_the_first`, `app_delete::a_confirmation_whose_target_becomes_refused_says_why`, `app_delete::d_on_the_parent_row_still_gets_the_hint`, `app_controls::d_on_an_other_filesystem_row_keeps_its_own_refusal`
- Results and errors: `app_delete::partial_result_groups_failures_and_v_lists_them`, `app_delete::the_next_delete_replaces_the_failures`, `app_delete::v_without_failures_says_so`, `app_delete::a_changed_folder_gives_the_not_deleted_dialog`, `app_delete::a_running_delete_shows_in_the_status_line`, `app_delete::an_executor_panic_adds_a_line_and_does_not_stop_the_scan`, `app_delete::a_partial_delete_while_stopping_queues_one_rescan_until_done`
- Model: `delete_model::declining_changes_neither_the_model_nor_the_files`, `delete_model::confirming_a_delete_leaves_totals_equal_to_du`, `delete_model::an_unfinished_folder_can_be_deleted_while_the_scan_runs`, `delete_model::delete_check_numbers_follow_the_scan`, `delete_model::partial_clears_the_tombstone_and_asks_for_a_rescan`, `delete_model::a_partial_delete_asks_for_a_rescan_that_matches_du`, `delete_model::refusals_cover_the_root_other_filesystems_and_running_deletes`
- Executor (real files): `delete_executor::removes_a_directory_tree_with_exact_counts`, `delete_executor::removes_a_file`, `delete_executor::unremovable_entries_are_reported_and_the_rest_is_removed`, `delete_executor::symlinks_inside_the_subtree_are_removed_not_followed`, `delete_executor::hard_link_outside_the_subtree_keeps_its_data`, `delete_executor::target_replaced_by_a_different_object_is_changed_since_scan`, `delete_executor::target_swapped_for_a_symlink_is_not_touched`, `delete_executor::parent_component_swapped_for_a_symlink_aborts`, `delete_executor::rel_paths_that_could_escape_or_name_the_root_are_refused`, `delete_executor::very_deep_tree_is_removed_with_bounded_open_fds`
- Mount tests (opt-in, need `unshare -rm`): `app_delete::a_mount_point_target_gives_the_not_deleted_dialog`, `delete_executor::mount_inside_target_is_not_descended`, `delete_executor::mount_same_device_bind_inside_target_is_not_descended`, `delete_executor::mount_point_as_the_target_is_refused`

Run: `cargo test --test app_delete --test delete_model --test delete_executor`
Opt-in mount variants: `cargo test --test delete_executor --test app_delete -- --ignored` (prints `SKIP: unshare unavailable` and passes where user namespaces are off).

**MC-7 (manual; deletes only inside `/tmp/sa-accept`)**
1. `cargo run --release -- /tmp/sa-accept`, select `docs/`, press `d`. The dialog shows the full path under `/tmp/sa-accept`, `Folder · … KiB (disk usage) · 41 files…` and `This cannot be undone.` Press `n`: nothing changes (`ls /tmp/sa-accept/docs` in another terminal).
2. `d` again, then press `x` and `Tab`: ignored, the dialog stays. Press `Esc`: it closes; nothing is deleted.
3. `d`, then `y`: `docs/` disappears from the tree and the status shows `Deleted /tmp/sa-accept/docs · freed … (41 files)`. `ls /tmp/sa-accept` agrees.
4. Select `cache/` (it holds a hard link to `media/clip.mp4`): the dialog has a "Space freed may be less…" line. Select `locked/` and press `d`, `y`: a `Delete finished with errors` dialog lists `permission denied`; `v` lists the path; `Esc`, then `Enter`; the status says `Press v to see them`.
5. Select the root row and press `d`: `Can't delete the scan root.`

## M8
**Requirement:** totals equal `du` (disk usage; hard links once; symlinks not followed).

**Automated** (each runs on the default temp dir and, when asked, on btrfs):
`scan_du_parity::totals_equal_du_in_disk_apparent_and_one_filesystem_modes`, `scan_du_parity::hard_links_are_counted_once_with_one_duplicate_per_extra_link`, `scan_du_parity::symlinks_are_leaves_and_never_followed`, `scan_du_parity::directories_have_no_apparent_size_like_du`, `scan_du_parity::awkward_names_and_deep_nesting_survive`, `scan_du_parity::a_folder_of_ten_thousand_files_arrives_in_parts_and_still_equals_du`, `model_apply::hard_link_is_charged_once`, `model_apply::random_streams_converge_to_the_sum_of_all_entries`, `delete_model::hard_links_across_the_deleted_subtree_still_match_du`

Run: `cargo test --test scan_du_parity`
**btrfs variant:** `mkdir -p $HOME/tmp/sa-tests && SA_TEST_BTRFS_DIR=$HOME/tmp/sa-tests cargo test --test scan_du_parity -- --nocapture` (without the variable the btrfs case prints `SKIP: SA_TEST_BTRFS_DIR is not set, btrfs case not run`). `$HOME` must be on btrfs for this to mean anything.

**MC-8 (manual)**
1. `du -sB1 --apparent-size /tmp/sa-accept` and `du -s /tmp/sa-accept` in a shell (the link-to-usr symlink is not followed by `du` either).
2. In the program, wait for `Done` and compare the root row's size (press `a` to toggle apparent size and disk usage). The disk-usage total equals `du -s` (allow the `locked/` directory: unreadable folders are reported under `4 Skipped`, and `du` reports an error for them too).
3. The hard link: `media/clip.mp4` plus `cache/clip-link.mp4` count once.

## M9
**Requirement:** nothing is modified except through M7.

**Automated:** `write_guard::repo_sources_pass` (no code outside `src/delete/**` and `src/history/**` can write), `write_guard::planted_violation_outside_allowed_modules_fails` and the other `write_guard::` cases (the guard itself is tested), `scan_du_parity::scanning_changes_no_access_or_modification_time`, `delete_model::opening_the_check_changes_nothing`, `delete_model::declining_changes_neither_the_model_nor_the_files`, `app_eta::without_history_nothing_is_read_or_written`

Run: `cargo test --test write_guard --test scan_du_parity`.

**MC-9 (manual):** `stat -c '%X %Y' /tmp/sa-accept/media/movie.mkv` before and after a full scan with deletion declined (`d`, `n`): identical. `find /tmp/sa-accept -newer /tmp/sa-accept/media/movie.mkv` lists nothing new. No `~/.local/state` file appears without `--history`.

## M10
**Requirement:** permission errors and vanishing files do not crash; skipped paths are reported.

**Automated:** `scan_du_parity::an_unreadable_directory_is_one_issue_and_the_rest_completes`, `scan_robustness::files_that_vanish_between_listing_and_stat_become_vanished_issues`, `scan_robustness::a_scan_beside_a_thread_that_deletes_and_recreates_files_still_finishes`, `scan_walker::errors_are_reported_and_the_scan_still_completes`, `scan_walker::real_fs_refuses_symlinks_and_reports_permissions`, `scan_walker::real_scan_of_a_temp_tree_lists_everything_and_flags_unreadable_dirs`, `app_cli::an_unreadable_directory_is_permission_denied`, `ui_lists::skipped_lists_every_reason_newest_first`, `app_loop::a_scan_worker_panic_shows_an_error_dialog_and_stops_the_scan`, `app_loop::a_panic_on_a_non_scan_thread_does_not_stop_the_scan`

Run: `cargo test --test scan_robustness --test scan_du_parity --test scan_walker`.

**MC-10 (manual):** scan `/tmp/sa-accept`: `locked/` shows `(permission denied)` with an issue marker; the header shows an issue count; view `4` lists `/tmp/sa-accept/locked (permission denied)`. In another shell, `rm /tmp/sa-accept/docs/note1.txt` while scanning a larger tree (for example scan `/usr` while that happens): no crash.

## M11
**Requirement:** prioritise (zoom or `p`), skip/resume (`x`/`r`), shallow-first.

**Automated:** `app_controls::the_priority_folder_is_scanned_before_the_others_and_announces_when_done`, `app_controls::zoom_sets_the_focus_going_back_to_the_root_clears_it_and_p_toggles`, `app_controls::x_skips_a_folder_and_r_brings_back_the_full_totals`, `app_controls::r_on_an_unfinished_folder_moves_it_to_the_front_instead_of_restarting`, `app_controls::r_on_an_unfinished_folder_also_resumes_the_skipped_folders_inside`, `app_controls::r_on_the_root_while_scanning_changes_nothing`, `app_controls::r_after_a_stop_is_a_full_rescan_at_once`, `app_controls::while_stopping_r_p_and_x_only_show_a_message_and_are_not_replayed`, `app_controls::queued_rescans_start_together_and_the_scan_stays_in_flight_until_both_finish`, `app_controls::r_p_and_x_on_an_other_filesystem_row_say_so_and_change_nothing`, `scan_walker::focus_lists_the_focused_subtree_before_unfocused_siblings`, `scan_walker::skipped_subtree_is_cancelled_and_never_listed`, `scan_walker::single_thread_lists_shallow_directories_first`, `scan_walker::rescan_many_gives_one_done_after_all_the_rescans`, `model_controls::skipping_and_then_resuming_gives_the_unskipped_totals`, `model_controls::rescan_plan_for_an_unfinished_folder_lists_nothing_twice_and_ends_exact`, `lib::scan::queue::tests::focus_beats_depth_and_clearing_restores_order`, `lib::scan::queue::tests::shallow_first_and_fifo_within_a_depth`

Run: `cargo test --test app_controls --test model_controls --test scan_walker`; `cargo test --lib -- scan::queue`.

**MC-11 (manual)**
1. `cargo run --release -- /usr`. While it scans, move to a folder that is not finished and press `p`: the status shows `▲ Scanning <name>/ first.` and its rows start to fill before the others. `p` again clears it.
2. Press `x` on another unfinished folder: it shows `⊘`, `(scan skipped)` and a `≥` size; the ancestors show `≥`. Press `r` on it: `◌ Resuming…`, and when done the marks clear.
3. Press `⏎` on a folder not yet scanned: it is zoomed and prioritised.
4. The first level is listed before deeper levels are done (top-level sizes appear first).

## M12
**Requirement:** quit always asks; during a delete only `y` quits; the stderr line says how far the delete got.

**Automated:** `app_loop::quit_always_asks_first`, `app_loop::second_q_ctrl_c_or_y_quits`, `app_loop::n_esc_enter_stay_and_other_keys_are_ignored`, `app_loop::quit_with_a_full_scan_channel_via_q_y_returns_promptly`, `app_loop::quit_with_a_full_scan_channel_via_sigterm_returns_promptly`, `app_loop::quitting_during_a_real_scan_with_a_full_channel_returns_promptly`, `app_quit::quit_during_a_delete_asks_and_only_y_quits`, `app_quit::unknown_total_leaves_out_of_m`, `app_quit::quit_without_a_delete_prints_nothing`, `app_quit::sigterm_during_a_delete_also_reports`, `app_quit::quit_with_the_event_channel_full_exits_within_two_seconds`, `app_quit::an_executor_that_never_finishes_cannot_hold_up_quitting`, `app_quit::a_delete_that_finishes_while_quitting_prints_no_interrupted_line`, `app_quit::a_delete_that_ends_with_errors_while_quitting_says_so`, `app_quit::a_stopped_delete_reports_how_far_it_got`, `app_quit::exit_messages_never_carry_escape_bytes_from_the_path`, `app_panic::only_a_main_thread_panic_restores_the_terminal`, `app_loop::a_render_panic_unwinds_through_the_terminal_guard`

Run: `cargo test --test app_loop --test app_quit --test app_panic`.

**MC-12 (manual)**
1. `q`: a dialog asks; `n` stays; `Ctrl-C`: asks again; `y` quits and the terminal is restored (no leftover alternate screen).
2. To see the delete variant, create a large scratch tree: `mkdir /tmp/sa-accept/many && cd /tmp/sa-accept/many && for i in $(seq 1 30000); do : > f$i; done`, scan `/tmp/sa-accept`, select `many/`, `d`, `y`, then immediately `q`: the dialog `Quit while deleting?` appears (if the delete is still running); `n` returns; `q` then `y` quits and the shell prints `<cmd>: delete of /tmp/sa-accept/many interrupted: N of 30,000 files removed`. (If the delete finished too fast, make the tree larger.)

---

## Should
**One filesystem by default, `--cross-filesystems`:** `scan_walker::other_filesystems_are_flagged_and_not_descended_unless_allowed`, `scan_du_parity::crossing_filesystems_allowed_changes_nothing_on_a_single_filesystem`, `model_apply::other_filesystem_dir_is_not_counted_or_pending`, `model_controls::another_filesystem_is_never_rescanned`, `app_controls::one_other_filesystem_is_named_in_the_status_line`, `app_controls::several_name_the_shallowest_and_count_the_rest`, `app_controls::the_hint_shows_in_every_state_and_never_with_cross_filesystems`, `app_cli::help_names_the_binary_and_every_option`, `lib::app::cli::tests::one_filesystem_and_cross_filesystems_conflict`
**Opt-in history:** `history::save_then_load_roundtrips_and_creates_the_directory`, `history::saving_is_atomic_and_leaves_no_temp_file`, `history::a_corrupt_file_is_reported_and_left_alone_until_the_next_save`, `history::the_file_keeps_at_most_fifty_roots_dropping_the_oldest`, `app_eta::with_history_a_complete_scan_is_written_atomically_and_the_next_run_uses_it`, `app_eta::without_history_nothing_is_read_or_written`, `app_eta::a_cancelled_scan_or_one_with_user_skips_writes_nothing`, `app_eta::a_corrupt_history_file_is_ignored_with_a_message_and_not_overwritten_until_a_save`
**Disk usage / apparent toggle:** `ui_accessibility::a_toggles_between_disk_usage_and_apparent_size_everywhere`, `ui_accessibility::the_size_mode_changes_what_the_views_show`, `ui_treemap::apparent_size_mode_changes_the_tile_sizes`
**Stop a scan cleanly:** `app_controls::s_stops_through_a_frozen_stopping_line_to_stopped_and_r_on_the_root_rescans`, `scan_walker::cancel_all_ends_with_done_cancelled_and_stops_listing`, `scan_walker::cancel_all_while_workers_block_on_a_full_channel_still_ends_the_scan`, `scan_walker::a_rescan_right_after_a_stop_is_never_lost`

Run: `cargo test --test app_controls --test scan_walker --test history --test app_eta --test app_cli --test ui_accessibility`.

- **MC-13 (one filesystem):** `df /tmp /home` to find two filesystems, then scan a directory that has another filesystem mounted below it (for example `/`). The mount appears with `(other filesystem)`, is not descended, and the status line ends `not scanned (other fs): …`. `r`, `p`, `x` on it say `<name>/ is on another filesystem. To scan it, use --cross-filesystems.` Restart with `--cross-filesystems`: it is scanned and the hint is gone.
- **MC-14 (history):** `ls ~/.local/state/*` before; run `cargo run --release -- --history /tmp/sa-accept`, wait for `Done`, quit; one small file appears; a second run shows an estimate with source `last scan`. Without `--history` nothing is written.
- **MC-15 (size toggle):** press `a`: the header says `Apparent size` / `Disk usage` and all three views change.
- **MC-16 (stop):** scan `/usr`, press `s`: `■ Stopping…` with a frozen clock, then `■ Stopped at … · partial results`; `r` on the root restarts.

---

## UX §11 checks

| # | Check | Tests |
|---|---|---|
| 1 | 80×24 snapshots, Unicode and ASCII | `ui_snapshots::every_screen_in_unicode_and_ascii`, `ui_lists::snapshots_of_each_view_in_each_state`, `ui_treemap::map_snapshots_for_each_state`, `app_delete::dialogs::every_delete_dialog_in_unicode_and_ascii`, `ui_accessibility::the_ascii_fallback_draws_every_view_and_dialog_in_plain_ascii`, `ui_accessibility::exactly_80_by_24_is_not_too_small`, `ui_accessibility::below_80_by_24_only_the_message_is_shown_and_most_keys_do_nothing` |
| 2 | NO_COLOR | `ui_accessibility::without_colour_no_cell_has_a_foreground_or_background_and_the_text_is_unchanged`, `ui_accessibility::every_state_is_still_told_apart_by_glyph_or_words_without_colour`, `ui_accessibility::colour_depth_follows_no_color_then_colorterm_then_term` |
| 3 | Contrast ≥ 4.5 | `ui_accessibility::every_palette_pair_meets_the_contrast_floor_in_both_colour_modes`, `ui_treemap::the_palette_meets_the_contrast_floor_in_truecolor_and_256` |
| 4 | ETA rules | see M2 (`app_eta::…`, `lib::eta::…`) |
| 5 | Elapsed changes each second | `app_loop::elapsed_text_changes_in_every_second_of_a_ten_second_run` |
| 6 | Delete dialog | `app_delete::exact_dialog_shows_the_numbers_and_only_y_deletes`, `app_delete::unfinished_target_shows_lower_bounds_and_p_asks_for_the_scan_first` |
| 7 | Refusals, MountPoint | `app_delete::the_scan_root_cannot_be_deleted`, `app_delete::a_second_delete_waits_for_the_first`, `app_controls::d_on_an_other_filesystem_row_keeps_its_own_refusal`, `app_delete::a_mount_point_target_gives_the_not_deleted_dialog` (`--ignored`) |
| 8 | Quit | see M12 |
| 9 | Zoom, `p`, `r`, stop keys | see M11 |
| 10 | `x` marks | `app_controls::x_skips_a_folder_and_r_brings_back_the_full_totals`, `ui_tree::markers_follow_the_state_table_and_precedence` |
| 11 | Label truncation, no raw control bytes | `ui_tree::hostile_names_stay_inside_the_row_and_reach_the_buffer_escaped`, `ui_tree::a_name_with_a_bidi_override_shows_escaped_within_its_width`, `lib::ui::sanitize::tests::controls_become_hex_escapes`, `lib::ui::sanitize::tests::bidi_and_format_controls_become_unicode_escapes`, `app_delete::dialogs::a_bidi_override_in_the_path_is_escaped_in_the_confirmation`, `app_cli::a_path_with_escape_bytes_is_printed_escaped` |
| 12 | `? help` always in hints | `ui_accessibility::key_hints_always_keep_help_at_80_columns_in_every_context`, `lib::ui::hints::tests::hints_drop_from_the_right_and_keep_help` |
| 13 | Hidden-items notice | `ui_treemap::tiny_items_without_an_aggregate_tile_show_the_hidden_notice`, `ui_treemap::a_drawn_aggregate_tile_replaces_the_notice`, `ui_treemap::a_hidden_selected_item_leads_the_notice_and_the_fit_order_applies`, `ui_treemap::the_notice_shortens_on_a_narrow_title_rule` |
| 14 | CLI validation | `app_cli::invalid_option_values_are_usage_errors`, `app_cli::boundary_values_pass_validation`, `app_cli::only_the_first_invalid_flag_is_reported`, `lib::app::cli::tests::invalid_values_name_the_flag_and_value`, `lib::app::cli::tests::usage_error_has_a_second_help_line` |
| 15 | Number formats | `lib::ui::format::tests::compact_size_follows_spec_7_1`, `lib::ui::format::tests::compact_size_is_at_most_six_cells_for_any_u64`, `lib::ui::format::tests::long_size`, `lib::ui::format::tests::compact_count`, `lib::ui::format::tests::grouping` |
| 16 | Selected state | `ui_tree::tree_focus_selection_is_reverse_bold_across_the_pane_and_never_dim`, `ui_tree::map_focus_selection_is_bold_with_the_name_underlined_and_not_reversed`, `ui_treemap::the_selected_tile_frame_is_reverse_bold_with_a_marked_title_and_keeps_its_fill` |
| 17 | Tile colours | `lib::ui::tile_colors::tests::file_tiles_take_their_type_hue_and_the_aggregate_is_grey`, `lib::ui::tile_colors::tests::neighbours_never_share_a_folder_hue_while_free_hues_remain`, `lib::ui::tile_colors::tests::the_same_input_and_memory_give_the_same_output`, `ui_treemap::folders_never_use_the_hue_of_a_file_type_in_the_same_view`, `ui_treemap::touching_tiles_never_share_a_fill_while_free_hues_remain`, `ui_treemap::colours_stay_put_while_sizes_change_and_move_once_for_a_new_file_type`, `ui_treemap::zooming_in_and_out_keeps_every_folders_colour`, `ui_treemap::with_all_seven_types_present_folders_are_unfilled`, `ui_treemap::tile_frames_use_the_default_background_and_only_the_interior_is_filled`, `ui_treemap::without_colours_nothing_is_filled_but_labels_still_tell_files_from_folders` |
| 18 | `..` row | `ui_tree::the_parent_row_appears_only_when_zoomed_and_sits_above_the_view_root`, `ui_tree::enter_on_the_parent_row_equals_backspace_and_selects_the_folder_you_left`, `ui_tree::d_p_x_r_on_the_parent_row_show_the_message_and_change_nothing`, `ui_tree::the_parent_row_is_never_inside_the_aggregate_or_counted_in_the_page`, `ui_treemap::with_the_parent_row_selected_the_map_shows_the_same_tiles_but_highlights_none` |
| 19 | Hints and help | `ui_accessibility::hint_lines_equal_the_ux_strings_at_80_columns_in_unicode_and_ascii`, `ui_accessibility::ascii_mode_never_uses_single_letters_as_key_names`, `ui_accessibility::the_help_overlay_fits_80_by_24_in_two_columns_in_unicode_and_ascii`, `ui_accessibility::help_keys_esc_and_question_mark_close_q_opens_the_quit_prompt_and_others_are_ignored`, `lib::ui::help::tests::nineteen_entries_fit_the_twenty_row_overlay` |
| 20 | Delete failures | `app_delete::partial_result_groups_failures_and_v_lists_them`, `app_delete::the_next_delete_replaces_the_failures`, `app_delete::v_without_failures_says_so`, `app_delete::help_lists_the_failure_key` |
| 21 | Other-filesystem rows | `app_controls::r_p_and_x_on_an_other_filesystem_row_say_so_and_change_nothing`, `app_controls::a_long_other_filesystem_name_is_cut_in_the_middle_and_the_advice_stays_whole` |
| 22 | Largest `updating…` | **gap, see below** |

---

## Known limits and measured numbers
Measured on this machine (`$HOME`, about 19.9 M entries, warm cache) and reported by the spike tasks; the targets are **not** gates (README), except the first frame ≤ 1 s.

| Item | Measured | Target | Source |
|---|---|---|---|
| R1 memory per entry (20 M entries) | 152.8 B/entry, 2.85 GiB RSS | 120 B | `docs/tasks/008-report.md`, `spikes/r1-memory/` |
| Model apply rate | 2.7 to 4.1 M entries/s | n/a | T008, T009 |
| R2 apply/walker ratio | 82 % at 4 threads, 48 % at 8, 39 % at 16; wall time flat from 4 to 16 threads | ≥ 70 % | `spikes/R2-R4-R8-REPORT.md` |
| Longest apply tick (T030) | 9.2 ms | 50 ms | `docs/tasks/030-report.md` |
| Tree row build | 5.3 ms at 100 k children, 83.8 ms at 1 M | 50 ms | R2 report |
| Input latency p95 (test, `TestBackend`) | 0.1 to 8.7 ms | 50 ms | `app_loop::input_latency_during_a_flood_is_measured` |
| First frame (test, simulated terminal) | 0.5 ms | 200 ms (gate: 1 s) | `app_loop::m1_frame_zero_is_drawn_before_the_scanner_starts_and_within_one_second` |
| Delete check | 18.2 ms per 1 M nodes | 50 ms | `delete_model::delete_check_timing` |
| Squarify, 5,000 items | 0.08 ms | 1 ms | `lib::layout::squarify::tests::five_thousand_items_timing` |
| R5 treemap jitter | keep the 500 ms relayout throttle; rank swaps p95 = 1 among the top 8 | n/a | `spikes/R5-REPORT.md` |

Known limits
- **ETA unknown on a first scan with no source** (btrfs, history off): by design (ADR-010); the M2 amendment is with the project owner.
- **R1 memory is 152.8 B/entry**, above the 120 B target; a full `$HOME` scan of about 20 M entries needs about 2.9 GiB of RAM.
- **Hard links** are counted once, in the folder scanned first: totals equal `du`, individual folder sizes may differ from `du`'s.
- **Delete is permanent** and never follows symlinks; the scan root, other-filesystem folders and mount points cannot be deleted.
- **Terminal size:** below 80×24 only a "terminal too small" message is shown.
- **The test terminal is simulated** (`TestBackend`); real-terminal rendering, resize and colour depth are covered only by the manual checks above and by the lead's pty runs.
- Delete failures are listed in path order (byte-order sorting and size sorting were considered and left out).

## Gaps (requirements with no automated test)
- **UX check 22 (Largest `updating…`):** not built yet; it comes with T030b (event-loop wiring of the resumable top-N rebuild). There is no test and no feature to check manually until then.
- **NO_COLOR rendering of each dialog:** the dialog snapshots are text only; colour-free rendering is covered for the main screens (`ui_accessibility::without_colour_…`) but not by a separate snapshot per dialog (accepted in the T021 review).
- **Real-terminal behaviour** (alternate screen entry and exit, resize, terminal colour depth, a terminal that sends a bell) has no automated test: only the manual checks.
- **Check 11's "ESC in a name" in a real filesystem:** the tests use synthetic names and a real temp tree for the CLI path; no test creates a file whose name contains ESC and scans it end to end. (Names with unusual bytes are covered by `scan_du_parity::awkward_names_and_deep_nesting_survive`.)

---

## Full run
```
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo test --doc
cargo build --release
```
Expected at the time of writing (2026-10-23, after T029): `cargo test --all-targets` reports **524 passed, 0 failed, 4 ignored** summed over all targets (unit tests, integration tests, and the `scan_harness` and `spike_r5_jitter` example tests); `cargo test --doc` reports 0 tests. Counts grow as tasks land.

Opt-in variants
- btrfs parity: `mkdir -p $HOME/tmp/sa-tests && SA_TEST_BTRFS_DIR=$HOME/tmp/sa-tests cargo test --test scan_du_parity -- --nocapture`
- Mount-namespace tests (the 4 ignored ones, need `unshare -rm`): `cargo test --test delete_executor --test app_delete -- --ignored` (expected: 4 passed, or `SKIP: unshare unavailable`)
- Measured targets: `cargo test --release --all-targets -- --nocapture 2>&1 | grep TARGET:` The headless harness (`examples/scan_harness.rs`, T009/T030) prints more `TARGET:` lines; its usage is in the file header.
