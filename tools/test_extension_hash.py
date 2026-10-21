"""Tests for extension_hash.py. Only temp folders are created (and removed).

`write_sample(root)` builds a fixture-sized data set with core output and two listed extensions;
the parity check's tests may import it.
"""

import hashlib
import json
import os
import sqlite3
import struct
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import extension_hash as eh  # noqa: E402

def write_glb(path: Path, document: dict, binary: bytes = b"\0\0\0\0") -> None:
    text = json.dumps(document, separators=(",", ":")).encode("utf-8")
    text += b" " * (-len(text) % 4)
    binary += b"\0" * (-len(binary) % 4)
    length = 12 + 8 + len(text) + 8 + len(binary)
    data = struct.pack("<III", eh.GLB_MAGIC, 2, length)
    data += struct.pack("<II", len(text), eh.GLB_JSON_CHUNK) + text
    data += struct.pack("<II", len(binary), 0x004E4942) + binary
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def write_sample(root: Path) -> dict:
    """A tiny data set: core `statics` and a core mesh, plus `mudcrab.door_links` and `mudcrab.material`."""
    root.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(root / eh.DATABASE)
    connection.executescript(
        """
        CREATE TABLE statics (id INTEGER PRIMARY KEY, editor_id TEXT, model_path TEXT);
        INSERT INTO statics VALUES (16, 'RockA', 'meshes/rock.glb');
        CREATE TABLE door_links (ref_id INTEGER PRIMARY KEY, destination_ref_id INTEGER,
                                 pos_x REAL, pos_y REAL, pos_z REAL, note TEXT, data BLOB);
        INSERT INTO door_links VALUES (2, 1, 0.5, -1.25, 1e-7, 'Whiterun ✓', x'00ff');
        INSERT INTO door_links VALUES (1, 2, 3.0, 4.0, NULL, NULL, NULL);
        CREATE INDEX idx_door_links_destination ON door_links(destination_ref_id);
        """
    )
    connection.commit()
    connection.close()
    write_glb(
        root / "meshes" / "rock.glb",
        {
            "asset": {"version": "2.0"},
            "extensionsUsed": ["MUDCRAB_material", "OPEN_SKYRIM_material"],
            "materials": [
                {"extensions": {"OPEN_SKYRIM_material": {"shaderFlags1": 1},
                                "MUDCRAB_material": {"uvScale": [1, 2.5], "blendSource": "SRC_ALPHA"}}},
                {"extensions": {"OPEN_SKYRIM_material": {"shaderFlags1": 2}}},
            ],
        },
    )
    write_glb(root / "meshes" / "plain.glb", {"asset": {"version": "2.0"}})
    extensions = [
        {"id": "mudcrab.door_links", "version": 1,
         "owns": {"tables": ["door_links"], "indexes": ["idx_door_links_destination"],
                  "files": [], "gltf_extensions": []},
         "built_from": "0" * 64, "stats": {"links": 2}},
        {"id": "mudcrab.material", "version": 1,
         "owns": {"tables": [], "indexes": [], "files": [], "gltf_extensions": ["MUDCRAB_material"]},
         "built_from": "0" * 64, "stats": {}},
    ]
    for entry in extensions:
        entry["sha256"] = eh.extension_hash(root, entry)
    manifest = {"schema_version": 26, "complete": True, "extensions": extensions}
    (root / eh.MANIFEST).write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    return manifest


class EncodingTests(unittest.TestCase):
    def test_sql_values(self):
        self.assertEqual(eh.encode_sql(None), "n")
        self.assertEqual(eh.encode_sql(-3), "i-3")
        self.assertEqual(eh.encode_sql(1.0), "f3ff0000000000000")
        self.assertEqual(eh.encode_sql("é"), "tc3a9")
        self.assertEqual(eh.encode_sql(b"\x00\xff"), "b00ff")

    def test_json_is_canonical(self):
        a = eh.encode_json({"b": 1, "a": [True, None, "x"]})
        b = eh.encode_json({"a": [True, None, "x"], "b": 1.0})
        self.assertEqual(a, b)
        self.assertEqual(a, "{t61:[true,null,t78],t62:f3ff0000000000000}")


class HashTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "set"
        self.manifest = write_sample(self.root)

    def tearDown(self):
        self.tmp.cleanup()

    def test_sample_checks_clean(self):
        self.assertEqual(eh.check(self.root), [])

    def test_row_order_does_not_matter(self):
        entry = self.manifest["extensions"][0]
        lines = eh.extension_lines(self.root, entry)
        rows = lines[lines.index("table door_links") + 1 : lines.index("table door_links") + 3]
        self.assertEqual(rows, sorted(rows))

    def test_row_change_is_caught(self):
        connection = sqlite3.connect(self.root / eh.DATABASE)
        connection.execute("UPDATE door_links SET pos_x = 0.5000001 WHERE ref_id = 2")
        connection.commit()
        connection.close()
        problems = eh.check(self.root)
        self.assertEqual(len(problems), 1)
        self.assertIn("mudcrab.door_links", problems[0])

    def test_column_change_is_caught(self):
        connection = sqlite3.connect(self.root / eh.DATABASE)
        connection.execute("ALTER TABLE door_links ADD COLUMN extra INTEGER")
        connection.commit()
        connection.close()
        self.assertEqual(len(eh.check(self.root)), 1)

    def test_core_change_is_not_an_extension_change(self):
        connection = sqlite3.connect(self.root / eh.DATABASE)
        connection.execute("UPDATE statics SET editor_id = 'RockB'")
        connection.commit()
        connection.close()
        self.assertEqual(eh.check(self.root), [])

    def test_gltf_extension_change_is_caught(self):
        document = eh.glb_json(self.root / "meshes" / "rock.glb")
        document["materials"][0]["extensions"]["MUDCRAB_material"]["uvScale"] = [1, 3]
        write_glb(self.root / "meshes" / "rock.glb", document)
        problems = eh.check(self.root)
        self.assertEqual(len(problems), 1)
        self.assertIn("mudcrab.material", problems[0])

    def test_core_gltf_extension_is_not_hashed(self):
        document = eh.glb_json(self.root / "meshes" / "rock.glb")
        document["materials"][1]["extensions"]["OPEN_SKYRIM_material"]["shaderFlags1"] = 7
        write_glb(self.root / "meshes" / "rock.glb", document)
        self.assertEqual(eh.check(self.root), [])

    def test_missing_table_is_a_problem(self):
        connection = sqlite3.connect(self.root / eh.DATABASE)
        connection.execute("DROP TABLE door_links")
        connection.commit()
        connection.close()
        problems = eh.check(self.root)
        self.assertTrue(any("door_links is missing" in p for p in problems))

    def test_shared_ownership_is_a_problem(self):
        extensions = self.manifest["extensions"]
        extensions[1]["owns"]["tables"] = ["door_links"]
        self.assertEqual(eh.ownership_problems(extensions),
                         ["table door_links is owned by mudcrab.door_links and mudcrab.material"])

    def test_missing_owned_directory_covers_nothing(self):
        self.assertEqual(eh.owned_files(self.root, ["nothing/"]), [])
        with self.assertRaises(LookupError):
            eh.owned_files(self.root, ["nothing.bin"])

    def test_owned_directory_expands(self):
        (self.root / "ext" / "a").mkdir(parents=True)
        (self.root / "ext" / "a" / "x.bin").write_bytes(b"1")
        (self.root / "ext" / "b.bin").write_bytes(b"2")
        self.assertEqual(eh.owned_files(self.root, ["ext/"]), ["ext/a/x.bin", "ext/b.bin"])

    def test_pinned_hashes(self):
        hashes = {e["id"]: e["sha256"] for e in self.manifest["extensions"]}
        self.assertEqual(hashes["mudcrab.door_links"], PINNED["mudcrab.door_links"])
        self.assertEqual(hashes["mudcrab.material"], PINNED["mudcrab.material"])


def write_base(root: Path, identity: str = "core-a") -> None:
    """A base with a core identity and a manifest that lists no extensions (as in the Rust fixtures)."""
    root.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(root / eh.DATABASE)
    connection.executescript(
        f"""
        CREATE TABLE lod_build (id INTEGER, build_identity TEXT);
        INSERT INTO lod_build VALUES (1, '{identity}');
        CREATE TABLE statics (id INTEGER);
        """
    )
    connection.commit()
    connection.close()
    (root / eh.MANIFEST).write_text('{"extensions":[]}', encoding="utf-8")


def write_overlay(base: Path, label: str = "default", settings_id: str = "aa11") -> Path:
    """A bound overlay with one table extension, one owned file and a payload folder."""
    settings = {"contract": "mudcrab.cluster-overlay.settings.v1", "builder_identity": "ab" * 32,
                "converter_version": "0.1.0", "triangle_threshold": int(settings_id, 16),
                "terrain": False, "include_trees": False}
    settings_id = hashlib.sha256(json.dumps(settings, sort_keys=True, separators=(",", ":"),
                                            ensure_ascii=False).encode()).hexdigest()[:16]
    folder = base / eh.DERIVED_DIR / f"{eh.OVERLAY_DIR_PREFIX}{settings_id}"
    folder.mkdir(parents=True)
    connection = sqlite3.connect(folder / eh.OVERLAY_DATABASE)
    connection.executescript(
        """
        CREATE TABLE cluster_meshes (id INTEGER PRIMARY KEY, name TEXT);
        INSERT INTO cluster_meshes VALUES (2, 'b'), (1, 'a');
        CREATE INDEX idx_cluster_meshes_name ON cluster_meshes(name);
        """
    )
    connection.commit()
    connection.close()
    (folder / "clusters").mkdir()
    (folder / "clusters" / "chunk0.bin").write_bytes(b"payload")
    identity = eh.base_core_identity(base)
    entry = {
        "id": "mudcrab.clusters", "version": 1,
        "owns": {"tables": ["cluster_meshes"], "indexes": ["idx_cluster_meshes_name"],
                 "files": ["clusters/"], "gltf_extensions": []},
        "built_from": identity, "stats": {},
    }
    entry["sha256"] = eh.extension_hash(folder, entry, database=folder / eh.OVERLAY_DATABASE)
    metadata = {
        "schema_version": eh.OVERLAY_SCHEMA_VERSION, "kind": eh.OVERLAY_KIND, "label": label,
        "settings_id": settings_id, "base_core_identity": identity,
        "base_fingerprint": eh.base_fingerprint(base), "builder_identity": "ab" * 32,
        "build_settings": settings,
        "created": "2026-10-20T00:00:00Z", "owner": "test fixture", "extensions": [entry],
    }
    (folder / eh.OVERLAY_METADATA).write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    return folder


class OverlayTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name) / "base"
        write_base(self.base)
        self.overlay = write_overlay(self.base)

    def tearDown(self):
        self.tmp.cleanup()

    def test_recorded_settings_must_hash_to_the_settings_id(self):
        path = self.overlay / eh.OVERLAY_METADATA
        metadata = json.loads(path.read_text())
        metadata["build_settings"]["triangle_threshold"] += 1
        path.write_text(json.dumps(metadata))
        with self.assertRaisesRegex(eh.OverlayError, "settings id"):
            eh.read_overlay_metadata(self.overlay)

    def test_rebuild_hint_preserves_the_recorded_variant_and_label(self):
        # The same fixtures and expected strings as the Rust tests in cluster_overlay.rs.
        metadata = eh.read_overlay_metadata(self.overlay)
        base = Path(r"D:\Mods\My Base")
        metadata.update(label="default", terrain_sources=None)
        metadata["build_settings"].update(triangle_threshold=1024, terrain=False, include_trees=False)
        self.assertEqual(
            eh.derive_command(base, metadata),
            r"converter derive-clusters 'D:\Mods\My Base' --triangle-threshold 1024 --label 'default' --replace",
        )
        metadata.update(label="dense", terrain_sources=r"E:\raw dds")
        metadata["build_settings"].update(triangle_threshold=512, terrain=True, include_trees=True)
        self.assertEqual(
            eh.derive_command(base, metadata),
            r"converter derive-clusters 'D:\Mods\My Base' --triangle-threshold 512 --label 'dense' --replace "
            r"--terrain --terrain-sources 'E:\raw dds' --include-trees",
        )
        self.assertNotIn("--jobs", eh.derive_command(base, metadata))
        # Terrain sources only mean something for a terrain build.
        metadata["build_settings"]["terrain"] = False
        self.assertNotIn("--terrain", eh.derive_command(base, metadata))

    def test_rebuild_command_arguments_are_powershell_single_quoted(self):
        metadata = eh.read_overlay_metadata(self.overlay)
        curly = chr(0x2019)
        metadata["label"] = f"a 'b' `c` \"d\" $e {curly}f"
        command = eh.derive_command(Path(r"D:\it's $base"), metadata)
        self.assertIn(r"'D:\it''s $base'", command)
        self.assertIn(f"--label 'a ''b'' `c` \"d\" $e {curly}{curly}f'", command)
        for quote in map(chr, range(0x2018, 0x201C)):
            self.assertEqual(eh.quote_argument(quote), f"'{quote}{quote}'")
        double = chr(0x201C) + chr(0x201D)
        self.assertEqual(eh.quote_argument(double), f"'{double}'")  # typographic double quotes stay literal
        self.assertEqual(eh.command_path(Path(r"\\?\D:\base")), r"D:\base")
        self.assertEqual(eh.command_path(Path(r"\\?\UNC\host\share\base")), r"\\host\share\base")

    def test_rebuild_hint_matches_rust_including_the_terrain_sources_note(self):
        metadata = eh.read_overlay_metadata(self.overlay)
        base = self.base
        self.assertEqual(
            eh.rebuild_command(base),
            f"rebuild it with converter derive-clusters {eh.quote_argument(base)}",
        )
        plain = eh.rebuild_command(base, metadata)
        self.assertEqual(plain, f"rebuild it with {eh.derive_command(base, metadata)}")
        metadata["build_settings"].update(terrain=True, include_trees=True)
        metadata["terrain_sources"] = r"E:\raw dds"
        hint = eh.rebuild_command(base, metadata)
        self.assertTrue(hint.startswith(f"rebuild it with {eh.derive_command(base, metadata)}; the terrain build"))
        self.assertTrue(hint.endswith(
            "which the overlay does not keep: restore or provide the matching raw DDS sources there, "
            "or pass their folder with --terrain-sources"
        ))
        self.assertIn(r"raw sources from E:\raw dds,", hint)
        # Without recorded sources the hint is the command alone.
        metadata["terrain_sources"] = None
        self.assertTrue(eh.rebuild_command(base, metadata).endswith("--terrain --include-trees"))

    def rewrite_metadata(self, change):
        path = self.overlay / eh.OVERLAY_METADATA
        metadata = json.loads(path.read_text(encoding="utf-8"))
        change(metadata)
        path.write_text(json.dumps(metadata), encoding="utf-8")

    def test_fingerprint_is_pinned_to_the_rust_algorithm(self):
        # Same fixture as the Rust test (identity core-a, manifest `{"extensions":[]}`); computed
        # here from the documented stream, not read back from a Rust run.
        stream = (
            "mudcrab-overlay-base-fingerprint-v1\ncore-a\n"
            + __import__("hashlib").sha256(b'{"extensions":[]}').hexdigest() + "\n"
        )
        self.assertEqual(eh.base_fingerprint(self.base), __import__("hashlib").sha256(stream.encode()).hexdigest())
        self.assertEqual(len(eh.base_fingerprint(self.base)), 64)

    def test_bound_overlay_checks_clean_and_base_is_not_touched(self):
        before = (self.base / eh.DATABASE).read_bytes(), (self.base / eh.MANIFEST).read_bytes()
        self.assertEqual(eh.check_overlay(self.base, self.overlay), [])
        self.assertEqual(before, ((self.base / eh.DATABASE).read_bytes(), (self.base / eh.MANIFEST).read_bytes()))

    def test_selectors(self):
        self.assertEqual(eh.resolve_overlay(self.base, ""), self.overlay)
        settings_id = self.overlay.name.removeprefix("clusters-")
        for selector in ("default", settings_id, self.overlay.name, str(self.overlay)):
            self.assertEqual(eh.resolve_overlay(self.base, selector), self.overlay)
        with self.assertRaisesRegex(eh.OverlayError, "no cluster overlay 'nope'"):
            eh.resolve_overlay(self.base, "nope")

    def test_two_overlays_need_a_selector_and_duplicate_labels_are_ambiguous(self):
        other = write_overlay(self.base, label="default", settings_id="bb22")
        with self.assertRaisesRegex(eh.OverlayError, "2 cluster overlays"):
            eh.resolve_overlay(self.base, "")
        with self.assertRaisesRegex(eh.OverlayError, "matches 2 overlays"):
            eh.resolve_overlay(self.base, "default")
        self.assertEqual(eh.resolve_overlay(self.base, other.name), other)

    def break_overlay(self, folder):
        (folder / eh.OVERLAY_METADATA).write_text("{not json", encoding="utf-8")

    def test_named_selector_ignores_unrelated_unreadable_overlays(self):
        broken = write_overlay(self.base, label="other", settings_id="bb22")
        self.break_overlay(broken)
        settings_id = self.overlay.name.removeprefix(eh.OVERLAY_DIR_PREFIX)
        for selector in ("default", settings_id, self.overlay.name, str(self.overlay)):
            self.assertEqual(eh.resolve_overlay(self.base, selector), self.overlay)
        self.assertEqual(eh.main(["--assets", str(self.base), "--overlay", "default"]), 0)

    def test_bare_selection_and_listing_still_refuse_an_unreadable_overlay(self):
        broken = write_overlay(self.base, label="other", settings_id="bb22")
        self.break_overlay(broken)
        for call in (lambda: eh.resolve_overlay(self.base, ""), lambda: eh.list_overlays(self.base)):
            with self.assertRaisesRegex(eh.OverlayError, f"{broken.name} is unreadable"):
                call()

    def test_selecting_an_unreadable_overlay_by_folder_name_reports_why(self):
        broken = write_overlay(self.base, label="other", settings_id="bb22")
        self.break_overlay(broken)
        with self.assertRaisesRegex(eh.OverlayError, f"{broken.name} is unreadable"):
            eh.resolve_overlay(self.base, broken.name)
        settings_id = broken.name.removeprefix(eh.OVERLAY_DIR_PREFIX)
        with self.assertRaisesRegex(eh.OverlayError, f"{broken.name} is unreadable"):
            eh.resolve_overlay(self.base, settings_id)
        # A label of an unreadable overlay cannot be known, so it is simply not found.
        with self.assertRaisesRegex(eh.OverlayError, "no cluster overlay 'other'"):
            eh.resolve_overlay(self.base, "other")

    def test_base_failure_count_is_optional_and_must_be_a_count(self):
        self.assertNotIn("base_failure_count", eh.read_overlay_metadata(self.overlay))
        self.rewrite_metadata(lambda m: m.update(base_failure_count=3))
        self.assertEqual(eh.read_overlay_metadata(self.overlay)["base_failure_count"], 3)
        for bad in (-1, True, "3", 1.5, None):
            self.rewrite_metadata(lambda m, bad=bad: m.update(base_failure_count=bad))
            with self.assertRaisesRegex(eh.OverlayError, "base_failure_count"):
                eh.read_overlay_metadata(self.overlay)

    def test_no_overlay_is_an_error(self):
        empty = Path(self.tmp.name) / "empty"
        write_base(empty)
        with self.assertRaisesRegex(eh.OverlayError, "no cluster overlay under"):
            eh.resolve_overlay(empty, "")

    def test_changed_manifest_or_identity_is_refused_with_the_rebuild_command(self):
        (self.base / eh.MANIFEST).write_text('{"extensions":[],"x":1}', encoding="utf-8")
        problems = eh.check_overlay(self.base, self.overlay)
        self.assertTrue(any("another conversion of this base" in p and "converter derive-clusters" in p
                            for p in problems), problems)
        write_base_identity = sqlite3.connect(self.base / eh.DATABASE)
        write_base_identity.execute("UPDATE lod_build SET build_identity = 'core-b'")
        write_base_identity.commit()
        write_base_identity.close()
        (self.base / eh.MANIFEST).write_text('{"extensions":[]}', encoding="utf-8")
        problems = eh.check_overlay(self.base, self.overlay)
        self.assertTrue(any("recorded core identity" in p for p in problems), problems)
        self.assertTrue(any("built_from" in p for p in problems), problems)

    def test_binding_refusal_precedes_reading_a_broken_cluster_database(self):
        (self.base / eh.MANIFEST).write_text('{"extensions":[],"changed":true}', encoding="utf-8")
        (self.overlay / eh.OVERLAY_DATABASE).write_bytes(b"not a SQLite database")
        problems = eh.check_overlay(self.base, self.overlay)
        self.assertTrue(any("another conversion of this base" in p for p in problems), problems)

    def test_changed_overlay_content_is_caught(self):
        connection = sqlite3.connect(self.overlay / eh.OVERLAY_DATABASE)
        connection.execute("UPDATE cluster_meshes SET name = 'z' WHERE id = 1")
        connection.commit()
        connection.close()
        problems = eh.check_overlay(self.base, self.overlay)
        self.assertEqual(len(problems), 1)
        self.assertIn("mudcrab.clusters", problems[0])
        self.assertIn("overlay.json", problems[0])
        connection = sqlite3.connect(self.overlay / eh.OVERLAY_DATABASE)
        connection.execute("UPDATE cluster_meshes SET name = 'a' WHERE id = 1")
        connection.commit()
        connection.close()
        self.assertEqual(eh.check_overlay(self.base, self.overlay), [])
        (self.overlay / "clusters" / "chunk0.bin").write_bytes(b"other")
        self.assertEqual(len(eh.check_overlay(self.base, self.overlay)), 1)

    def test_a_table_in_both_databases_is_refused(self):
        connection = sqlite3.connect(self.base / eh.DATABASE)
        connection.execute("CREATE TABLE cluster_meshes (id INTEGER)")
        connection.commit()
        connection.close()
        problems = eh.check_overlay(self.base, self.overlay)
        self.assertTrue(any("exists in both" in p or "full-copy" in p for p in problems), problems)

    def test_old_full_copy_base_is_refused(self):
        (self.base / "cluster-manifest.json").write_text("{}", encoding="utf-8")
        with self.assertRaisesRegex(eh.OverlayError, "old full-copy cluster dataset"):
            eh.resolve_overlay(self.base, "")
        self.assertTrue(eh.check_overlay(self.base, self.overlay))

    def test_foreign_kind_and_schema_are_refused(self):
        self.rewrite_metadata(lambda m: m.update(kind="mudcrab.cluster-overlay"))
        with self.assertRaisesRegex(eh.OverlayError, "not mudcrab.cluster_overlay"):
            eh.read_overlay_metadata(self.overlay)
        self.rewrite_metadata(lambda m: m.update(kind=eh.OVERLAY_KIND, schema_version=2))
        with self.assertRaisesRegex(eh.OverlayError, "schema version"):
            eh.read_overlay_metadata(self.overlay)

    def test_missing_clusters_db_is_a_problem(self):
        (self.overlay / eh.OVERLAY_DATABASE).unlink()
        self.assertTrue(any("clusters.db" in p for p in eh.check_overlay(self.base, self.overlay)))

    def test_command_line_modes(self):
        import contextlib
        import io

        def run(*argv):
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = eh.main([str(a) for a in argv])
            return code, out.getvalue()

        code, out = run("--assets", self.base, "--overlay")
        self.assertEqual((code, out.strip().splitlines()[-1]), (0, "1 extension(s), 0 problem(s)"))
        self.assertEqual(run(self.base, "--overlay", "default")[0], 0)
        self.assertEqual(run("--assets", self.base, f"--overlay={self.overlay.name}")[0], 0)
        self.assertEqual(run("--assets", self.base, "--overlay", "nope")[0], 1)
        self.assertEqual(run("--overlay")[0], 2)
        self.assertEqual(run(self.base, "--assets", self.base)[0], 2)
        # Base mode is unchanged: this base lists no extensions.
        self.assertEqual(run(self.base), (0, "0 extension(s), 0 problem(s)\n"))
        (self.base / eh.MANIFEST).write_text('{"extensions":[],"x":1}', encoding="utf-8")
        code, out = run("--assets", self.base, "--overlay")
        self.assertEqual(code, 1)
        self.assertIn("another conversion of this base", out)


# The pinned hashes of the sample's two extensions. The Rust writer's cross test must produce the
# same values for the same content; change them only together with the algorithm name.
PINNED = {
    "mudcrab.door_links": "5c9f99438d7658c66aafebaa45045f6d553f0d52521309be28b1ef108db20e87",
    "mudcrab.material": "714a8bfd6413db8bf63f22174dfb3f5c173a8126f93625d1c63a00b7df144567",
}


if __name__ == "__main__":
    unittest.main()
