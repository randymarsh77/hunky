import json
import os
from pathlib import Path
import re
import shlex
import sys
import tempfile
import unittest

from harness import Repository, ROOT, Terminal


def entries(files):
    return {
        name: ("100644", content.encode() if isinstance(content, str) else content)
        for name, content in files.items()
    }


class NativeE2E(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="hunky-native-e2e-")
        self.addCleanup(self.temp.cleanup)
        self.repo = Repository(Path(self.temp.name))
        self.terminals = []
        self.addCleanup(self.close_terminals)

    def close_terminals(self):
        result = self._outcome.result
        failed = (
            any(test is self for test, _ in result.failures + result.errors)
            or any(error is not None for _, error in getattr(self._outcome, "errors", []))
        )
        try:
            if failed:
                directory = ROOT / "target/e2e-results" / self.id()
                directory.mkdir(parents=True, exist_ok=True)
                for number, terminal in enumerate(self.terminals):
                    (directory / "{}.ansi".format(number)).write_bytes(terminal.transcript)
                    (directory / "{}.screen.txt".format(number)).write_text(terminal.text)
                (directory / "git.json").write_text(json.dumps(self.repo.diagnostics(), indent=2))
                log = Path(self.temp.name) / "hunky.log"
                if log.exists():
                    (directory / "hunky.log").write_bytes(log.read_bytes())
        finally:
            for terminal in self.terminals:
                terminal.close()

    def start(self, args=("--no-splash",), cwd=None):
        terminal = Terminal(self.repo, args=args, cwd=cwd)
        self.terminals.append(terminal)
        terminal.see("Mode: VIEW")
        self.terminal = terminal
        self.head = self.repo.git("rev-parse", "--verify", "HEAD") if (self.repo.path / ".git/refs/heads/main").exists() else None
        self.worktree = self.repo.worktree()
        return terminal

    def baseline(self, files):
        for name, content in files.items():
            self.repo.write(name, content)
        self.repo.commit()

    def assert_state(self, expected, status):
        self.terminal.wait(
            lambda: self.repo.index() == entries(expected) and self.repo.status() == status,
            "exact index contents and status " + repr(status),
        )
        self.assertEqual(self.repo.index(), entries(expected))
        self.assertEqual(self.repo.worktree(), self.worktree, "Staging must not modify working files")
        if self.head is not None:
            self.assertEqual(self.repo.git("rev-parse", "HEAD"), self.head, "Staging must not alter HEAD")

    def stage(self, expected, status):
        self.terminal.send("s")
        self.assert_state(expected, status)

    def select_line(self, content, occurrence=0):
        self.terminal.send("l")
        self.terminal.see("\u25ba")
        for _ in range(100):
            selected = next((line for line in self.terminal.screen.display if "\u25ba" in line), "")
            matching = [line for line in self.terminal.screen.display if content in line]
            if content in selected and matching.index(selected) == occurrence:
                return
            previous = self.terminal.text
            self.terminal.send("j")
            self.terminal.wait(lambda: self.terminal.text != previous, "next selected change line")
        self.fail("Could not select line " + repr(content))

    def test_file_stage_unstage_restage_and_quit(self):
        self.baseline({"a.txt": "old\n", "b.txt": "untouched\n"})
        self.repo.write("a.txt", "new\n")
        self.start()
        self.terminal.send("\t")
        self.terminal.see("Files [FOCUSED]")
        self.stage({"a.txt": "new\n", "b.txt": "untouched\n"}, "M  a.txt\n")
        self.stage({"a.txt": "old\n", "b.txt": "untouched\n"}, " M a.txt\n")
        self.stage({"a.txt": "new\n", "b.txt": "untouched\n"}, "M  a.txt\n")
        self.terminal.quit()

    def test_hunk_stage_unstage_restage(self):
        self.baseline({"a.txt": "one\ntwo\nthree\n"})
        self.repo.write("a.txt", "one\nupdated\nthree\n")
        self.start()
        self.stage({"a.txt": "one\nupdated\nthree\n"}, "M  a.txt\n")
        self.stage({"a.txt": "one\ntwo\nthree\n"}, " M a.txt\n")
        self.stage({"a.txt": "one\nupdated\nthree\n"}, "M  a.txt\n")
        self.terminal.quit()

    def test_only_selected_hunk_and_back_navigation(self):
        original = "".join("line {:02}\n".format(i) for i in range(30))
        first = original.replace("line 02\n", "first edit\n")
        both = first.replace("line 25\n", "second edit\n")
        self.baseline({"a.txt": original})
        self.repo.write("a.txt", both)
        self.start()
        self.terminal.see("Hunk 1/2")
        self.stage({"a.txt": first}, "MM a.txt\n")
        self.terminal.send(" ")
        self.terminal.see("Hunk 2/2")
        self.stage({"a.txt": both}, "M  a.txt\n")
        self.terminal.send("b")
        self.terminal.see("Hunk 1/2")
        self.stage({"a.txt": original.replace("line 25\n", "second edit\n")}, "MM a.txt\n")
        self.terminal.quit()

    def test_single_addition_and_removal_round_trip(self):
        original = "one\ntwo\nthree\n"
        self.baseline({"a.txt": original})
        self.repo.write("a.txt", "one\nnew\nthree\n")
        self.start()
        self.select_line("+ new")
        self.stage({"a.txt": "one\ntwo\nnew\nthree\n"}, "MM a.txt\n")
        self.stage({"a.txt": original}, " M a.txt\n")
        self.terminal.send("k")
        self.terminal.wait(
            lambda: any("\u25ba" in line and "- two" in line for line in self.terminal.screen.display),
            "selected removal",
        )
        self.stage({"a.txt": "one\nthree\n"}, "MM a.txt\n")
        self.stage({"a.txt": original}, " M a.txt\n")
        self.terminal.quit()

    def test_duplicate_line_selection_stages_only_second_occurrence(self):
        original = "one\ntwo\nthree\nfour\n"
        self.baseline({"a.txt": original})
        self.repo.write("a.txt", "one\nduplicate\ntwo\nthree\nduplicate\nfour\n")
        self.start()
        self.select_line("+ duplicate", occurrence=1)
        self.stage({"a.txt": "one\ntwo\nthree\nduplicate\nfour\n"}, "MM a.txt\n")
        self.stage({"a.txt": original}, " M a.txt\n")
        self.terminal.quit()

    def test_partial_hunk_stages_remainder_preserving_other_file(self):
        self.baseline({"a.txt": "one\ntwo\nthree\n", "b.txt": "other\n"})
        self.repo.write("a.txt", "one\nalready staged\nthree\n")
        self.repo.git("add", "a.txt")
        self.repo.write("a.txt", "one\nalready staged\nremaining\nthree\n")
        self.repo.write("b.txt", "leave unstaged\n")
        self.start()
        self.stage({"a.txt": "one\nalready staged\nremaining\nthree\n", "b.txt": "other\n"},
                   "M  a.txt\n M b.txt\n")
        self.terminal.quit()

    def test_line_stage_preserves_existing_index_changes(self):
        self.baseline({"a.txt": "one\ntwo\nthree\nfour\n", "b.txt": "original\n"})
        self.repo.write("a.txt", "one\nstaged\nthree\nfour\n")
        self.repo.write("b.txt", "already indexed\n")
        self.repo.git("add", "a.txt", "b.txt")
        self.repo.write("a.txt", "one\nstaged\nthree\nfour\nextra\n")
        self.start()
        self.select_line("+ extra")
        self.stage({"a.txt": "one\nstaged\nthree\nfour\nextra\n", "b.txt": "already indexed\n"},
                   "M  a.txt\nM  b.txt\n")
        self.stage({"a.txt": "one\nstaged\nthree\nfour\n", "b.txt": "already indexed\n"},
                   "MM a.txt\nM  b.txt\n")
        self.terminal.quit()

    def test_new_file_with_intent_to_add_round_trip(self):
        self.baseline({"baseline.txt": "baseline\n"})
        self.repo.write("added.txt", "new contents\n")
        self.repo.git("add", "-N", "added.txt")
        self.start()
        self.terminal.see("added.txt (Hunk")
        self.stage({"baseline.txt": "baseline\n", "added.txt": "new contents\n"}, "A  added.txt\n")
        self.stage({"baseline.txt": "baseline\n"}, "?? added.txt\n")
        self.terminal.quit()

    def test_deleted_file_round_trip(self):
        self.baseline({"a.txt": "delete me\n", "b.txt": "keep me\n"})
        (self.repo.path / "a.txt").unlink()
        self.start()
        self.stage({"b.txt": "keep me\n"}, "D  a.txt\n")
        self.stage({"a.txt": "delete me\n", "b.txt": "keep me\n"}, " D a.txt\n")
        self.terminal.quit()

    def test_space_and_unicode_filename(self):
        filename = "nested directory/caf\u00e9 notes.txt"
        self.baseline({filename: "before\n"})
        self.repo.write(filename, "after\n")
        self.repo.git("config", "core.quotepath", "false")
        self.start()
        self.terminal.send("\t")
        self.terminal.see("Files [FOCUSED]")
        self.stage({filename: "after\n"}, 'M  "{}"\n'.format(filename))
        self.stage({filename: "before\n"}, ' M "{}"\n'.format(filename))
        self.terminal.quit()

    def test_no_trailing_newline_file_stage(self):
        self.baseline({"a.txt": b"old"})
        self.repo.write("a.txt", b"new")
        self.start()
        self.terminal.send("\t")
        self.terminal.see("Files [FOCUSED]")
        self.stage({"a.txt": b"new"}, "M  a.txt\n")
        self.stage({"a.txt": b"old"}, " M a.txt\n")
        self.terminal.quit()

    def test_crlf_file_stage_preserves_bytes(self):
        self.baseline({"a.txt": b"one\r\ntwo\r\n"})
        self.repo.write("a.txt", b"one\r\nchanged\r\n")
        self.start()
        self.terminal.send("\t")
        self.terminal.see("Files [FOCUSED]")
        self.stage({"a.txt": b"one\r\nchanged\r\n"}, "M  a.txt\n")
        self.stage({"a.txt": b"one\r\ntwo\r\n"}, " M a.txt\n")
        self.terminal.quit()

    def test_executable_mode_preserved(self):
        self.baseline({"script.sh": "#!/bin/sh\necho old\n"})
        os.chmod(self.repo.path / "script.sh", 0o755)
        self.repo.commit("Make script executable")
        self.repo.write("script.sh", "#!/bin/sh\necho new\n")
        self.start()
        self.terminal.send("\t")
        self.terminal.see("Files [FOCUSED]")
        self.terminal.send("s")
        expected = {"script.sh": ("100755", b"#!/bin/sh\necho new\n")}
        self.terminal.wait(lambda: self.repo.index() == expected, "executable index entry")
        self.assertEqual(self.repo.worktree(), self.worktree)
        self.assertEqual(self.repo.status(), "M  script.sh\n")
        self.terminal.quit()

    def test_file_navigation_does_not_stage_neighbors(self):
        self.baseline({"a.txt": "a\n", "b.txt": "b\n", "c.txt": "c\n"})
        for name in ("a", "b", "c"):
            self.repo.write(name + ".txt", name + " changed\n")
        self.start()
        self.terminal.send("\tj")
        self.terminal.see("b.txt (Hunk")
        self.stage({"a.txt": "a\n", "b.txt": "b changed\n", "c.txt": "c\n"},
                   " M a.txt\nM  b.txt\n M c.txt\n")
        self.terminal.send("k")
        self.terminal.see("a.txt (Hunk")
        self.stage({"a.txt": "a changed\n", "b.txt": "b changed\n", "c.txt": "c\n"},
                   "M  a.txt\nM  b.txt\n M c.txt\n")
        self.terminal.quit()

    def test_watcher_refreshes_external_worktree_edit(self):
        self.baseline({"a.txt": "one\n"})
        self.start()
        self.repo.write("a.txt", "watcher change\n")
        self.worktree = self.repo.worktree()
        self.terminal.see("+ watcher change")
        self.stage({"a.txt": "watcher change\n"}, "M  a.txt\n")
        self.terminal.quit()

    def test_watcher_refreshes_external_index_edit(self):
        self.baseline({"a.txt": "before\n"})
        self.repo.write("a.txt", "after\n")
        self.start()
        self.repo.git("add", "a.txt")
        self.terminal.see("[STAGED")
        self.stage({"a.txt": "before\n"}, " M a.txt\n")
        self.terminal.quit()

    def test_help_focus_staging_is_noop(self):
        self.baseline({"a.txt": "before\n"})
        self.repo.write("a.txt", "after\n")
        self.start()
        self.terminal.send("h\t")
        self.terminal.see("Keys [FOCUSED]")
        self.terminal.send("s")
        self.terminal.send("h")
        self.terminal.wait(lambda: "Keys" not in self.terminal.text, "help closed")
        self.assert_state({"a.txt": "before\n"}, " M a.txt\n")
        self.terminal.quit()

    def test_review_acceptance_does_not_modify_repository(self):
        self.baseline({"a.txt": "initial\n"})
        self.repo.write("a.txt", "committed\n")
        self.repo.commit("Review target")
        self.repo.write("a.txt", "working edit\n")
        self.start()
        before = self.repo.index()
        self.terminal.send("r")
        self.terminal.see("Review target")
        self.terminal.send("\r")
        self.terminal.see("Mode: REVIEW")
        self.terminal.send("s")
        self.terminal.see("[ACCEPTED")
        self.assertEqual(self.repo.index(), before)
        self.assertEqual(self.repo.worktree(), self.worktree)
        self.assertEqual(self.repo.git("rev-parse", "HEAD"), self.head)
        self.terminal.send("\x1b")
        self.terminal.see("Mode: VIEW")
        self.stage({"a.txt": "working edit\n"}, "M  a.txt\n")
        self.terminal.quit()

    def test_explicit_repo_from_outside_and_nested_discovery(self):
        self.baseline({"a.txt": "old\n"})
        self.repo.write("a.txt", "new\n")
        self.start(args=("--no-splash", "--repo", str(self.repo.path)),
                   cwd=Path(self.temp.name))
        self.stage({"a.txt": "new\n"}, "M  a.txt\n")
        self.terminal.quit()
        nested = self.repo.path / "nested"
        nested.mkdir()
        self.start(cwd=nested)
        self.stage({"a.txt": "old\n"}, " M a.txt\n")
        self.terminal.quit()

    def test_empty_repo_quit_and_terminal_restoration(self):
        self.start()
        self.terminal.send("s")
        self.terminal.barrier()
        self.assert_state({}, "")
        self.terminal.quit("\x03")

    def test_resize_and_reset_preserve_staging_behavior(self):
        self.baseline({"a.txt": "before\n"})
        self.repo.write("a.txt", "after\n")
        self.start()
        self.terminal.send("l")
        self.terminal.see("\u25ba")
        self.terminal.resize(90, 28)
        self.terminal.see("Mode: VIEW")
        self.terminal.send("\x1b")
        self.terminal.wait(lambda: "\u25ba" not in self.terminal.text, "line mode reset")
        self.stage({"a.txt": "after\n"}, "M  a.txt\n")
        self.terminal.quit()

    def test_splash_skip_does_not_stage(self):
        self.baseline({"a.txt": "before\n"})
        self.repo.write("a.txt", "after\n")
        terminal = Terminal(self.repo)
        self.terminals.append(terminal)
        terminal.wait(lambda: bool(re.search(r"\S", terminal.text)), "first splash frame")
        terminal.send("s")
        terminal.see("Mode: VIEW")
        terminal.barrier()
        self.assertEqual(self.repo.index(), entries({"a.txt": "before\n"}))
        terminal.quit()

    def test_untracked_file_without_intent_to_add(self):
        self.baseline({"baseline.txt": "baseline\n"})
        self.repo.write("new.txt", "untracked\n")
        self.start()
        self.terminal.see("new.txt (Hunk")
        self.stage({"baseline.txt": "baseline\n", "new.txt": "untracked\n"}, "A  new.txt\n")
        self.stage({"baseline.txt": "baseline\n"}, "?? new.txt\n")
        self.terminal.quit()

    def test_first_file_in_unborn_repository(self):
        self.repo.write("first.txt", "first commit contents\n")
        self.start()
        self.terminal.see("first.txt")
        self.stage({"first.txt": "first commit contents\n"}, "A  first.txt\n")
        self.stage({}, "?? first.txt\n")
        self.terminal.quit()

    def test_empty_file_and_binary_file_round_trip(self):
        self.baseline({"baseline.txt": "baseline\n"})
        self.repo.write("a-empty.txt", b"")
        self.repo.write("b-binary.dat", b"\0binary\xffdata\n")
        self.start()
        self.terminal.send("\t")
        self.terminal.see("Files [FOCUSED]")
        self.stage({"baseline.txt": "baseline\n", "a-empty.txt": b""},
                   "A  a-empty.txt\n?? b-binary.dat\n")
        self.stage({"baseline.txt": "baseline\n"}, "?? a-empty.txt\n?? b-binary.dat\n")
        self.terminal.send("n")
        self.terminal.see("b-binary.dat")
        self.stage({"baseline.txt": "baseline\n", "b-binary.dat": b"\0binary\xffdata\n"},
                   "A  b-binary.dat\n?? a-empty.txt\n")
        self.stage({"baseline.txt": "baseline\n"}, "?? a-empty.txt\n?? b-binary.dat\n")
        self.terminal.quit()

    def test_ignored_file_is_not_staged(self):
        self.baseline({".gitignore": "ignored.txt\n", "a.txt": "before\n"})
        self.repo.write("ignored.txt", "must stay outside index\n")
        self.repo.write("a.txt", "after\n")
        self.start()
        self.stage({".gitignore": "ignored.txt\n", "a.txt": "after\n"}, "M  a.txt\n")
        self.assertNotIn("ignored.txt", self.terminal.text)
        self.terminal.quit()

    def test_staged_changes_absent_from_worktree_can_be_unstaged(self):
        self.baseline({"a.txt": "original\n"})
        self.repo.write("a.txt", "index only\n")
        self.repo.git("add", "a.txt")
        self.repo.write("a.txt", "original\n")
        self.start()
        self.terminal.see("index only")
        self.stage({"a.txt": "original\n"}, "")
        self.terminal.see("No changes")
        self.terminal.quit()

    def test_watcher_clears_diff_when_last_change_is_reverted(self):
        self.baseline({"a.txt": "original\n"})
        self.repo.write("a.txt", "changed\n")
        self.start()
        self.repo.write("a.txt", "original\n")
        self.worktree = self.repo.worktree()
        self.terminal.see("No changes")
        self.assert_state({"a.txt": "original\n"}, "")
        self.terminal.quit()

    def test_watcher_keeps_final_edit_in_a_burst(self):
        self.baseline({"a.txt": "original\n"})
        self.start()
        for number in range(20):
            self.repo.write("a.txt", "burst {}\n".format(number))
        self.worktree = self.repo.worktree()
        self.terminal.see("+ burst 19")
        self.stage({"a.txt": "burst 19\n"}, "M  a.txt\n")
        self.terminal.quit()

    def test_partially_staged_hunk_does_not_stage_a_second_hunk(self):
        original = "".join("line {:02}\n".format(i) for i in range(30))
        partial = original.replace("line 02\n", "first edit\n")
        first = partial.replace("line 03\n", "second edit\n")
        both = first.replace("line 25\n", "other hunk\n")
        self.baseline({"a.txt": original})
        self.repo.write("a.txt", partial)
        self.repo.git("add", "a.txt")
        self.repo.write("a.txt", both)
        self.start()
        self.stage({"a.txt": first}, "MM a.txt\n")
        self.terminal.quit()

    def test_deletion_line_at_first_and_last_positions(self):
        self.baseline({"a.txt": "first\nmiddle\nlast\n"})
        self.repo.write("a.txt", "middle\n")
        self.start()
        self.select_line("- last")
        self.stage({"a.txt": "first\nmiddle\n"}, "MM a.txt\n")
        self.stage({"a.txt": "first\nmiddle\nlast\n"}, " M a.txt\n")
        self.terminal.send("k")
        self.terminal.wait(
            lambda: any("\u25ba" in line and "- first" in line for line in self.terminal.screen.display),
            "first deletion selected",
        )
        self.stage({"a.txt": "middle\nlast\n"}, "MM a.txt\n")
        self.stage({"a.txt": "first\nmiddle\nlast\n"}, " M a.txt\n")
        self.terminal.quit()

    def test_special_path_single_line_round_trip(self):
        filename = 'nested directory/caf\u00e9 "notes".txt'
        self.baseline({filename: "first\nlast\n"})
        self.repo.write(filename, "first\nadded\nlast\n")
        self.start()
        self.select_line("+ added")
        staged_status = self.repo.status().replace(" M ", "M  ", 1)
        unstaged_status = self.repo.status()
        self.stage({filename: "first\nadded\nlast\n"}, staged_status)
        self.stage({filename: "first\nlast\n"}, unstaged_status)
        self.terminal.quit()

    def test_single_added_line_at_start_and_end(self):
        self.baseline({"a.txt": "middle\n"})
        self.repo.write("a.txt", "first\nmiddle\nlast\n")
        self.start()
        self.select_line("+ last")
        self.stage({"a.txt": "middle\nlast\n"}, "MM a.txt\n")
        self.stage({"a.txt": "middle\n"}, " M a.txt\n")
        self.terminal.send("k")
        self.terminal.wait(
            lambda: any("\u25ba" in line and "+ first" in line for line in self.terminal.screen.display),
            "first addition selected",
        )
        self.stage({"a.txt": "first\nmiddle\n"}, "MM a.txt\n")
        self.stage({"a.txt": "middle\n"}, " M a.txt\n")
        self.terminal.quit()

    def test_single_added_line_without_final_newline(self):
        self.baseline({"a.txt": b"first\n"})
        self.repo.write("a.txt", b"first\nlast")
        self.start()
        self.select_line("+ last")
        self.stage({"a.txt": b"first\nlast"}, "M  a.txt\n")
        self.stage({"a.txt": b"first\n"}, " M a.txt\n")
        self.terminal.quit()

    def test_index_only_single_line_unstage(self):
        self.baseline({"a.txt": "original\n"})
        self.repo.write("a.txt", "original\nindex only\n")
        self.repo.git("add", "a.txt")
        self.repo.write("a.txt", "original\n")
        self.start()
        self.select_line("+ index only")
        self.stage({"a.txt": "original\n"}, "")
        self.terminal.see("No changes")
        self.terminal.quit()

    def test_rename_is_staged_as_addition_and_deletion(self):
        self.baseline({"old.txt": "rename contents\n"})
        (self.repo.path / "old.txt").rename(self.repo.path / "new.txt")
        self.start()
        self.terminal.send("\t")
        self.terminal.see("Files [FOCUSED]")
        self.stage({"old.txt": "rename contents\n", "new.txt": "rename contents\n"},
                   "A  new.txt\n D old.txt\n")
        self.terminal.send("n")
        self.terminal.see("old.txt")
        self.stage({"new.txt": "rename contents\n"}, "R  old.txt -> new.txt\n")
        self.terminal.quit()

    def test_uppercase_stage_key(self):
        self.baseline({"a.txt": "before\n"})
        self.repo.write("a.txt", "after\n")
        self.start()
        self.terminal.send("S")
        self.assert_state({"a.txt": "after\n"}, "M  a.txt\n")
        self.terminal.quit("Q")

    def test_buffered_stream_receives_and_stages_an_external_edit(self):
        self.baseline({"a.txt": "original\n"})
        self.start()
        self.terminal.send("m")
        self.terminal.see("STREAMING (Buffered)")
        self.repo.write("a.txt", "streamed edit\n")
        self.worktree = self.repo.worktree()
        self.terminal.see("+ streamed edit")
        self.stage({"a.txt": "streamed edit\n"}, "M  a.txt\n")
        self.terminal.send("\x1b")
        self.terminal.see("Mode: VIEW")
        self.stage({"a.txt": "original\n"}, " M a.txt\n")
        self.terminal.quit()

    def test_mode_cycle_returns_to_view_without_repository_mutations(self):
        self.baseline({"a.txt": "before\n"})
        self.repo.write("a.txt", "after\n")
        self.start()
        for label in (
            "STREAMING (Buffered)", "STREAMING (Auto - Fast)",
            "STREAMING (Auto - Medium)", "STREAMING (Auto - Slow)", "Mode: VIEW",
        ):
            self.terminal.send("m")
            self.terminal.see(label)
        self.assert_state({"a.txt": "before\n"}, " M a.txt\n")
        self.terminal.quit()

    def test_arrow_keys_select_file_for_staging(self):
        self.baseline({"a.txt": "a\n", "b.txt": "b\n"})
        self.repo.write("a.txt", "a changed\n")
        self.repo.write("b.txt", "b changed\n")
        self.start()
        self.terminal.send("\t")
        self.terminal.see("Files [FOCUSED]")
        self.terminal.send("\x1b[B")
        self.terminal.see("b.txt (Hunk")
        self.stage({"a.txt": "a\n", "b.txt": "b changed\n"}, " M a.txt\nM  b.txt\n")
        self.terminal.send("\x1b[A")
        self.terminal.see("a.txt (Hunk")
        self.stage({"a.txt": "a changed\n", "b.txt": "b changed\n"}, "M  a.txt\nM  b.txt\n")
        self.terminal.quit()

    def test_non_repository_startup_fails_without_entering_raw_mode(self):
        terminal = Terminal(
            self.repo, args=("--no-splash", "--repo", str(Path(self.temp.name) / "home")),
        )
        self.terminals.append(terminal)
        terminal.wait(
            lambda: terminal.exit_code is not None and "Error:" in terminal.text,
            "explicit non-repository error and process exit", allow_exit=True,
        )
        self.assertNotEqual(terminal.exit_code, 0)
        self.assertNotIn(b"\x1b[?1049h", terminal.transcript)
        self.assertEqual(self.repo.index(), {})

    def test_index_lock_error_is_visible_and_retry_succeeds(self):
        self.baseline({"a.txt": "before\n"})
        self.repo.write("a.txt", "after\n")
        self.start()
        (self.repo.path / ".git/index.lock").write_text("E2E lock\n")
        self.terminal.send("s")
        self.terminal.see("Staging failed:")
        self.assertEqual(self.repo.git("show", ":a.txt"), b"before\n")
        self.assertEqual(self.repo.worktree(), self.worktree)
        (self.repo.path / ".git/index.lock").unlink()
        self.stage({"a.txt": "after\n"}, "M  a.txt\n")
        self.terminal.quit()

    def editor(self, cancel=False):
        script = Path(self.temp.name) / "editor.py"
        script.write_text(
            "import pathlib, sys\n"
            + ("sys.exit(1)\n" if cancel else
               "pathlib.Path(sys.argv[1]).write_text('E2E tool commit\\n')\n")
        )
        self.repo.env["GIT_EDITOR"] = "{} {}".format(
            shlex.quote(sys.executable), shlex.quote(str(script)),
        )

    def test_commit_handoff_commits_only_staged_changes(self):
        self.baseline({"a.txt": "before\n", "b.txt": "leave\n"})
        self.repo.write("a.txt", "after\n")
        self.repo.write("b.txt", "leave unstaged\n")
        self.editor()
        self.start()
        self.stage({"a.txt": "after\n", "b.txt": "leave\n"}, "M  a.txt\n M b.txt\n")
        self.terminal.send("c")
        self.terminal.wait(
            lambda: self.repo.git("rev-parse", "HEAD") != self.head,
            "commit created by the tool",
        )
        self.terminal.see("b.txt (Hunk")
        self.assertEqual(self.repo.git("log", "-1", "--format=%s"), b"E2E tool commit\n")
        self.assertEqual(self.repo.git("show", "HEAD:a.txt"), b"after\n")
        self.assertEqual(self.repo.git("show", "HEAD:b.txt"), b"leave\n")
        self.assertEqual(self.repo.status(), " M b.txt\n")
        self.assertEqual(self.repo.worktree(), self.worktree)
        self.terminal.quit()

    def test_cancelled_editor_restores_tui_without_modifying_head(self):
        self.baseline({"a.txt": "before\n"})
        self.repo.write("a.txt", "after\n")
        self.editor(cancel=True)
        self.start()
        self.stage({"a.txt": "after\n"}, "M  a.txt\n")
        self.terminal.send("c")
        self.terminal.wait(
            lambda: self.terminal.transcript.count(b"\x1b[?1049h") >= 2,
            "TUI restored after commit terminal handoff",
        )
        self.terminal.barrier()
        self.assert_state({"a.txt": "after\n"}, "M  a.txt\n")
        self.stage({"a.txt": "before\n"}, " M a.txt\n")
        self.terminal.quit()


if __name__ == "__main__":
    unittest.main()
