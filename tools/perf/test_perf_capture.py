"""Tests for the scene arguments of tools/perf/perf_capture.py.

Run with: python -m unittest tools/perf/test_perf_capture.py
"""

import argparse
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import perf_capture as pc  # noqa: E402


def namespace(scene, assets=None, seconds=None, out=None, engine_args=()):
    return argparse.Namespace(scene=scene, assets=assets, seconds=seconds, out=out, label="t", engine_args=list(engine_args))


class SceneArgumentTests(unittest.TestCase):
    def test_every_scene_resolves_with_assets_and_window(self):
        scenes = json.loads(pc.SCENES.read_text(encoding="utf-8"))["scenes"]
        with tempfile.TemporaryDirectory() as out:
            for name, scene in scenes.items():
                args = pc.scene_args(namespace(name, assets="C:/data" if scene["assets"] else None, out=out))
                self.assertIn("--benchmark-duration", args, name)
                self.assertEqual("--assets" in args, scene["assets"], name)
                self.assertFalse(any("{" in arg for arg in args), f"{name}: unresolved placeholder in {args}")

    def test_seconds_and_trailing_engine_arguments_are_kept(self):
        with tempfile.TemporaryDirectory() as out:
            args = pc.scene_args(namespace("synthetic", seconds=7, out=out, engine_args=["--no-fog"]))
        self.assertEqual(args[args.index("--benchmark-duration") + 1], "7")
        self.assertEqual(args[-1], "--no-fog")

    def test_world_scene_without_assets_is_refused(self):
        with tempfile.TemporaryDirectory() as out, self.assertRaises(SystemExit):
            pc.scene_args(namespace("dense", out=out))

    def test_unknown_scene_is_refused(self):
        with self.assertRaises(SystemExit):
            pc.scene_args(namespace("nope"))


if __name__ == "__main__":
    unittest.main()
