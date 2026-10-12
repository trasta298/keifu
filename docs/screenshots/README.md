# README screenshots

These images show keifu at `16d5edf`, using a disposable demo repository.
They are captured from the running app's terminal output, replayed in xterm.js,
and saved as PNGs. The UI and text are not drawn by the capture script.

| Image | Scene | Terminal size |
| --- | --- | --- |
| `overview.png` | Branch graph and selected commit | 110 × 30 |
| `diff.png` | File diff with `--file-view replace` | 110 × 30 |
| `staging.png` | Working tree with one file staged through the TUI | 110 × 30 |
| `search.png` | Incremental branch search | 110 × 30 |
| `compact.png` | Stacked detail panes in a narrow terminal | 54 × 36 |

The terminal uses **Moralerspace Argon v2.0.0** (Regular and Bold), **Noto Color
Emoji 2.047**, a 16 px font, and a dark palette defined in `render.mjs`. PNGs are
captured at 2× scale. Fonts and browser dependencies are local capture tools;
they are not required to run keifu or bundled in this repository.
The canvas renderer draws box-drawing characters across the whole terminal cell,
keeping pane borders, graph edges, and scrollbar blocks connected.

## Recreate the images

Use Linux or macOS with Python 3, Node.js, Chrome/Chromium, Git, and Rust.
Run the following from the repository root.

1. Put these three font files in a local directory:
   - `MoralerspaceArgon-Regular.ttf` and `MoralerspaceArgon-Bold.ttf` from
     [Moralerspace v2.0.0](https://github.com/yuru7/moralerspace/releases/tag/v2.0.0)
     (`Moralerspace_v2.0.0.zip`).
   - `NotoColorEmoji.ttf` from [Noto Emoji](https://github.com/googlefonts/noto-emoji).
     The current captures use the font from Ubuntu's
     `fonts-noto-color-emoji` package, version `2.047-0ubuntu0.24.04.1`.

2. Install the capture dependencies outside the repository:

   ```bash
   capture_tools=$(mktemp -d)
   npm install --prefix "$capture_tools" --save-exact \
     @xterm/xterm@5.5.0 @xterm/addon-unicode11@0.8.0 \
     @xterm/addon-canvas@0.7.0 playwright-core@1.56.1
   ```

3. Build and capture, substituting your font directory and browser executable:

   ```bash
   cargo build --bin keifu
   python3 docs/screenshots/capture.py \
     --node-modules "$capture_tools/node_modules" \
     --chrome /usr/bin/google-chrome \
     --font-dir /path/to/fonts
   ```

`capture.py` creates its own temporary repository and configuration, disables
automatic fetch/refresh, and clears `NO_COLOR` for the captured process. It drives
keifu through the [debug server](../debugging.md), asserts the mode and visible
text, and checks that ANSI colors were emitted. Staging only affects the demo.
Each app process is closed after capture.

The five PNGs are overwritten in this directory. Use `--output /path/to/preview`
to inspect candidates first. The script prints the temporary directory containing
the demo repository, ANSI recordings, and text dumps; retain those when diagnosing
a capture. Relative dates in commit details naturally change on later runs.

After capture, inspect every image at README display size, verify color, emoji,
and font rendering, and check both README language versions. Update the source
commit above when recapturing from a newer revision.
