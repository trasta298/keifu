# 🧬 keifu

[日本語版はこちら](docs/README_JA.md)

keifu (系譜, /keːɸɯ/) is a terminal UI tool that visualizes Git commit graphs. It shows a colored commit graph, commit details, and a summary of changed files, and lets you perform basic branch operations.

![Screenshot](docs/win_terminal.png)

## Features

- Unicode commit graph with per-branch colors
- Commit list with branch labels, date, author, short hash, and message
- Commit detail panel with full message and changed file stats (+/-)
- Git operations: checkout, create/delete branch

## Requirements

- Run inside a Git repository (auto-discovery from current directory)
- A terminal with Unicode line drawing support and color
- Rust toolchain (for building from source)

## Installation

### From crates.io

```bash
cargo install keifu
```

### From source

```bash
cargo install --path .
```

Or:

```bash
cargo build --release
./target/release/keifu
```

## Usage

Run inside a Git repository:

```bash
keifu
```

## Keybindings

### Navigation

| Key | Action |
| --- | --- |
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `]` / `Tab` | Jump to next commit that has branch labels |
| `[` / `Shift+Tab` | Jump to previous commit that has branch labels |
| `Ctrl+d` | Page down |
| `Ctrl+u` | Page up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |

### Git operations

| Key | Action |
| --- | --- |
| `Enter` | Checkout selected branch/commit |
| `b` | Create branch at selected commit |
| `d` | Delete branch (local, non-HEAD) |

### Other

| Key | Action |
| --- | --- |
| `R` | Refresh repository data |
| `?` | Toggle help |
| `q` / `Esc` | Quit |

## Notes and limitations

- The TUI loads up to 500 commits across all branches.
- Merge commits are diffed against the first parent; the initial commit is diffed against an empty tree.
- Changed files are capped at 50 and binary files are skipped.
- Checking out `origin/xxx` creates or updates a local branch and sets its upstream. If the local branch exists but points to a different commit, it is force-updated to match the remote.
- Remote branches are displayed, but delete operations only work with local branches.

## License

MIT
