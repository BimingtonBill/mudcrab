"""Tests for scripts/cluster_repro_check.py on the base + derived overlay layout. Temp folders only."""

import contextlib
import importlib.util
import io
import json
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
import extension_hash as eh  # noqa: E402
from test_extension_hash import write_base, write_overlay  # noqa: E402

SPEC = importlib.util.spec_from_file_location("cluster_repro_check", ROOT / "scripts" / "cluster_repro_check.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


def run(*argv):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = CHECK.main([str(argument) for argument in argv])
    return code, out.getvalue()


def rewrite_metadata(overlay, change):
    path = overlay / eh.OVERLAY_METADATA
    metadata = json.loads(path.read_text(encoding="utf-8"))
    change(metadata)
    path.write_text(json.dumps(metadata, indent=2), encoding="utf-8")


class ReproCheckTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.first_base = Path(self.tmp.name) / "first"
        self.second_base = Path(self.tmp.name) / "second"
        write_base(self.first_base)
        write_base(self.second_base)
        self.first = write_overlay(self.first_base)
        self.second = write_overlay(self.second_base)

    def test_identical_overlays_match_by_folder_and_by_base(self):
        self.assertEqual(run(self.first, self.second), (0, "identical: 1 files and 1 tables\n"))
        self.assertEqual(run(self.first_base, self.second_base)[0], 0)
        self.assertEqual(run(self.first_base, self.second_base, "--overlay", "default")[0], 0)
        self.assertEqual(run(self.first_base, self.second, f"--overlay={self.first.name}")[0], 0)

    def test_run_specific_metadata_does_not_count(self):
        rewrite_metadata(self.second, lambda m: m.update(
            label="jobs4", created="2026-10-21T00:00:00Z", owner="another run",
            base_fingerprint="0" * 64))
        self.assertEqual(run(self.first, self.second)[0], 0)

    def test_row_order_and_page_layout_do_not_count(self):
        connection = sqlite3.connect(self.second / eh.OVERLAY_DATABASE)
        connection.execute("DELETE FROM cluster_meshes")
        connection.executemany("INSERT INTO cluster_meshes VALUES (?, ?)", [(1, "a"), (2, "b")])
        connection.commit()
        connection.execute("VACUUM")
        connection.close()
        self.assertEqual(run(self.first, self.second)[0], 0)

    def test_a_changed_row_names_the_table(self):
        connection = sqlite3.connect(self.second / eh.OVERLAY_DATABASE)
        connection.execute("UPDATE cluster_meshes SET name = 'z' WHERE id = 1")
        connection.commit()
        connection.close()
        code, out = run(self.first, self.second)
        self.assertEqual(code, 1)
        self.assertIn("table cluster_meshes", out)
        self.assertEqual(out.count("\n"), 2, out)  # the header and that one difference

    def test_a_table_only_one_overlay_has_is_a_difference_even_if_unlisted(self):
        # The comparison reads every table of clusters.db, so a stray or R-tree shadow table counts.
        connection = sqlite3.connect(self.second / eh.OVERLAY_DATABASE)
        connection.execute("CREATE TABLE cluster_chunks_spatial_node (nodeno INTEGER PRIMARY KEY, data BLOB)")
        connection.commit()
        connection.close()
        code, out = run(self.first, self.second)
        self.assertEqual(code, 1)
        self.assertIn("table cluster_chunks_spatial_node", out)

    def test_an_index_difference_is_reported(self):
        connection = sqlite3.connect(self.second / eh.OVERLAY_DATABASE)
        connection.execute("DROP INDEX idx_cluster_meshes_name")
        connection.commit()
        connection.close()
        code, out = run(self.first, self.second)
        self.assertEqual(code, 1)
        self.assertIn("table (indexes)", out)

    def test_payload_and_manifest_differences_are_reported(self):
        (self.first / "cluster-manifest.json").write_text("{}", encoding="utf-8")
        (self.second / "cluster-terrain-manifest.json").write_text("{}", encoding="utf-8")
        (self.second / "clusters" / "chunk0.bin").write_bytes(b"other")
        (self.second / "clusters" / "extra.bin").write_bytes(b"x")
        code, out = run(self.first, self.second)
        self.assertEqual(code, 1)
        for name in ("file cluster-manifest.json", "file cluster-terrain-manifest.json", "file clusters/chunk0.bin", "file clusters/extra.bin"):
            self.assertIn(name, out)

    def test_other_resolved_settings_are_a_difference(self):
        other_base = Path(self.tmp.name) / "other-settings"
        write_base(other_base)
        other = write_overlay(other_base, settings_id="bb22")
        code, out = run(self.first, other)
        self.assertEqual(code, 1, out)
        self.assertIn("overlay.json settings_id", out)
        self.assertIn("overlay.json build_settings", out)

    def test_another_base_core_identity_is_a_difference(self):
        other_base = Path(self.tmp.name) / "other-core"
        write_base(other_base, identity="core-b")
        other = write_overlay(other_base)
        code, out = run(self.first, other)
        self.assertEqual(code, 1, out)
        self.assertIn("overlay.json base_core_identity", out)
        self.assertIn("overlay.json extensions", out)  # built_from and the content hash follow the identity

    def test_recorded_failure_count_and_extension_hashes_are_compared(self):
        rewrite_metadata(self.second, lambda m: m.update(base_failure_count=4))
        code, out = run(self.first, self.second)
        self.assertEqual(code, 1, out)
        self.assertEqual([line for line in out.splitlines() if "overlay.json" in line][0].split(":")[0],
                         "  overlay.json base_failure_count")
        rewrite_metadata(self.second, lambda m: m.update(base_failure_count=0))
        rewrite_metadata(self.second, lambda m: m["extensions"][0].update(sha256="0" * 64))
        code, out = run(self.first, self.second)
        self.assertEqual(code, 1, out)
        self.assertIn("overlay.json extensions", out)

    def test_an_absent_failure_count_equals_zero(self):
        rewrite_metadata(self.first, lambda m: m.update(base_failure_count=0))
        self.assertEqual(run(self.first, self.second)[0], 0)

    def test_base_with_several_overlays_needs_a_selector(self):
        write_overlay(self.first_base, label="dense", settings_id="bb22")
        code, out = run(self.first_base, self.second)
        self.assertEqual(code, 2)
        self.assertIn("2 cluster overlays", out)
        self.assertEqual(run(self.first_base, self.second, "--overlay", "default")[0], 0)

    def test_a_named_overlay_ignores_an_unrelated_unreadable_one(self):
        broken = write_overlay(self.first_base, label="broken", settings_id="bb22")
        (broken / eh.OVERLAY_METADATA).write_text("{", encoding="utf-8")
        self.assertEqual(run(self.first_base, self.second_base, "--overlay", "default")[0], 0)
        code, out = run(self.first_base, self.second_base)
        self.assertEqual(code, 2)
        self.assertIn("is unreadable", out)

    def test_a_base_path_with_spaces_and_a_hash_sign_opens(self):
        odd = Path(self.tmp.name) / "my base #1 100%"
        write_base(odd)
        write_overlay(odd)
        self.assertEqual(run(self.first, odd)[0], 0)

    def test_unusable_inputs_exit_two(self):
        self.assertEqual(run(self.first)[0], 2)
        self.assertEqual(run(self.first, self.second, self.second)[0], 2)
        self.assertEqual(run(self.first, self.second, "--overlay")[0], 2)
        empty = Path(self.tmp.name) / "empty"
        write_base(empty)
        self.assertEqual(run(self.first, empty)[0], 2)
        (self.second / eh.OVERLAY_DATABASE).write_bytes(b"not a SQLite database")
        self.assertEqual(run(self.first, self.second)[0], 2)


if __name__ == "__main__":
    unittest.main()
