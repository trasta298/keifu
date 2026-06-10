# Debugging keifu

keifu ships two debugging facilities aimed at both humans and AI agents:
file-based logging and a remote control server.

## Logging

```bash
keifu --log-file /tmp/keifu.log
```

Appends `tracing` logs to the file. The level filter is read from the
`KEIFU_LOG` environment variable using `RUST_LOG` syntax (default: `debug`).

```bash
KEIFU_LOG=trace keifu --log-file /tmp/keifu.log
```

The log rotates once it exceeds 5 MB (one `.old` generation is kept).

### Measuring performance with the log alone

The log is enough to diagnose slowness — no debug server needed:

```bash
keifu --log-file /tmp/keifu.log
# ...reproduce the slow interaction, then quit with q...
tail -30 /tmp/keifu.log
```

- Operations slower than 10 ms are logged live as `slow operation`
  (`draw`, `events`, `refresh`, `refresh.status`, `refresh.log`,
  `refresh.graph`, `stage_states`, `open_file_diff`, ...)
- On exit, a `perf summary` line per operation reports count/avg/max

## Remote control server

```bash
keifu --debug-listen 127.0.0.1:7167
```

Listens for newline-delimited JSON commands over TCP. Each request line gets
exactly one JSON response line. Only bind to loopback addresses; the protocol
is unauthenticated.

### Commands

| Request | Response |
| --- | --- |
| `{"cmd":"keys","keys":"j j <enter>"}` | `{"ok":true}` |
| `{"cmd":"mouse","kind":"click","x":5,"y":3}` | `{"ok":true}` |
| `{"cmd":"dump"}` | `{"ok":true,"width":…,"height":…,"screen":"…"}` |
| `{"cmd":"dump","width":100,"height":30}` | same, rendered at the given size |
| `{"cmd":"state"}` | `{"ok":true,"mode":…,"selected_index":…,…}` |
| `{"cmd":"perf"}` | per-operation timing stats and recent slow operations |

- `keys` — whitespace-separated tokens fed through the normal keybinding
  layer. Single characters are sent as-is (uppercase implies Shift). Special
  keys: `<enter> <esc> <tab> <backtab> <space> <up> <down> <left> <right>
  <home> <end> <pgup> <pgdn> <backspace> <c-x>` (Ctrl+x).
- `mouse` — `kind` is `click`, `scroll_up`, or `scroll_down`; `x`/`y` are
  screen coordinates (0-based).
- `dump` — renders the current state to plain text. Without `width`/`height`
  the real terminal size is used (falling back to sane bounds when headless).
- `state` — mode, focused pane, selection, HEAD, async operation status.
- `perf` — count/avg/max/last per instrumented operation (`draw`, `events`,
  `refresh`, `refresh.status`, `refresh.log`, `refresh.graph`,
  `stage_states`, `open_file_diff`, …) plus the most recent operations that
  exceeded 10 ms. Slow operations are also emitted as `tracing` debug events
  when `--log-file` is active.

### Example session

```bash
script -qec "keifu --debug-listen 127.0.0.1:7167" /dev/null &
sleep 2
printf '%s\n' '{"cmd":"keys","keys":"j j"}' | nc -q1 127.0.0.1 7167
printf '%s\n' '{"cmd":"dump","width":100,"height":30}' | nc -q1 127.0.0.1 7167
printf '%s\n' '{"cmd":"keys","keys":"q"}' | nc -q1 127.0.0.1 7167
```
