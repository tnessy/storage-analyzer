# storage-analyzer

A disk-usage TUI for Linux: a directory tree plus a treemap, updating live
while the scan runs. Think WinDirStat or WizTree, in a terminal.

Linux only. It reads mount tables and `/sys` block-device statistics.

## Install

From crates.io (needs Rust 1.99 or newer):

```sh
cargo install --locked storage-analyzer
```

An AUR package for Arch Linux and derivatives is planned for the future.

From source:

```sh
git clone https://github.com/tnessy/storage-analyzer
cd storage-analyzer
cargo install --locked --path .
```

## Usage

```sh
storage-analyzer [OPTIONS] [PATH]
```

`PATH` is the directory to analyze and defaults to the current directory.
Press `?` inside the app for the key bindings.

| Option | Effect |
|---|---|
| `-x`, `--one-file-system` | Stay on the root's filesystem. By default other local filesystems are entered too, but not proc, sys, dev or network mounts. |
| `--cross-filesystems` | Descend into every filesystem, including proc, sys and network mounts. |
| `--apparent` | Show apparent size instead of disk usage. |
| `--threads <N>` | Scan threads. Default: `min(cores, 8)`. |
| `--top <N>` | Number of entries in the Largest view. |
| `--history` | Remember scans between sessions (writes one small file under `~/.local/state`). |
| `--no-animation` | Freeze the spinner. |
| `--cell-aspect <F>` | Terminal cell height/width, 0.5 to 4.0. Default: 2.0. |

## Building

You need Linux and Rust 1.99 or newer (`rustup update stable`). There are no
system library dependencies.

```sh
git clone https://github.com/tnessy/storage-analyzer
cd storage-analyzer
cargo build --release
```

The binary is written to `target/release/storage-analyzer`. To run without
installing:

```sh
cargo run --release -- ~/some/dir
```

To run the tests:

```sh
cargo test
```

The UI tests compare against snapshots in `tests/snapshots`. After an
intentional UI change, review and accept the new ones with
[`cargo insta review`](https://insta.rs) (`cargo install cargo-insta`).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
