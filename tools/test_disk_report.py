"""Tests for disk_report.py. Only temp folders are created (and removed)."""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from datetime import date, datetime, timezone
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import disk_report as dr  # noqa: E402


def write(path, text="", size=0):
    """Create a file with text, or `size` zero bytes."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as handle:
        handle.write(text.encode() if not size else b"\0" * size)


def owner_text(**overrides):
    """OWNER.txt content with sensible defaults."""
    values = {"owner": "Test Dev", "class": "A", "keep-until": "while-open", "links": "no"}
    values.update(overrides)
    return "\n".join(f"{k}: {v}" for k, v in values.items()) + "\n"


class TempCase(unittest.TestCase):
    """Base class giving each test a temp root that is removed afterwards."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="disk_report_test_")
        self.addCleanup(shutil.rmtree, self.root, True)

    def entry_dir(self, name, owner_file=None):
        path = os.path.join(self.root, name)
        os.makedirs(path)
        if owner_file is not None:
            write(os.path.join(path, "OWNER.txt"), owner_file)
        return path


class ParseOwnerTests(TempCase):
    def test_good_file_with_inline_comments(self):
        text = (
            "# comment\n"
            "owner: Phase 2 Dev\n"
            "purpose: build folder\n"
            "class: a            # A = rebuildable\n"
            "keep-until: pr:125  # or date:2026-10-09\n"
            "links: no           # yes if junctions\n"
            "created: 2026-10-02\n"
            "mystery: ignored\n"
        )
        info, problem = dr.parse_owner_text(text)
        self.assertEqual(problem, "")
        self.assertEqual(info["owner"], "Phase 2 Dev")
        self.assertEqual(info["class"], "A")
        self.assertEqual(info["keep-until"], "pr:125")
        self.assertFalse(info["links"])

    def test_defaults(self):
        info, _ = dr.parse_owner_text("owner: X\nclass: B\n")
        self.assertEqual(info["keep-until"], "while-open")
        self.assertFalse(info["links"])

    def test_links_yes(self):
        info, _ = dr.parse_owner_text("owner: X\nclass: B\nlinks: Yes\n")
        self.assertTrue(info["links"])

    def test_garbage_and_missing_fields(self):
        for text in ["", "\x00\x01 garbage", "owner:\nclass: A", "owner: X", "owner: X\nclass: Z",
                     "owner: X\nclass: A\nlinks: maybe"]:
            info, problem = dr.parse_owner_text(text)
            self.assertIsNone(info, text)
            self.assertTrue(problem)

    def test_missing_file(self):
        folder = self.entry_dir("nofile")
        info, problem = dr.read_owner(folder)
        self.assertIsNone(info)
        self.assertIn("no OWNER.txt", problem)

    def test_read_from_disk(self):
        folder = self.entry_dir("ok", owner_text())
        info, _ = dr.read_owner(folder)
        self.assertEqual(info["owner"], "Test Dev")

    def test_template_parses(self):
        info, problem = dr.parse_owner_text(dr.TEMPLATE)
        self.assertEqual(problem, "")
        self.assertEqual(info["class"], "A")
        self.assertEqual(info["keep-until"], "pr:125")


class ExpiryTests(unittest.TestCase):
    def setUp(self):
        self.opts = dr.Options(
            grace_days=3,
            now=datetime(2026, 10, 10, 12, 0, tzinfo=timezone.utc),
            today=date(2026, 10, 10),
        )

    def info(self, keep, cls="A"):
        return {"keep-until": keep, "class": cls}

    def test_while_open_never_expires(self):
        self.assertFalse(dr.check_expiry(self.info("while-open"), self.opts)[0])

    def test_date(self):
        self.assertTrue(dr.check_expiry(self.info("date:2026-10-09"), self.opts)[0])
        self.assertFalse(dr.check_expiry(self.info("date:2026-10-10"), self.opts)[0])
        self.assertFalse(dr.check_expiry(self.info("date:2026-10-11"), self.opts)[0])

    def test_bad_values_do_not_expire(self):
        self.assertFalse(dr.check_expiry(self.info("date:nonsense"), self.opts)[0])
        self.assertFalse(dr.check_expiry(self.info("whenever"), self.opts)[0])

    def pr(self, state, cls, merged=None, closed=None, keep="pr:125"):
        data = {"state": state, "mergedAt": merged, "closedAt": closed}
        return dr.check_expiry(self.info(keep, cls), self.opts, lambda n: data)

    def test_pr_open_does_not_expire(self):
        self.assertFalse(self.pr("OPEN", "A")[0])

    def test_pr_class_a_expires_when_merged_or_closed(self):
        self.assertTrue(self.pr("MERGED", "A", merged="2026-10-10T08:00:00Z")[0])
        self.assertTrue(self.pr("CLOSED", "A", closed="2026-10-10T08:00:00Z")[0])

    def test_pr_class_b_waits_grace_days(self):
        self.assertFalse(self.pr("MERGED", "B", merged="2026-10-08T00:00:00Z")[0])
        self.assertTrue(self.pr("MERGED", "B", merged="2026-10-07T00:00:00Z")[0])
        self.assertFalse(self.pr("CLOSED", "B", closed="2026-10-09T00:00:00Z")[0])
        self.assertTrue(self.pr("CLOSED", "B", closed="2026-10-01T00:00:00Z")[0])

    def test_pr_class_b_without_date_does_not_expire(self):
        self.assertFalse(self.pr("MERGED", "B")[0])

    def test_gh_failure_is_unknown_not_expired(self):
        expired, note = dr.check_expiry(self.info("pr:7"), self.opts, lambda n: None)
        self.assertFalse(expired)
        self.assertIn("unknown", note)

    def test_fetch_pr_returns_none_when_gh_fails(self):
        with mock.patch.object(dr.subprocess, "run", side_effect=OSError("no gh")):
            self.assertIsNone(dr.fetch_pr(1))
        failed = subprocess.CompletedProcess([], 1, "", "boom")
        with mock.patch.object(dr.subprocess, "run", return_value=failed):
            self.assertIsNone(dr.fetch_pr(1))

    def test_fetch_pr_parses_json_and_is_read_only_view(self):
        done = subprocess.CompletedProcess([], 0, '{"state": "OPEN"}', "")
        with mock.patch.object(dr.subprocess, "run", return_value=done) as run:
            self.assertEqual(dr.fetch_pr(5), {"state": "OPEN"})
        cmd = run.call_args[0][0]
        self.assertEqual(cmd[:4], ["gh", "pr", "view", "5"])


class WalkTests(TempCase):
    def test_plain_size(self):
        folder = self.entry_dir("plain")
        write(os.path.join(folder, "a.bin"), size=1000)
        write(os.path.join(folder, "sub", "b.bin"), size=2000)
        result = dr.walk_tree(folder, 30)
        self.assertEqual(result.size, 3000)
        self.assertEqual(result.files, 2)
        self.assertIsNone(result.links_found)
        self.assertFalse(result.timed_out)

    def test_hard_link_counted_once(self):
        folder = self.entry_dir("hard")
        first = os.path.join(folder, "a.bin")
        write(first, size=5000)
        try:
            os.link(first, os.path.join(folder, "b.bin"))
        except OSError as exc:
            self.skipTest(f"cannot create hard link: {exc}")
        result = dr.walk_tree(folder, 30)
        self.assertEqual(result.size, 5000)
        self.assertEqual(result.hardlinked_files, 2)

    def test_hard_link_note_in_entry(self):
        folder = self.entry_dir("hardentry", owner_text(**{"keep-until": "date:2000-01-01"}))
        first = os.path.join(folder, "a.bin")
        write(first, size=5000)
        try:
            os.link(first, os.path.join(folder, "b.bin"))
        except OSError as exc:
            self.skipTest(f"cannot create hard link: {exc}")
        entry = dr.process_entry(folder, dr.Options(), None)
        self.assertEqual(entry.section, dr.CAN_GO)
        self.assertIn("2 hard-linked files: deleting frees less than shown", entry.reason)

    def test_timeout_reports_partial(self):
        folder = self.entry_dir("slow")
        for i in range(5):
            write(os.path.join(folder, f"f{i}.bin"), size=10)
        ticks = iter(range(1000))
        result = dr.walk_tree(folder, 2.5, clock=lambda: next(ticks))
        self.assertTrue(result.timed_out)
        self.assertLess(result.files, 5)

    def test_timed_out_entry_size_text(self):
        entry = dr.Entry("x", dr.KEEP, size=3 * dr.GB, timed_out=True)
        self.assertEqual(dr.format_size(entry), ">= 3.0 GB (timed out)")
        entry.timed_out = False
        self.assertEqual(dr.format_size(entry), "3.0 GB")

    def make_link_entry(self, kind):
        """Entry folder with a link to a sibling target holding a big file."""
        target = self.entry_dir("target_outside")
        write(os.path.join(target, "big.bin"), size=4_000_000)
        folder = self.entry_dir("linked", owner_text(**{"keep-until": "date:2000-01-01"}))
        write(os.path.join(folder, "small.bin"), size=100)
        link = os.path.join(folder, "thelink")
        if kind == "junction":
            done = subprocess.run(["cmd", "/c", "mklink", "/J", link, target],
                                  capture_output=True)  # fmt: skip
            if done.returncode != 0:
                self.skipTest("cannot create junction")
        else:
            try:
                os.symlink(target, link, target_is_directory=True)
            except (OSError, NotImplementedError) as exc:
                self.skipTest(f"cannot create symlink: {exc}")
        self.addCleanup(self.remove_link, link)
        return folder

    @staticmethod
    def remove_link(link):
        """Remove only the link itself (never the target's contents)."""
        try:
            os.rmdir(link)
        except OSError:
            pass

    def check_link_kind(self, kind):
        folder = self.make_link_entry(kind)
        result = dr.walk_tree(folder, 30)
        self.assertEqual(result.links_found, "thelink")
        self.assertLess(result.size, 4_000_000)
        entry = dr.process_entry(folder, dr.Options(), None)
        self.assertEqual(entry.section, dr.CLASS_C)
        self.assertIn("contains links -> treat as class C", entry.reason)
        self.assertIsNone(entry.size)

    def test_junction_is_reported_not_entered(self):
        self.check_link_kind("junction")

    def test_symlink_is_reported_not_entered(self):
        self.check_link_kind("symlink")

    def test_entry_folder_that_is_a_junction_is_class_c(self):
        target = self.entry_dir("realtarget", owner_text())
        link = os.path.join(self.root, "junc")
        done = subprocess.run(["cmd", "/c", "mklink", "/J", link, target], capture_output=True)
        if done.returncode != 0:
            self.skipTest("cannot create junction")
        self.addCleanup(self.remove_link, link)
        with mock.patch.object(dr, "walk_tree") as walker:
            entry = dr.process_entry(link, dr.Options(), None)
        walker.assert_not_called()
        self.assertEqual(entry.section, dr.CLASS_C)


class SectionTests(TempCase):
    def test_unknown_owner_not_walked(self):
        folder = self.entry_dir("mystery")
        garbage = self.entry_dir("garbage", "not an owner file")
        with mock.patch.object(dr, "walk_tree") as walker:
            for path in (folder, garbage):
                entry = dr.process_entry(path, dr.Options(), None)
                self.assertEqual(entry.section, dr.UNKNOWN)
                self.assertIsNone(entry.size)
        walker.assert_not_called()

    def test_class_c_and_links_yes_not_walked(self):
        c_folder = self.entry_dir("convdata", owner_text(**{"class": "C"}))
        links_folder = self.entry_dir("withlinks", owner_text(links="yes"))
        with mock.patch.object(dr, "walk_tree") as walker:
            for path in (c_folder, links_folder):
                entry = dr.process_entry(path, dr.Options(), None)
                self.assertEqual(entry.section, dr.CLASS_C)
        walker.assert_not_called()

    def test_never_touch_names_not_walked(self):
        names = ["SkyrimConverted", "SkyrimConverted-upstream", "skyrimconverted2", "Bethini Pie (x)",
                 "Modlists", "Wabbajack", "OpenSkyrimProject"]
        for name in names:
            folder = self.entry_dir(name, owner_text(**{"keep-until": "date:2000-01-01"}))
            write(os.path.join(folder, "big.bin"), size=1000)
        with mock.patch.object(dr, "walk_tree") as walker, \
                mock.patch.object(dr, "read_owner") as reader, \
                mock.patch.object(dr, "run_git") as git:
            entries = dr.build_report([self.root], dr.Options())
        walker.assert_not_called()
        reader.assert_not_called()
        git.assert_not_called()
        self.assertEqual(len(entries), len(names))
        self.assertTrue(all(e.section == dr.NEVER for e in entries))

    def test_expired_clean_folder_can_go_and_open_one_is_kept(self):
        old = self.entry_dir("old", owner_text(**{"keep-until": "date:2000-01-01"}))
        write(os.path.join(old, "a.bin"), size=10)
        new = self.entry_dir("new", owner_text())
        write(os.path.join(new, "a.bin"), size=10)
        entries = {os.path.basename(e.path): e for e in dr.build_report([self.root], dr.Options())}
        self.assertEqual(entries["old"].section, dr.CAN_GO)
        self.assertEqual(entries["new"].section, dr.KEEP)

    def test_wt_children_are_entries_not_the_parent(self):
        parent = self.entry_dir("wt")
        for name in ("one", "two"):
            os.makedirs(os.path.join(parent, name))
        paths = dr.discover([self.root])
        names = sorted(os.path.basename(p) for p in paths)
        self.assertEqual(names, ["one", "two"])

    def test_dirty_worktree_never_can_go(self):
        folder = self.entry_dir("wtree", owner_text(**{"keep-until": "date:2000-01-01"}))
        write(os.path.join(folder, ".git"), "gitdir: elsewhere\n")
        with mock.patch.object(dr, "worktree_info", return_value={"branch": "b", "dirty": 3}):
            entry = dr.process_entry(folder, dr.Options(), None)
        self.assertEqual(entry.section, dr.KEEP)
        self.assertIn("has uncommitted changes", entry.reason)
        self.assertIn("owner must commit or discard first", entry.reason)

    def test_clean_worktree_can_go_and_git_failure_blocks(self):
        folder = self.entry_dir("wtree2", owner_text(**{"keep-until": "date:2000-01-01"}))
        write(os.path.join(folder, ".git"), "gitdir: elsewhere\n")
        with mock.patch.object(dr, "worktree_info", return_value={"branch": "feat", "dirty": 0}):
            entry = dr.process_entry(folder, dr.Options(), None)
        self.assertEqual(entry.section, dr.CAN_GO)
        self.assertIn("branch feat", entry.reason)
        with mock.patch.object(dr, "worktree_info", return_value={"branch": "?", "dirty": None}):
            entry = dr.process_entry(folder, dr.Options(), None)
        self.assertEqual(entry.section, dr.KEEP)

    def test_pr_unknown_stays_in_keep(self):
        folder = self.entry_dir("prfolder", owner_text(**{"keep-until": "pr:9"}))
        with mock.patch.object(dr, "fetch_pr", return_value=None):
            entries = dr.build_report([self.root], dr.Options())
        self.assertEqual(entries[0].section, dr.KEEP)
        self.assertIn("unknown", entries[0].reason)
        self.assertTrue(folder)


class RenderTests(unittest.TestCase):
    def test_sections_order_footer_and_sort(self):
        entries = [
            dr.Entry("C:/a", dr.KEEP, "o", "A", "while-open", 1 * dr.GB, reason="r"),
            dr.Entry("C:/b", dr.KEEP, "o", "A", "while-open", 5 * dr.GB, reason="r"),
            dr.Entry("C:/c", dr.CAN_GO, "o", "B", "date:x", 2 * dr.GB, reason="done"),
            dr.Entry("C:/d", dr.UNKNOWN, reason="unknown owner"),
            dr.Entry("C:/e", dr.CLASS_C, "o", "C", reason="x"),
            dr.Entry("C:/SkyrimConverted", dr.NEVER),
        ]
        text = dr.render_report(entries, datetime(2026, 10, 2, 9, 30))
        titles = ["## Can go", "## Keep", "## Who owns this?", "## Class C", "## Never-touch"]
        positions = [text.index(t) for t in titles]
        self.assertEqual(positions, sorted(positions))
        self.assertLess(text.index("C:/b"), text.index("C:/a"))
        self.assertIn("Generated 2026-10-02 09:30", text)
        self.assertIn("5.0 GB", text)
        self.assertTrue(text.rstrip().endswith("(never --force)."))
        self.assertIn("- C:/SkyrimConverted", text)

    def test_pipes_are_escaped(self):
        text = dr.render_report([dr.Entry("C:/a|b", dr.KEEP, reason="x|y")])
        self.assertIn("C:/a\\|b", text)

    def test_template_command(self):
        self.assertEqual(dr.main(["--template"]), 0)


class EndToEndTests(TempCase):
    def test_main_writes_out_file(self):
        folder = self.entry_dir("thing", owner_text())
        write(os.path.join(folder, "a.bin"), size=10)
        out = os.path.join(self.root, "report.md")
        with mock.patch("sys.stdout"):
            dr.main(["--root", self.root, "--time-limit", "5", "--out", out])
        with open(out, encoding="utf-8") as handle:
            text = handle.read()
        self.assertIn("## Keep", text)
        self.assertIn("thing", text)


if __name__ == "__main__":
    unittest.main()
