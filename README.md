# 🧬 keifu

[![Crate Status](https://img.shields.io/crates/v/keifu.svg)](https://crates.io/crates/keifu)
[![Built With Ratatui](https://img.shields.io/badge/Built_With-Ratatui-000?logo=ratatui&logoColor=fff&labelColor=000&color=fff)](https://ratatui.rs)

[日本語版はこちら](docs/README_JA.md)

keifu (系譜, /keːɸɯ/) is a terminal UI tool that visualizes Git commit graphs. It shows a colored commit graph, commit details, and a summary of changed files, and lets you perform basic branch operations.

![Screenshot](docs/win_terminal.png)

## Motivation

- **Readable commit graph** — `git log --graph` is hard to read; keifu renders a cleaner, color-coded graph
- **Fast branch switching** — With the rise of vibe coding, working on multiple branches in parallel has become common. keifu makes branch switching quick and visual
- **Keep it simple** — Only basic Git operations are supported; this is not a full-featured Git client
- **Narrow terminal friendly** — Works well in split panes and small windows
- **No image protocol required** — Works on any terminal with Unicode support

## Features

- Unicode commit graph with per-branch colors
- Commit list with branch labels, date, author, short hash, and message (some fields may be hidden on narrow terminals)
- Commit detail panel with full message and changed file stats (+/-)
- File diff view with syntax highlighting and word-level change emphasis
- Git operations: checkout, create/delete branch, fetch
- Branch search with dropdown UI

## Requirements

- Run inside a Git repository (auto-discovery from current directory)
- A terminal with Unicode line drawing support and color
- `git` command in PATH (required for fetch)
- Rust toolchain (for building from source)

## Installation

### From crates.io

```bash
cargo install keifu
```

### With mise

```bash
mise use -g github:trasta298/keifu@latest
```

### With Homebrew

```bash
brew install trasta298/tap/keifu
```

### From source

```bash
git clone https://github.com/trasta298/keifu && cd keifu && cargo install --path .
```

## Usage

Run inside a Git repository:

```bash
keifu
```

## Configuration

See [docs/configuration.md](docs/configuration.md) for configuration options.

## Keybindings

### Navigation

| Key | Action |
| --- | --- |
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `]` / `Tab` | Jump to next commit that has branch labels |
| `[` / `Shift+Tab` | Jump to previous commit that has branch labels |
| `h` / `←` | Select left branch (same commit) |
| `l` / `→` | Select right branch (same commit) |
| `Ctrl+d` | Page down |
| `Ctrl+u` | Page up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |
| `@` | Jump to HEAD (current branch) |
| `Space` | Open file diff view |

### Git operations

| Key | Action |
| --- | --- |
| `Enter` | Checkout selected branch/commit |
| `b` | Create branch at selected commit |
| `d` | Delete branch (local, non-HEAD) |
| `f` | Fetch from origin |

### Search

| Key | Action |
| --- | --- |
| `/` | Search branches (incremental fuzzy search) |
| `↑` / `Ctrl+k` | Select previous result |
| `↓` / `Ctrl+j` | Select next result |
| `Enter` | Jump to selected branch |
| `Esc` / `Backspace` on empty | Cancel search |

### File diff view

| Key | Action |
| --- | --- |
| `j` / `k` / `↑` / `↓` | Scroll up/down |
| `h` / `l` / `←` / `→` | Scroll left/right |
| `Ctrl+d` / `Ctrl+u` | Half-page down/up |
| `Ctrl+f` / `Ctrl+b` | Full page down/up |
| `g` / `G` | Go to top/bottom |
| `0` | Scroll to line start |
| `]` / `[` | Jump to next/previous hunk |
| `n` / `N` | Jump to next/previous file |
| `Esc` / `q` | Back to file select / close |

### Other

| Key | Action |
| --- | --- |
| `R` | Refresh repository data |
| `?` | Toggle help |
| `q` / `Esc` | Quit |

## Notes and limitations

- The TUI loads up to 500 commits across all branches.
- Merge commits are diffed against the first parent; the initial commit is diffed against an empty tree.
- Changed files are capped at 50. Binary files are shown without line stats.
- If there are staged, unstaged, or untracked changes, an "uncommitted changes" row appears at the top.
- When multiple branches point to the same commit, the label is collapsed to a single name with a `+N` suffix (e.g., `main +2`). Use `h`/`l` or `←`/`→` to switch between them.
- Checking out `origin/xxx` creates or updates a local branch. Upstream is set only when creating a new branch. If the local branch exists but points to a different commit, it is force-updated to match the remote.
- Remote branches are displayed, but delete operations only work with local branches.
- Fetch requires the `origin` remote to be configured.

## License

MIT
