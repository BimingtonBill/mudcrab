"""tools/hive.py: notes, dedupe, guards, brief, live pages and the edit-hook collision push."""
import contextlib
import io
import json
import os
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest import mock

from tools import hive


def note_args(**kw):
    base = dict(kind="gotcha", area="engine-streaming", claim="Streaming settles after the cell loads", body="", evidence="",
                files=None, touched=False, tier=2, supersedes=None, force=False, new=False, session="aaaaaaaa-1111")
    base.update(kw)
    return Namespace(**base)


class HiveCase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.hive_root = self.base / "hive"
        self.work = self.base / "work"
        (self.work / ".git").mkdir(parents=True)
        (self.work / ".git" / "HEAD").write_text("ref: refs/heads/plus-test\n")
        env = mock.patch.dict(os.environ, {"HIVE_ROOT": str(self.hive_root), "HIVE_SCOPE": str(self.base),
                                           "CLAUDE_CODE_SESSION_ID": "aaaaaaaa-1111"})
        env.start()
        self.addCleanup(env.stop)
        hive.cmd_init(None)

    def run_cmd(self, fn, args):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            fn(args)
        return out.getvalue()

    def note(self, **kw):
        return self.run_cmd(hive.cmd_note, note_args(**kw)).strip()

    def fails(self, fn, args, code=2):
        err = io.StringIO()
        with contextlib.redirect_stderr(err), self.assertRaises(SystemExit) as ctx:
            fn(args)
        self.assertEqual(ctx.exception.code, code)
        return err.getvalue()


class Notes(HiveCase):
    def test_init_makes_store_and_repo(self):
        for name in ("OWNER.txt", "areas.toml", ".git"):
            self.assertTrue((self.hive_root / name).exists(), name)

    def test_note_is_written_and_committed(self):
        note_id = self.note()
        self.assertTrue((self.hive_root / "notes" / f"{note_id}.md").exists())
        log = hive.subprocess.run(["git", "-C", str(self.hive_root), "log", "--oneline"], capture_output=True, text=True).stdout
        self.assertIn(note_id, log)

    def test_rejects_unknown_area_kind_and_long_claim(self):
        self.assertIn("unknown area", self.fails(hive.cmd_note, note_args(area="nope")))
        self.assertIn("unknown kind", self.fails(hive.cmd_note, note_args(kind="nope")))
        self.fails(hive.cmd_note, note_args(claim="x" * 201))

    def test_rejects_secrets_and_big_bodies(self):
        self.assertIn("secret", self.fails(hive.cmd_note, note_args(claim="use ghp_" + "a" * 30 + " here")))
        self.fails(hive.cmd_note, note_args(body="line\n" * 20))
        self.fails(hive.cmd_note, note_args(evidence="password = hunter2hunter2hunter2"))
        self.fails(hive.cmd_note, note_args(body="bin\x00ary"))

    def test_dedupe_then_supersede(self):
        first = self.note()
        err = self.fails(hive.cmd_note, note_args(claim="Streaming settles after the cell loads fully"), code=3)
        self.assertIn(first, err)
        second = self.note(claim="Streaming settles after the cell loads fully", supersedes=first)
        _, meta, _ = hive.load_note(first)
        self.assertEqual(meta["status"], "superseded")
        self.assertEqual(meta["superseded_by"], second)

    def test_new_flag_skips_dedupe(self):
        self.note()
        self.note(claim="Streaming settles after the cell loads again", new=True)

    def test_verify_needs_evidence_and_reviewer(self):
        note_id = self.note()
        self.fails(hive.cmd_verify, Namespace(id=note_id, evidence="  ", reviewer="r"))
        with mock.patch.dict(os.environ, {"CLAUDE_CODE_SESSION_ID": "rrrrrrrr-9999"}):
            hive.cmd_verify(Namespace(id=note_id, evidence="commit abc1234", reviewer="claude-review"))
        _, meta, _ = hive.load_note(note_id)
        self.assertEqual(meta["status"], "verified")

    def test_wrong_marks_disputed(self):
        note_id = self.note()
        hive.feedback(note_id, "wrong", "bbbbbbbb")
        hive.set_status(note_id, "disputed")
        self.assertEqual(hive.load_note(note_id)[1]["status"], "disputed")


class Brief(HiveCase):
    def brief(self, **kw):
        args = Namespace(area=kw.get("area"), files=kw.get("files"))
        with mock.patch("os.getcwd", return_value=str(self.work)):
            return self.run_cmd(hive.cmd_brief, args)

    def test_ranks_by_area_and_hides_unrelated_retired_and_tier3(self):
        keep = self.note(claim="alpha streaming rule for cells")
        other = self.note(claim="beta shader rule", area="renderer-materials")
        gone = self.note(claim="gamma retired streaming rule")
        hive.set_status(gone, "retired")
        deep = self.note(claim="delta tier three disassembly detail", tier=3)
        out = self.brief(area=["engine-streaming"])
        self.assertIn(keep, out)
        for hidden in (other, gone, deep):
            self.assertNotIn(hidden, out)
        self.assertIn('untrusted="true"', out)

    def test_caps_drafts_and_marks_stale(self):
        target = self.work / "a.rs"
        target.write_text("one\n")
        with mock.patch("os.getcwd", return_value=str(self.work)):
            stale_id = self.note(claim="file specific streaming fact", files=["a.rs"])
        for n in range(6):
            self.note(claim=f"unique{n} streaming{n} fact{n} word{n}x", new=True)
        target.write_text("two\n")
        out = self.brief(area=["engine-streaming"], files=["a.rs"])
        self.assertEqual(out.count("[engine-streaming/gotcha/draft"), hive.BRIEF_DRAFTS)
        line = next(l for l in out.splitlines() if stale_id in l)
        self.assertIn("stale?", line)

    def test_crlf_does_not_make_a_note_stale(self):
        target = self.work / "b.rs"
        target.write_bytes(b"one\r\ntwo\r\n")
        with mock.patch("os.getcwd", return_value=str(self.work)):
            note_id = self.note(claim="crlf check", files=["b.rs"])
        target.write_bytes(b"one\ntwo\n")
        line = next(l for l in self.brief(area=["engine-streaming"]).splitlines() if note_id in l)
        self.assertNotIn("stale?", line)

    def test_stays_inside_budget(self):
        for n in range(30):
            self.note(claim=f"fact{n} " + "lorem ipsum dolor " * 8, body="body text " * 100, new=True)
        self.assertLessEqual(len(self.brief(area=["engine-streaming"])), hive.BRIEF_CHARS + 100)

    def test_search_finds_notes(self):
        self.note(claim="zebra settle rule")
        self.assertIn("zebra", self.run_cmd(hive.cmd_search, Namespace(terms=["zebra"])))


class LivePages(HiveCase):
    def hook(self, sid, name="a.rs", cwd=None, repo=None):
        repo = repo or self.work
        data = {"session_id": sid, "cwd": str(cwd or repo), "tool_input": {"file_path": str(repo / name)}}
        return hive.hook_edit(data)

    def join(self, sid, title):
        hive.cmd_join(Namespace(session=sid, title=title, task="T1", area=["engine-streaming"], goal="g"))

    def test_collision_is_pushed_once_with_a_fact(self):
        self.join("bbbbbbbb-2222", "Build: other thread")
        self.assertIsNone(self.hook("bbbbbbbb-2222"))
        message = self.hook("aaaaaaaa-1111")
        self.assertIn("Build: other thread", message)
        self.assertIn("a.rs", message)
        self.assertIsNone(self.hook("aaaaaaaa-1111"))

    def test_rate_limit_queues_second_cause(self):
        self.hook("bbbbbbbb-2222", "a.rs")
        self.hook("bbbbbbbb-2222", "b.rs")
        self.assertIn("a.rs", self.hook("aaaaaaaa-1111", "a.rs"))
        self.assertIsNone(self.hook("aaaaaaaa-1111", "b.rs"))
        state = hive.push_state("aaaaaaaa-1111")
        state["last"] -= hive.PUSH_GAP_SECS + 1
        hive.write_atomic(self.hive_root / "state" / "push-aaaaaaaa-1111.json", json.dumps(state))
        self.assertIn("b.rs", self.hook("aaaaaaaa-1111", "c.rs"))

    def test_idle_threads_and_outside_paths_are_ignored(self):
        self.hook("bbbbbbbb-2222")
        page = hive.load_live("bbbbbbbb-2222")
        page["updated"] -= hive.IDLE_SECS + 5
        hive.write_atomic(hive.live_path("bbbbbbbb-2222"), json.dumps(page))
        self.assertIsNone(self.hook("aaaaaaaa-1111"))
        outside = tempfile.TemporaryDirectory()
        self.addCleanup(outside.cleanup)
        self.assertIsNone(hive.hook_edit({"session_id": "cccccccc", "cwd": outside.name,
                                          "tool_input": {"file_path": str(Path(outside.name) / "x.rs")}}))
        self.assertIsNone(hive.hook_edit({"session_id": "cccccccc", "cwd": str(self.hive_root),
                                          "tool_input": {"file_path": str(self.hive_root / "notes" / "x.md")}}))

    def test_hook_fails_open_on_bad_input(self):
        with mock.patch("sys.stdin", io.StringIO("not json")):
            self.run_cmd(hive.cmd_hook_edit, None)
        self.assertTrue((self.hive_root / "hook-errors.log").exists())

    def test_who_peek_and_now(self):
        self.join("bbbbbbbb-2222", "Fix: ktx2 padding")
        hive.cmd_now(Namespace(session="bbbbbbbb-2222", step="writing tests", decision="use smallest-first",
                               question=None, blocker=None, clear_blockers=False))
        self.assertIn("Fix: ktx2 padding", self.run_cmd(hive.cmd_who, None))
        peek = self.run_cmd(hive.cmd_peek, Namespace(thread="ktx2"))
        self.assertIn("writing tests", peek)
        self.assertLessEqual(len(peek.strip().splitlines()), 10)


if __name__ == "__main__":
    unittest.main()
