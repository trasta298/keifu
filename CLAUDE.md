# keifu development notes

keifu is a Rust TUI (ratatui + crossterm) that visualizes Git commit graphs.

## Build / test

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo fmt --check
```

## Debugging the TUI yourself

keifu has a remote-control debug server, so you can drive the real app and
see its screen without a human. See docs/debugging.md for the full protocol.

```bash
# Launch in a PTY with the debug server (and optional log file)
script -qec "./target/debug/keifu --debug-listen 127.0.0.1:7167 --log-file /tmp/keifu.log" /dev/null &
sleep 2

# Send keys (same bindings as a user pressing them)
printf '%s\n' '{"cmd":"keys","keys":"j j <enter>"}' | nc -q1 127.0.0.1 7167

# Dump the rendered screen as plain text (size is optional)
printf '%s\n' '{"cmd":"dump","width":100,"height":30}' | nc -q1 127.0.0.1 7167

# Inspect app state (mode, selection, focus, async ops)
printf '%s\n' '{"cmd":"state"}' | nc -q1 127.0.0.1 7167

# Synthetic mouse input: click / scroll_up / scroll_down at (x, y)
printf '%s\n' '{"cmd":"mouse","kind":"click","x":5,"y":3}' | nc -q1 127.0.0.1 7167

# Quit the app
printf '%s\n' '{"cmd":"keys","keys":"q"}' | nc -q1 127.0.0.1 7167
```

Log levels are controlled with the `KEIFU_LOG` env var (RUST_LOG syntax,
default `debug`). Always reproduce UI bugs through this interface (drive →
dump → assert) before and after a fix.

## Architecture quick map

- `src/app.rs` — application state machine (`AppMode`), async diff loading
  via mpsc + threads, staging/commit/push, focus model
- `src/ui/` — ratatui widgets; `ui::draw` records pane rects into
  `App::layout` for mouse hit-testing
- `src/mouse.rs` — position-based mouse routing (uses `App::layout`)
- `src/keybindings.rs` — key → `Action` mapping per mode
- `src/git/` — git2-based operations; fetch/push shell out to `git` for auth
- `src/debug_server.rs` — NDJSON-over-TCP remote control described above
