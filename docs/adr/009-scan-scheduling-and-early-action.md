# 009: Shallow-first scan, subtree prioritise/skip, delete of unfinished folders
- Status: accepted (user confirmed O2 triggers and O5 rejection, 2026-10-23)
- Context: Full-depth scans of large roots take long. The user recognizes folders by name and wants to act on them immediately (G3, M1, M7). A full walk is unavoidable without root: WizTree's speed comes from reading the NTFS MFT directly with admin rights, and btrfs's equivalent, BTRFS_IOC_TREE_SEARCH, requires CAP_SYS_ADMIN (kernel fs/btrfs/ioctl.c). So scheduling and partial-result handling are the levers.
- Options:
  - O1 shallow-first priority queue
  - O2 prioritise a subtree (`p` / zoom)
  - O3 delete unfinished with `≥` bounds, cancelling its scan
  - O4 skip a subtree (`x`)
  - O5 `--max-depth` scan limit
  - O6 per-subtree probe size estimates

  Details in design §3.11.
- Decision:
  - Adopt O1 + O2 + O3 + O4.
  - Reject O5: it undercounts, breaks M8, and the expand-on-demand tree already limits display.
  - Defer O6: misleading next to real sizes.
  - Supporting pieces: workers check a shared `ScanControl` (cancelled prefixes, focus, generation). The model separates `is_complete` (no work left) from `is_exact` (complete and nothing skipped). O3 keeps every ADR-007 safety rule; only the counts become lower bounds.
- Consequences:
  - **Benefits:** immediate action on recognized folders; exact numbers on demand via `p`; less wasted scan time via `x`.
  - **Costs:** a bigger pending frontier (capped at 1 M, then deepest-first); an O(pending) re-heap on focus change; one more per-dir counter.
  - **M7 interpretation:** clearly labeled lower bounds count as "size and file/dir count". Project-owner should update the brief.
  - **Risk:** the user deletes more than expected. Mitigated by the full path, the largest-known children, the warning title, and `y`-only confirmation.
- Blast radius / reversibility: The scan queue, the model counters, and the delete dialog. Reverting O3 to "blocked" is a one-line gate. O1 can be reverted to FIFO/LIFO internally.
- Revisit if: R8 shows frontier memory or re-key cost is too high, or the user finds `≥` deletes confusing.
