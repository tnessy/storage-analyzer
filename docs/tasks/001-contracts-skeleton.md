# T001: Crate skeleton, shared contracts, write guard, APP_ID

- **Engineer:** engineer-1. **Wave:** 0. **Depends on:** none. **Blocks:** every other code task.
- **Parallel with:** T002, which is a standalone crate, so there are no shared files.

## Goal
Create the crate and **every shared contract** from design §4 and §8 "Define first", so that both engineers can build against them. Most behavior is out of scope: function bodies that belong to later tasks are `todo!("T0NN")` stubs with the final signatures.

Acceptance criteria:
1. `cargo build` succeeds, and so do all five commands in `docs/tasks/README.md`.
2. All types below exist with the fields and meaning given. Spelling may vary slightly, but the field sets may not.
3. `tests/write_guard.rs` fails if any file outside `src/delete/**` or `src/history/**` contains `remove_file`, `remove_dir`, `remove_dir_all`, `unlinkat`, `File::create`, `OpenOptions` together with `.write(`/`.append(`/`.create(`, `rename`, `renameat`, `fs::write`, or `set_permissions`. Matches in comments and in `tests/` (fixtures may create files) are excluded. Prove it works with a test that runs the scanner over a temp source dir containing a planted violation.
4. `tests/no_name_literal.rs` fails if the `APP_ID` value appears as a string literal anywhere under `src/` except its single definition.
5. M9 groundwork: a unit test shows that `ScanControl` starts with nothing cancelled, no focus and generation 0.

## Design references
§3.2 modules, §3.3 concurrency (id allocation, completion), §3.4 node fields and flags, §3.10 contracts, §3.11 `ScanControl`, §4 Conventions (the code block is the contract), §8 "Define first". ADR-001, -003, -004, -009. Plan §5 D7, D8.

## Files owned (exclusive)
- `Cargo.toml`, `Cargo.lock`, `rustfmt.toml`, `clippy.toml` (if needed), `.gitignore`
- `src/lib.rs` (module declarations and `pub const APP_ID`), `src/main.rs` (minimal: `fn main() { }` calling nothing yet; T015 takes it over)
- `src/types.rs`
- `src/scan/mod.rs`, `src/scan/msg.rs`, `src/scan/control.rs`, `src/scan/fs.rs`
- `src/model/mod.rs` (struct + method signatures, stub bodies)
- `src/delete/mod.rs`, `src/delete/msg.rs`
- `src/eta/mod.rs`, `src/eta/contract.rs`
- `src/layout/mod.rs`, `src/layout/contract.rs`
- `src/probe/mod.rs`, `src/history/mod.rs`, `src/app/mod.rs`, `src/ui/mod.rs` (empty module files with a one-line doc comment)
- `tests/write_guard.rs`, `tests/no_name_literal.rs`

## Dependencies to add (all of them now; nobody edits `Cargo.toml` after this)
`rustix` 1.x (features `fs`, `process`, `thread`), `ratatui` 0.30, `crossterm` 0.29, `clap` 4 (derive), `unicode-width`, `smallvec`, `serde` + `serde_json` (history only), `signal-hook` (SIGTERM/SIGHUP). Dev-dependencies: `proptest`, `tempfile`, `insta` (snapshots), `assert_cmd` (binary tests). Pin exact versions in `Cargo.lock`. Package/binary name: `storage-analyzer` (placeholder). `APP_ID = "storage-analyzer"` (placeholder, the only literal).

## Contracts to define
In `src/types.rs`:
- `NodeId(u32)` with `NodeId::ROOT = NodeId(0)`, `Copy, Eq, Ord, Hash, Debug`.
- `Kind { File, Dir, Symlink, Other }`, `Op { Open, Read, Stat }`, `SizeMode { Disk, Apparent }`.
- `IssueKind { PermissionDenied, Vanished, OtherFilesystem, SkippedByUser, Other(i32) }`.
- `ScanIssue { node: NodeId, op: Op, kind: IssueKind }`.
- `Counts { files: u64, dirs: u64, bytes: u64 }`.
- `ScanState { Starting, Scanning, Done, Stopped }` (D7).

In `src/scan/msg.rs`: `RawEntry` and `ScanMsg` exactly as design §4. `Listing.first_id .. first_id + entries.len()` are the entries' ids.

In `src/scan/control.rs`: `ScanControl` from design §3.11, with methods (implement these; they are small and shared):
- `cancel_prefix(&self, p: PathBuf)`, `remove_prefixes_under(&self, p: &Path)` (D2), `is_cancelled(&self, p: &Path) -> bool`
- `set_focus(&self, Option<PathBuf>)`, `focus(&self) -> Option<PathBuf>`
- `cancel_all(&self)`, `reset_cancel_all(&self)`, `generation(&self) -> u64`. Every mutation bumps `generation`.
- Accepted in review: `new()` and `is_cancel_all(&self) -> bool`. `is_cancelled` checks prefixes only; `cancel_all` is checked separately.
- Unit tests: prefix matching works on whole path components (`/a/b` does not cancel `/a/bc`), and every mutation bumps the generation.

In `src/scan/fs.rs`, the fault-injection seam (design §3.5):
```rust
pub trait FsAccess: Send + Sync {
    type Dir: Send;
    fn open_dir(&self, path: &Path) -> Result<Self::Dir, Errno>;          // O_RDONLY|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC (+O_NOATIME)
    fn read_entries(&self, dir: &mut Self::Dir) -> Result<Vec<Box<[u8]>>, Errno>; // names, no . or ..
    fn stat_at(&self, dir: &Self::Dir, name: &[u8]) -> Result<StatInfo, Errno>;  // AT_SYMLINK_NOFOLLOW
}
pub struct StatInfo { kind: Kind, disk_bytes: u64, apparent_bytes: u64, ino: u64, dev: u64, nlink: u32 }
```
`Errno` is `rustix::io::Errno`. The real implementation is T006's job; leave a stub `RealFs` type only.

In `src/scan/mod.rs` (signatures, stub bodies → T006):
```rust
pub struct ScanOptions { pub threads: usize, pub one_filesystem: bool }
impl ScanOptions { pub fn default_threads() -> usize /* min(available_parallelism, 8) — a target, tunable */ }
pub struct ScanHandle { /* join handles, job sender */ }
pub fn start<F: FsAccess + 'static>(fs: Arc<F>, root: PathBuf, root_dev: u64, opts: ScanOptions,
        control: Arc<ScanControl>, tx: SyncSender<ScanMsg>, first_id: u32) -> ScanHandle;
impl ScanHandle {
    pub fn rescan(&self, dir: NodeId, path: PathBuf, depth: u32); // works any time, also after Done / Stopped
    pub fn shutdown(self);                                         // joins workers
}
```

In `src/delete/msg.rs`: `DeleteRequest`, `DeleteEvent`, `Outcome`, `DeleteFailure`, `DeleteFailKind` (design §4, D8), `AbortReason`.
In `src/delete/mod.rs` (stub → T010):
```rust
pub struct DeleteHandle { /* request sender, stop flag */ }
pub fn spawn(root_fd: OwnedFd, tx: SyncSender<DeleteEvent>) -> DeleteHandle;
impl DeleteHandle { pub fn submit(&self, req: DeleteRequest) -> Result<(), Busy>;
                    pub fn stop(&self); pub fn wait_idle(&self, max: Duration) -> bool; }
```

In `src/eta/contract.rs`: `EtaInput`, `EtaSource { History, FsInodes, Probe }`, `EtaView` (design §4). In `src/eta/mod.rs`: `pub fn view(input: &EtaInput) -> EtaView` stub (→ T013).

In `src/layout/contract.rs`: `Tile { id: Option<NodeId>, rect: ratatui::layout::Rect }`, `Direction { Up, Down, Left, Right }`. In `src/layout/mod.rs`: `squarify(items: &[(NodeId, u64)], area: Rect, cell_aspect: f32) -> Vec<Tile>` and `neighbor(tiles: &[Tile], from: usize, dir: Direction) -> Option<usize>` stubs (→ T014).

In `src/model/mod.rs` (struct shell + signatures; bodies → T003, T005, T011, T012):
```rust
pub struct ModelOptions { pub top_n: usize }
pub struct ScanTotals { pub entries_seen: u64, pub files: u64, pub dirs: u64, pub pending_dirs: u64, pub disk_bytes: u64, pub apparent_bytes: u64 }
pub enum Refusal { ScanRoot, OtherFilesystem, AlreadyDeleting, DeleteRunning(NodeId) }   // aggregates are a UI concept
pub enum Reconcile { None, Rescan(NodeId) }                                              // D1
pub struct DeleteCheck { /* design §4 */ }
pub enum ExtKey { Ext(Box<[u8]>), NoExt, Other }
pub enum Category { Video, Image, Archive, CodeText, Binary, Documents, AudioOther }
pub struct ExtTotals { pub disk_bytes: u64, pub apparent_bytes: u64, pub count: u64 }
impl Model {
  pub fn new(root_path: PathBuf, root: RawEntry, opts: ModelOptions) -> Model;          // T003
  pub fn root(&self) -> NodeId; pub fn exists(&self, id: NodeId) -> bool;              // T003
  pub fn name(&self, id: NodeId) -> &[u8]; pub fn parent(&self, id: NodeId) -> Option<NodeId>;
  pub fn kind(&self, id: NodeId) -> Kind; pub fn depth(&self, id: NodeId) -> u32;
  pub fn size(&self, id: NodeId, mode: SizeMode) -> u64; pub fn counts(&self, id: NodeId) -> (u64 /*files*/, u64 /*dirs*/);
  pub fn nlink(&self, id: NodeId) -> u32; pub fn identity(&self, id: NodeId) -> (u64 /*dev*/, u64 /*ino*/);
  pub fn children(&self, id: NodeId) -> &[NodeId];
  pub fn children_sorted(&self, id: NodeId, mode: SizeMode) -> Vec<NodeId>;            // (size desc, name asc)
  pub fn is_complete(&self, id: NodeId) -> bool; pub fn is_exact(&self, id: NodeId) -> bool;
  pub fn flags(&self, id: NodeId) -> Flags; pub fn issue(&self, id: NodeId) -> Option<&ScanIssue>;
  pub fn issues(&self) -> &[ScanIssue];
  pub fn path(&self, id: NodeId) -> PathBuf; pub fn rel_path(&self, id: NodeId) -> PathBuf;
  pub fn totals(&self) -> ScanTotals; pub fn state(&self) -> ScanState;
  pub fn apply(&mut self, msg: ScanMsg);                                                // T003
  pub fn ext_totals(&self) -> Vec<(ExtKey, ExtTotals)>; pub fn category_of(ext: &ExtKey) -> Category; // T005
  pub fn category_of_node(&self, id: NodeId) -> Category;                               // T005: files by ext, dirs by largest category inside
  pub fn top_files(&self, mode: SizeMode) -> Vec<NodeId>;                               // T005: sorted desc
  pub fn set_focus(&mut self, id: Option<NodeId>); pub fn take_focus_completed(&mut self) -> Option<NodeId>; // T012
  pub fn mark_skipped(&mut self, id: NodeId) -> bool;                                  // T012: false if already complete
  pub fn begin_rescan(&mut self, id: NodeId) -> (PathBuf, u32 /*depth*/);              // T012
  pub fn delete_refusal(&self, id: NodeId) -> Option<Refusal>;                          // T011
  pub fn delete_check(&self, id: NodeId, mode: SizeMode) -> DeleteCheck;                                // T011
  pub fn begin_delete(&mut self, id: NodeId) -> Result<DeleteRequest, Refusal>;         // T011
  pub fn apply_delete(&mut self, ev: &DeleteEvent) -> Reconcile;                        // T011
}
```
`Flags` is a bitset with `PENDING, DONE, INCOMPLETE, ERROR, OTHER_FS, HARDLINK_DUP, DELETING, TOMBSTONE, REMOVED`. `REMOVED` marks a hole left by a delete: NodeIds are never reused, and messages that target a removed node are ignored.

## Tests to write first
- `tests/write_guard.rs`: (a) the repo's `src/` passes; (b) a temp source tree with `std::fs::remove_file` in `src/scan/x.rs` fails; (c) the same call in `src/delete/x.rs` passes; (d) a forbidden word inside a `//` comment passes.
- `tests/no_name_literal.rs`: (a) the repo passes; (b) a planted literal fails.
- `src/scan/control.rs` unit tests as above.

## Out of scope
Any real scanning, model, delete, ETA, layout or UI behavior. Picking the final command name.
