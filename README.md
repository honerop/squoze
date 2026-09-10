# squoze

A lean, terminal-first archiver: create, extract and preview archives without leaving your shell.

## Features

- **Create** archives from files or directories
- **Extract** archives to a directory (with zip-slip protection — malicious paths like `../` are rejected)
- **Interactive TUI preview**: browse archive contents as an expandable file tree, open text files inline, and inspect sizes per entry
- **Fast**: built on pure-Rust crates; no external binaries required (7z included)
- **Beautiful crash messages** via `human-panic`

## Supported formats

| Format     | Extension(s)          | Create | Extract | Preview |
|------------|-----------------------|:------:|:-------:|:-------:|
| ZIP        | `.zip`                | ✅     | ✅      | ✅      |
| Tar        | `.tar`                | ✅     | ✅      | ✅      |
| Gzip       | `.gz`                 | ✅     | ✅      | ✅      |
| Tar+Gzip   | `.tar.gz`, `.tgz`     | ✅     | ✅      | ✅      |
| Zstandard  | `.zst`                | ✅     | ✅      | ✅      |
| Tar+Zstd   | `.tar.zst`            | ✅     | ✅      | ✅      |
| 7-Zip      | `.7z`                 | ✅     | ✅      | ✅      |

Notes:

- `.gz` and `.zst` are single-file compressors — they only accept a file, not a directory.
- The preview TUI shows the uncompressed size for `.gz`/`.zst` files (read from the frame header when available).
- Encrypted 7z archives are not supported (7z preview uses an empty password).

## Installation

Requires [Rust](https://rustup.rs) (edition 2024, i.e. Rust 1.85+).

```sh
cargo install --path .
```

Or build from source:

```sh
git clone <repo-url>
cd modern-archiver
cargo build --release
```

A [Nix](https://nixos.org) flake is included (`flake.nix`) for `nix develop` shells.

## Usage

```
squoze [OPTIONS]
```

Create an archive (format inferred from the output extension):

```sh
squoze --create ./my-project --output project.tar.zst
squoze -c ./my-project -o project.7z
```

Extract an archive:

```sh
squoze --extract project.tar.zst --output ./out-dir
squoze -x media.7z -o media/
```

Preview an archive (interactive TUI, nothing is written):

```sh
squoze --preview project.zip
squoze -p backup.tar.gz
```

### TUI keys

| Key              | Action                                        |
|------------------|-----------------------------------------------|
| `↑` / `↓` / `j` / `k` | Move selection / scroll                  |
| `←` / `→`        | Collapse / expand directory                   |
| `Enter`          | Toggle directory, or open file in viewer      |
| `PageUp` / `PageDown` | Scroll by page                           |
| `Home` / `End`   | Jump to top / bottom                          |
| `Esc` / `q`      | Close viewer / quit                           |

File contents are shown inline for text files (truncated at 1 MiB); binary files show a placeholder.

## Development

```sh
cargo test     # roundtrip, preview and format-detection tests
cargo run -- --help
```

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
