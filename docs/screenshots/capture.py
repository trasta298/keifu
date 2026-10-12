#!/usr/bin/env python3
"""Capture the real keifu TUI against a disposable Git repository (Linux/macOS)."""

import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import socket
import struct
import subprocess
import tempfile
import termios
import threading
import time


def create_demo(root):
    repo = root / "keifu-demo"
    repo.mkdir()
    env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")

    def git(*args):
        return subprocess.check_output(["git", *args], cwd=repo, env=env, stderr=subprocess.PIPE)

    def write(path, content):
        target = repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)

    day = 0

    def commit(message, *args):
        nonlocal day
        day += 1
        env.update(GIT_AUTHOR_DATE=f"2026-09-{day:02}T10:00:00+00:00",
                   GIT_COMMITTER_DATE=f"2026-09-{day:02}T10:00:00+00:00")
        git("add", ".")
        git("commit", "-m", message, *args)

    git("init", "-b", "main")
    git("config", "user.name", "trasta")
    git("config", "user.email", "trasta@example.com")
    write("Cargo.toml", '[package]\nname = "trail"\nversion = "0.1.0"\nedition = "2021"\n')
    write("src/main.rs", 'fn main() {\n    println!("Follow your work.");\n}\n')
    write("README.md", "# Trail\n\nA small workspace for your next idea.\n")
    commit("✨ feat: start the workspace")
    write("src/config.rs", 'pub const PAGE_SIZE: usize = 20;\n')
    commit("✨ feat: add project configuration")
    write("src/projects.rs", 'pub struct Project {\n    pub name: String,\n    pub archived: bool,\n}\n')
    commit("✨ feat: describe projects")
    git("branch", "feature/auth")
    git("switch", "-c", "feature/api")
    write("src/api.rs", 'pub fn status() -> &\'static str {\n    "ready"\n}\n')
    commit("✨ feat: add the status endpoint")
    write("src/routes.rs", 'pub const ROUTES: &[&str] = &["/status", "/projects"];\n')
    commit("✨ feat: register project routes")
    git("switch", "feature/auth")
    write("src/auth.rs", 'pub fn is_valid(token: &str) -> bool {\n    !token.trim().is_empty()\n}\n')
    commit("✨ feat: validate access tokens")
    write("src/session.rs", 'pub const SESSION_TTL: u64 = 3600;\n')
    commit("✨ feat: set a session lifetime")
    git("switch", "main")
    git("merge", "--no-ff", "--no-commit", "feature/api")
    commit("✨ feat: merge the API foundation")
    git("merge", "--no-ff", "--no-commit", "feature/auth")
    commit("✨ feat: merge authentication")
    git("tag", "v0.1.0")
    git("branch", "feature/search")
    git("switch", "-c", "feature/dashboard")
    write("src/dashboard.rs", 'pub fn greeting(name: &str) -> String {\n    format!("Welcome back, {name}")\n}\n')
    commit("✨ feat: add the activity dashboard")
    write("src/stats.rs", 'pub fn completion(done: usize, total: usize) -> usize {\n    done * 100 / total.max(1)\n}\n')
    commit("✨ feat: show project progress")
    git("switch", "main")
    git("merge", "--no-ff", "--no-commit", "feature/dashboard")
    commit("✨ feat: merge the dashboard")
    git("switch", "-c", "fix/empty-state")
    write("src/empty.rs", 'pub const EMPTY_MESSAGE: &str = "Start something new.";\n')
    commit("🐛 fix: show a helpful empty state")
    git("switch", "feature/search")
    search = '''pub fn search<'a>(items: &'a [String], query: &str) -> Vec<&'a String> {
    items
        .iter()
        .filter(|item| item.contains(query))
        .collect()
}
'''
    write("src/search.rs", search)
    write("src/lib.rs", "pub mod search;\n")
    commit("✨ feat: search project names")
    search = search.replace("    items\n", "    let query = query.trim().to_lowercase();\n\n    items\n")
    search = search.replace("item.contains(query)", "item.to_lowercase().contains(&query)")
    write("src/search.rs", search)
    write("tests/search.rs", '''use trail::search::search;

#[test]
fn search_ignores_case_and_whitespace() {
    let projects = vec!["Trail".to_string(), "Atlas".to_string()];
    assert_eq!(search(&projects, " TRA "), vec![&projects[0]]);
}
''')
    commit("✨ feat: make search case insensitive\n\nFind projects without matching their exact capitalization.\nTrim surrounding whitespace before filtering.")
    # Local remote refs demonstrate labels without a network dependency.
    for branch in ["main", "feature/search", "feature/dashboard", "fix/empty-state"]:
        git("update-ref", f"refs/remotes/origin/{branch}", branch)
    return repo


class Capture:
    def __init__(self, binary, repo, root, cols, rows):
        self.cols, self.rows = cols, rows
        self.stream = bytearray()
        master, slave = pty.openpty()
        self.master = master
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            self.port = probe.getsockname()[1]
        config = root / "config" / "keifu"
        config.mkdir(parents=True, exist_ok=True)
        (config / "config.toml").write_text("[refresh]\nauto_refresh = false\nauto_fetch = false\n")
        env = dict(os.environ, TERM="xterm-256color", COLORTERM="truecolor", TZ="UTC",
                   XDG_CONFIG_HOME=str(config.parent))
        env.pop("NO_COLOR", None)
        self.proc = subprocess.Popen(
            [str(binary), "--file-view", "replace", "--debug-listen", f"127.0.0.1:{self.port}"],
            cwd=repo, stdin=slave, stdout=slave, stderr=slave, start_new_session=True,
            env=env,
        )
        os.close(slave)
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()
        for _ in range(100):
            try:
                self.request("state")
                break
            except OSError:
                time.sleep(0.05)
        else:
            self.close()
            raise RuntimeError("keifu debug server did not start")

    def read(self):
        try:
            while chunk := os.read(self.master, 65536):
                self.stream.extend(chunk)
        except OSError:
            pass

    def request(self, cmd, **kwargs):
        with socket.create_connection(("127.0.0.1", self.port), timeout=5) as conn:
            conn.sendall((json.dumps(dict(cmd=cmd, **kwargs)) + "\n").encode())
            result = json.loads(conn.makefile("r").readline())
            assert result.get("ok"), result
            return result

    def keys(self, keys):
        self.request("keys", keys=keys)

    def save(self, root, name, mode, expected):
        deadline = time.monotonic() + 5
        while True:
            screen = self.request("dump", width=self.cols, height=self.rows)["screen"]
            if all(text in screen for text in expected):
                break
            if time.monotonic() > deadline:
                raise AssertionError(f"{name}: missing expected text\n{screen}")
            time.sleep(0.1)
        state = self.request("state")
        assert state["mode"] == mode, state
        # Let the physical terminal draw after the debug command; dump uses a separate backend.
        time.sleep(0.3)
        recording = bytes(self.stream)
        assert b"\x1b[38;" in recording, "terminal output contains no foreground colors"
        (root / f"{name}.ansi").write_bytes(recording)
        (root / f"{name}.txt").write_text(screen)
        return dict(name=name, cols=self.cols, rows=self.rows, expected=expected)

    def close(self):
        try:
            self.keys("<esc> <esc> q q")
            self.proc.wait(timeout=3)
        except (OSError, subprocess.TimeoutExpired):
            self.proc.terminate()
            self.proc.wait(timeout=3)
        finally:
            self.reader.join(timeout=1)
            os.close(self.master)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/keifu"))
    parser.add_argument("--node-modules", type=Path, required=True)
    parser.add_argument("--chrome", type=Path, required=True)
    parser.add_argument("--font-dir", type=Path, required=True,
                        help="directory containing Moralerspace Argon Regular/Bold and NotoColorEmoji.ttf")
    parser.add_argument("--output", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix="keifu-screenshots-"))
    repo = create_demo(root)
    shots = []
    app = Capture(args.binary.resolve(), repo, root, 110, 30)
    try:
        app.keys("@")
        shots.append(app.save(root, "overview", "normal", ["feature/search", "Changed Files (2)"]))
        app.keys("<space> <enter>")
        shots.append(app.save(root, "diff", "file_diff", ["src/search.rs", "to_lowercase", "Commit Detail"]))
        app.keys("<esc> <esc> / s e a")
        shots.append(app.save(root, "search", "input", ["Search branches", "feature/search"]))
        app.keys("<esc>")
        (repo / "README.md").write_text("# Trail\n\nFind projects by name, with case-insensitive search.\n")
        with (repo / "src/search.rs").open("a") as file:
            file.write('\npub const SEARCH_HINT: &str = "Find a project...";\n')
        (repo / "src/filters.rs").write_text('pub const DEFAULT_FILTER: &str = "all";\n')
        app.keys("R g")
        app.save(root, "working-tree", "normal", ["uncommitted changes (3)"])
        app.keys("<space> s j")
        shots.append(app.save(root, "staging", "file_select", ["1 staged", "src/filters.rs", "src/search.rs"]))
    finally:
        app.close()
    app = Capture(args.binary.resolve(), repo, root, 54, 36)
    try:
        app.keys("@ j")
        shots.append(app.save(root, "compact", "normal", ["feature/search", "Commit Detail", "Changed Files"]))
    finally:
        app.close()
    (root / "shots.json").write_text(json.dumps(shots))
    args.output.mkdir(parents=True, exist_ok=True)
    subprocess.run(["node", str(Path(__file__).with_name("render.mjs")),
                    str(root), str(args.output.resolve()), str(args.node_modules.resolve()),
                    str(args.chrome.resolve()), str(args.font_dir.resolve())], check=True)
    print(f"Captured {len(shots)} screenshots. ANSI recordings and screen assertions: {root}")


if __name__ == "__main__":
    main()
