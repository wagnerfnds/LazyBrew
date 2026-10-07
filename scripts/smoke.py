#!/usr/bin/env python3
"""Exercise the release binary in a PTY with a fake Homebrew. No real mutations."""
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import struct
import subprocess
import tempfile
import termios
import time

ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT = 120, 38


def screen(raw):
    cells = [[" "] * WIDTH for _ in range(HEIGHT)]
    row = col = 0
    for token in re.split(r"(\x1b\[[0-?]*[ -/]*[@-~])", raw.decode("utf-8", "replace")):
        if token.startswith("\x1b["):
            command, args = token[-1], token[2:-1].split(";")
            if command in "Hf":
                row = int(args[0] or "1") - 1
                col = int(args[1] or "1") - 1 if len(args) > 1 else 0
            elif command == "J" and args == ["2"]:
                cells = [[" "] * WIDTH for _ in range(HEIGHT)]
        else:
            for char in token:
                if char == "\r":
                    col = 0
                elif char == "\n":
                    row += 1
                elif char.isprintable():
                    if 0 <= row < HEIGHT and 0 <= col < WIDTH:
                        cells[row][col] = char
                    col += 1
    return "\n".join("".join(row) for row in cells)


def main():
    with tempfile.TemporaryDirectory(prefix="lazybrew-smoke-") as folder:
        fake = Path(folder) / "brew"
        fake.write_text("#!/usr/bin/env python3\n" +
                        "import json,sys,time\nfrom pathlib import Path\n" +
                        f"fixtures=Path({str(ROOT / 'tests/fixtures/homebrew')!r})\n" +
                        f"if Path({str(Path(folder) / 'offline')!r}).exists():\n print('simulated offline', file=sys.stderr)\n sys.exit(1)\n" +
                        "args=sys.argv[1:]\n" +
                        "if args[:2]==['services','list']: name='services.json'\n" +
                        "elif args[:2]==['services','info']: name='service-info.json'\n" +
                        "elif args[0]=='outdated': name='outdated.json'\n" +
                        "elif args[0]=='info': name='installed.json'\n" +
                        "elif args[0] in ('formulae','casks'):\n print('atuin')\n sys.exit(0)\n" +
                        "elif args[0]=='search':\n print('Error: No formulae or casks found for '+repr(args[-1])+'.',file=sys.stderr)\n sys.exit(1)\n" +
                        "else:\n print('FAKE OPERATION',flush=True)\n time.sleep(30 if args[0]=='update' else .05)\n sys.exit(0)\n" +
                        "print((fixtures/name).read_text())\n")
        fake.chmod(0o700)
        config = Path(folder) / "config.toml"
        config.write_text('refresh_interval_secs = 30\n[[stacks]]\nname = "Demo"\nformulae = ["atuin"]\nservices = ["atuin"]\n')
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", HEIGHT, WIDTH, 0, 0))
        before = termios.tcgetattr(slave)

        start = time.monotonic()
        launch_args = [str(ROOT / "target/release/lazybrew"), "--theme", "dark", "--brew-path", str(fake), "--data-dir", str(Path(folder) / "data"), "--config", str(config)]
        process = subprocess.Popen(launch_args, stdin=slave, stdout=slave, stderr=slave, env={**os.environ, "TERM": "xterm-256color"})
        raw = bytearray()
        first = None

        def wait_for(text, timeout=8):
            nonlocal first
            end = time.monotonic() + timeout
            while time.monotonic() < end:
                if select.select([master], [], [], 0.05)[0]:
                    chunk = os.read(master, 65536)
                    if not chunk:
                        break
                    if first is None:
                        first = (time.monotonic() - start) * 1000
                    raw.extend(chunk)
                if text in screen(raw):
                    return
            raise AssertionError(f"Missing {text!r}\n{screen(raw)}")

        def click(x, y):
            os.write(master, f"\x1b[<0;{x+1};{y+1}M\x1b[<0;{x+1};{y+1}m".encode())

        try:
            wait_for("All caught up")
            wait_for("HOMEPAGE")  # details load without Enter
            assert b"\x1b[?1000h" in raw, "mouse capture not enabled"
            click(3, 7)  # Services workspace
            wait_for("SERVICE FILE")
            click(3, 35)  # Start action
            wait_for("ONE QUICK CONFIRMATION")
            # Cancel confirmation through the keyboard.
            os.write(master, b"n")
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.05)[0]:
                    raw.extend(os.read(master, 65536))
                if "ONE QUICK CONFIRMATION" not in screen(raw):
                    break
            assert "ONE QUICK CONFIRMATION" not in screen(raw), "cancel did not dismiss modal"
            assert "FAKE OPERATION" not in screen(raw), "cancel executed a mutation"
            os.write(master, b"3/nonexistent-package\r")
            wait_for("0 results")
            os.write(master, b"?")
            wait_for("MAKE YOURSELF AT HOME")
            os.write(master, b" ")
            # Drain repaint before quitting, to avoid filling the PTY output buffer.
            for _ in range(5):
                if select.select([master], [], [], 0.05)[0]:
                    raw.extend(os.read(master, 65536))
            os.write(master, b"U")
            wait_for("ONE QUICK CONFIRMATION")
            os.write(master, b"y")
            wait_for("FAKE OPERATION")
            os.write(master, b"X")
            wait_for("Cancelled")
            wait_for("All caught up")
            os.write(master, b"h")
            wait_for("OPERATION HISTORY")
            wait_for("Update Homebrew metadata")
            os.write(master, b"\x1b")
            # Wait for modal repaint before sending the next command.
            for _ in range(5):
                if select.select([master], [], [], 0.05)[0]:
                    raw.extend(os.read(master, 65536))
            os.write(master, b"S")
            wait_for("DEVELOPMENT STACKS")
            wait_for("Demo")
            os.write(master, b"i")
            wait_for("ONE QUICK CONFIRMATION")
            wait_for("Start service atuin")
            os.write(master, b"n")
            for _ in range(5):
                if select.select([master], [], [], 0.05)[0]:
                    raw.extend(os.read(master, 65536))
            baseline = json.loads((Path(folder) / "data/state.json").read_text())["snapshot"]["updated"]
            deadline = time.monotonic() + 35
            refreshed = False
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.1)[0]:
                    raw.extend(os.read(master, 65536))
                snapshot = json.loads((Path(folder) / "data/state.json").read_text())["snapshot"]
                if snapshot["updated"] > baseline:
                    refreshed = True
                    break
            assert refreshed, "automatic background refresh did not run"
            os.write(master, b"q")
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline and process.poll() is None:
                if select.select([master], [], [], 0.05)[0]:
                    raw.extend(os.read(master, 65536))
            process.wait(timeout=2)
            assert process.returncode == 0
            assert b"\x1b[?1000l" in raw, "mouse capture not restored"
            assert termios.tcgetattr(slave) == before, "terminal mode not restored"
            saved = json.loads((Path(folder) / "data/state.json").read_text())["snapshot"]
            assert saved["catalogue"], "catalogue not persisted"
            assert saved["history"][0]["result"] == "Cancelled", "cancellation not persisted"
            assert saved["history"][0]["finished"], "history missing terminal time"
            # Restart without a working backend: catalogue and history must still work.
            (Path(folder) / "offline").touch()
            raw.clear()
            process = subprocess.Popen(launch_args, stdin=slave, stdout=slave, stderr=slave, env={**os.environ, "TERM": "xterm-256color"})
            wait_for("simulated offline")
            os.write(master, b"3")
            wait_for("atuin")
            os.write(master, b"h")
            wait_for("OPERATION HISTORY")
            wait_for("Cancelled")
            os.write(master, b"\x1b")
            for _ in range(5):
                if select.select([master], [], [], 0.05)[0]:
                    raw.extend(os.read(master, 65536))
            os.write(master, b"q")
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline and process.poll() is None:
                if select.select([master], [], [], 0.05)[0]:
                    raw.extend(os.read(master, 65536))
            process.wait(timeout=2)
            assert process.returncode == 0
            assert termios.tcgetattr(slave) == before, "terminal mode not restored after restart"
            print(json.dumps({"checks": ["automatic details", "mouse workspace", "mouse action", "mouse cancel", "search", "help", "running cancellation", "persistent history", "cached catalogue", "automatic background refresh", "offline restart", "stack confirmation", "terminal restoration"], "first_output_ms": round(first, 2), "exit": process.returncode}))
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
            os.close(slave)


if __name__ == "__main__":
    main()
