import codecs
import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import termios
import time

import pyte


ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("HUNKY_BINARY", ROOT / "target/debug/hunky")).resolve()
TIMEOUT = float(os.environ.get("HUNKY_E2E_TIMEOUT", "10"))


class Repository:
    def __init__(self, root):
        self.path = root / "repo"
        self.path.mkdir()
        home = root / "home"
        home.mkdir()
        self.env = {
            key: value for key, value in os.environ.items()
            if not key.startswith(("GIT_", "HUNKY_LOG"))
        }
        self.env.update({
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "TERM": "xterm-256color",
            "LC_ALL": "C",
            "HUNKY_LOG": "1",
            "HUNKY_LOG_LEVEL": "trace",
            "HUNKY_LOG_FILE": str(root / "hunky.log"),
        })
        self.git("init", "--initial-branch=main", "--template=")
        self.git("config", "user.name", "Hunky E2E")
        self.git("config", "user.email", "e2e@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "core.autocrlf", "false")
        self.git("config", "core.hooksPath", os.devnull)

    def git(self, *args):
        result = subprocess.run(
            ["git", *args], cwd=self.path, env=self.env,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=TIMEOUT,
        )
        if result.returncode:
            raise AssertionError(
                "git {} failed ({}): {}".format(
                    " ".join(args), result.returncode,
                    result.stderr.decode("utf8", "replace"),
                )
            )
        return result.stdout

    def write(self, filename, content):
        file = self.path / filename
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_bytes(content.encode() if isinstance(content, str) else content)

    def commit(self, message="E2E baseline"):
        self.git("add", "--all")
        self.git("commit", "-m", message)

    def index(self):
        entries = {}
        for record in self.git("ls-files", "--stage", "-z").split(b"\0"):
            if not record:
                continue
            metadata, filename = record.split(b"\t", 1)
            mode, oid, stage = metadata.split()
            if stage != b"0":
                raise AssertionError("Unexpected unmerged index entry: " + repr(record))
            entries[filename.decode()] = (
                mode.decode(), self.git("cat-file", "blob", oid.decode()),
            )
        return entries

    def worktree(self):
        return {
            str(file.relative_to(self.path)): (
                "100755" if file.stat().st_mode & 0o111 else "100644",
                file.read_bytes(),
            )
            for file in self.path.rglob("*")
            if file.is_file() and ".git" not in file.relative_to(self.path).parts
        }

    def status(self):
        return self.git("status", "--porcelain=v1", "--untracked-files=all").decode()

    def diagnostics(self):
        return {
            "status": self.status(),
            "staged_diff": self.git("diff", "--cached", "--binary", "--no-ext-diff").decode("utf8", "replace"),
            "unstaged_diff": self.git("diff", "--binary", "--no-ext-diff").decode("utf8", "replace"),
        }


class Terminal:
    def __init__(self, repo, args=(), cwd=None, columns=140, rows=45):
        if not BINARY.is_file():
            raise AssertionError("Build the native binary first: cargo build --locked")
        self.repo = repo
        self.screen = pyte.Screen(columns, rows)
        self.stream = pyte.Stream(self.screen)
        self.decoder = codecs.getincrementaldecoder("utf8")("replace")
        self.transcript = bytearray()
        self.exit_code = None
        self.closed = False
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            try:
                fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
                os.chdir(cwd or repo.path)
                os.execve(str(BINARY), [str(BINARY), *args], repo.env)
            except OSError as error:
                os.write(2, ("Cannot start native E2E binary: {}\n".format(error)).encode())
                os._exit(127)
        os.set_blocking(self.fd, False)

    @property
    def text(self):
        return "\n".join(self.screen.display)

    def poll_exit(self):
        if self.exit_code is None:
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                self.exit_code = os.waitstatus_to_exitcode(status)
        return self.exit_code

    def pump(self, timeout=0.05):
        if self.closed:
            return
        if select.select([self.fd], [], [], timeout)[0]:
            try:
                chunk = os.read(self.fd, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                chunk = b""
            if chunk:
                self.transcript.extend(chunk)
                self.stream.feed(self.decoder.decode(chunk))
        self.poll_exit()

    def wait(self, predicate, description, allow_exit=False):
        deadline = time.monotonic() + TIMEOUT
        while time.monotonic() < deadline:
            self.pump()
            if predicate():
                return
            if self.exit_code is not None and not allow_exit:
                break
        raise AssertionError(
            "Timed out waiting for {} (exit={!r})\n{}\n{}".format(
                description, self.exit_code, self.text,
                json.dumps(self.repo.diagnostics(), indent=2),
            )
        )

    def see(self, text):
        self.wait(lambda: text in self.text, repr(text))

    def send(self, keys):
        if self.poll_exit() is not None:
            raise AssertionError("Cannot send keys to exited process")
        os.write(self.fd, keys.encode())

    def barrier(self):
        # Toggling help gives an observable acknowledgment for no-op inputs.
        self.send("h")
        self.see("Keys")
        self.send("h")
        self.wait(lambda: "Keys" not in self.text, "help closed")

    def quit(self, keys="q"):
        self.send(keys)
        self.wait(lambda: self.exit_code is not None, "clean process exit", allow_exit=True)
        if self.exit_code != 0:
            raise AssertionError("Unexpected exit code: " + str(self.exit_code))
        if termios.tcgetattr(self.fd)[3] & (termios.ICANON | termios.ECHO) != (termios.ICANON | termios.ECHO):
            raise AssertionError("Terminal canonical mode/echo were not restored")
        if b"\x1b[?1049l" not in self.transcript:
            raise AssertionError("Alternate terminal screen was not restored")

    def resize(self, columns, rows):
        self.screen.resize(lines=rows, columns=columns)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
        os.kill(self.pid, signal.SIGWINCH)
        self.wait(lambda: self.screen.display[-1].endswith("\u2518"), "resized terminal frame")

    def close(self):
        if self.closed:
            return
        if self.poll_exit() is None:
            os.killpg(self.pid, signal.SIGTERM)
            deadline = time.monotonic() + 2
            while self.poll_exit() is None and time.monotonic() < deadline:
                self.pump()
            if self.poll_exit() is None:
                os.killpg(self.pid, signal.SIGKILL)
                _, status = os.waitpid(self.pid, 0)
                self.exit_code = os.waitstatus_to_exitcode(status)
        os.close(self.fd)
        self.closed = True
