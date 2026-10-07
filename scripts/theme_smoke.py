#!/usr/bin/env python3
"""Emulate terminal color replies in a controlling PTY; never run real Homebrew."""
import fcntl
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time

ROOT = Path(__file__).resolve().parents[1]


def check(mode, terminal_light=True, colorfgbg=None, reply=True, config_theme=None):
    with tempfile.TemporaryDirectory(prefix="lazybrew-theme-") as folder:
        fake = Path(folder) / "brew"
        fake.write_text("#!/usr/bin/env python3\nimport sys\na=sys.argv[1:]\nprint('{}' if a[0] in ('info','outdated') else '[]' if a[0]=='services' else 'demo')\n")
        fake.chmod(0o700)
        config = Path(folder) / "config.toml"
        config.write_text('refresh_interval_secs = 0\n' + (f'theme = "{config_theme}"\n' if config_theme else ''))
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 38, 120, 0, 0))
        original = termios.tcgetattr(slave)

        def controlling_tty():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        env = {k: v for k, v in os.environ.items() if k not in ("COLORFGBG", "TMUX", "STY", "TERM_PROGRAM", "NO_COLOR")}
        env["TERM"] = "xterm-256color"
        if colorfgbg is not None:
            env["COLORFGBG"] = colorfgbg
        args = [str(ROOT / "target/release/lazybrew"), "--config", str(config), "--brew-path", str(fake), "--data-dir", folder + "/data"]
        if mode:
            args.extend(["--theme", mode])
        # Keep the session leader alive after the app exits so macOS does not
        # hang up the PTY before terminal-restoration attributes can be inspected.
        wrapper = "import subprocess,sys,signal; signal.signal(signal.SIGHUP,signal.SIG_IGN); result=subprocess.run(sys.argv[1:]); print('THEME_CHILD_EXIT:'+str(result.returncode),flush=True); sys.stdin.readline(); sys.exit(result.returncode)"
        process = subprocess.Popen([sys.executable, "-c", wrapper, *args], stdin=slave, stdout=slave, stderr=slave, env=env, preexec_fn=controlling_tty)
        raw = bytearray()
        answered = set()
        try:
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if select.select([master], [], [], .05)[0]:
                    raw.extend(os.read(master, 65536))
                for query, response in [(b"\x1b]10;?", b"\x1b]10;rgb:" + (b"0000/0000/0000" if terminal_light else b"ffff/ffff/ffff") + b"\x1b\\"), (b"\x1b]11;?", b"\x1b]11;rgb:" + (b"ffff/ffff/ffff" if terminal_light else b"0000/0000/0000") + b"\x1b\\"), (b"\x1b[c", b"\x1b[?1;2c")]:
                    if query in raw and query not in answered:
                        if reply:
                            os.write(master, response)
                        answered.add(query)
                if b"lazybrew" in raw:
                    break
            else:
                raise AssertionError("UI did not start")
            expected_light = (mode or config_theme) == "light" or (mode in (None, "auto") and config_theme in (None, "auto") and (terminal_light if reply else colorfgbg == "0;15"))
            expected = b"48;2;244;247;250" if expected_light else b"48;2;18;22;30"
            assert expected in raw, f"wrong palette: mode={mode}, config={config_theme}, reply={reply}, COLORFGBG={colorfgbg}"
            if mode in (None, "auto") and config_theme in (None, "auto"):
                assert b"\x1b]11;?" in raw, "automatic detection did not query the terminal"
            else:
                assert b"\x1b]11;?" not in raw, "explicit theme still queried the terminal"
            os.write(master, b"q")
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline and b"THEME_CHILD_EXIT:" not in raw:
                if select.select([master], [], [], .05)[0]:
                    raw.extend(os.read(master, 65536))
            assert b"THEME_CHILD_EXIT:0" in raw, "app did not exit successfully"
            assert termios.tcgetattr(slave) == original, "terminal mode not restored"
            os.write(master, b"\n")
            os.close(master)
            os.close(slave)
            master = slave = -1
            process.wait(timeout=2)
            assert process.returncode == 0
        finally:
            if master >= 0:
                os.close(master)
            if slave >= 0:
                os.close(slave)
            if process.poll() is None:
                process.kill()
                process.wait(timeout=3)


if __name__ == "__main__":
    check("auto", terminal_light=True, colorfgbg="15;0")
    check("auto", terminal_light=False, colorfgbg="0;15")
    check("light", terminal_light=False)
    check("dark", terminal_light=True, config_theme="light")
    check(None, config_theme="light")
    check("auto", reply=False, colorfgbg="0;15")
    check("auto", reply=False)
    print("Theme PTY checks passed: auto light/dark, CLI/config overrides, environment/default fallbacks and terminal restoration.")
