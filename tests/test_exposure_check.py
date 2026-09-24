"""Tests for the render checks in tools/research/exposure_stats.py.

Synthetic images in a temporary directory: no game data, no render of a run.  The statistic is
loaded as a module and measured directly; the check itself is run as a command, because the exit
code is half of what it is for.
"""

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest

from PIL import Image, ImageDraw

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOL = os.path.join(REPO, "tools", "research", "exposure_stats.py")


def load_tool():
    spec = importlib.util.spec_from_file_location("exposure_stats", TOOL)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_image(path, size, colour):
    Image.new("RGB", size, colour).save(path)


def write_half_white(path, size):
    """An image whose left half is white and whose right half is black."""
    image = Image.new("RGB", size, (0, 0, 0))
    draw = ImageDraw.Draw(image)
    draw.rectangle([0, 0, size[0] // 2 - 1, size[1] - 1], fill=(255, 255, 255))
    image.save(path)


class FlatWhiteTest(unittest.TestCase):
    """The `flat_white` statistic: bright and colourless, which is what a sheet of water is."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.tool = load_tool()

    def tearDown(self):
        self.directory.cleanup()

    def measure(self, name, colour):
        path = os.path.join(self.directory.name, f"{name}.png")
        write_image(path, (16, 16), colour)
        return self.tool.statistics(path, 0.98, 0.05)

    def test_white_is_all_flat_white(self):
        row = self.measure("white", (255, 255, 255))
        self.assertAlmostEqual(row["flat_white"], 1.0, places=3)
        self.assertAlmostEqual(row["near_white"], 1.0, places=3)

    def test_black_and_mid_grey_are_not_flat_white(self):
        for name, colour in (("black", (0, 0, 0)), ("grey", (128, 128, 128))):
            self.assertEqual(self.measure(name, colour)["flat_white"], 0.0, name)

    def test_bright_but_coloured_is_not_flat_white(self):
        # A pale yellow: bright enough for `near_white` (its luminance is above 0.9), and its
        # saturation is nowhere near colourless.  A bright blue sky and a lit white wall are this
        # case; only a surface with no colour at all is the one the sheet of water reads as.
        row = self.measure("pale-yellow", (255, 255, 180))
        self.assertGreater(row["near_white"], 0.9)
        self.assertGreater(row["saturation"], 0.05)
        self.assertEqual(row["flat_white"], 0.0)

    def test_half_a_white_image_is_half_flat_white(self):
        path = os.path.join(self.directory.name, "half.png")
        write_half_white(path, (16, 16))
        row = self.tool.statistics(path, 0.98, 0.05)
        self.assertAlmostEqual(row["flat_white"], 0.5, places=1)
        self.assertAlmostEqual(row["near_white"], 0.5, places=1)


class CheckTest(unittest.TestCase):
    """`--check`: a shot's render against the band its own `metrics` block gives it."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = self.directory.name
        self.render_dir = os.path.join(self.root, "renders")
        os.makedirs(self.render_dir)

    def tearDown(self):
        self.directory.cleanup()

    def render(self, name, colour):
        write_image(os.path.join(self.render_dir, f"{name}.png"), (16, 16), colour)

    def run_check(self, shots):
        """Write the shots file, run `--check` over the renders, and return the result."""
        path = os.path.join(self.root, "shots.json")
        with open(path, "w", encoding="utf-8") as handle:
            json.dump({"width": 1400, "height": 1050, "shots": shots}, handle)
        return subprocess.run(
            [sys.executable, TOOL, "--shots", path, "--render", self.render_dir, "--check"],
            capture_output=True,
            text=True,
            check=False,
        )

    def shot(self, name, metrics=None):
        shot = {
            "name": name,
            "worldspace_id": 60,
            "interior_cell_id": None,
            "position": [0.0, 0.0, 0.0],
            "yaw": 0.0,
            "pitch": 0.0,
            "hfov": 75.0,
            "note": "synthetic",
        }
        if metrics is not None:
            shot["metrics"] = metrics
        return shot

    def test_a_render_inside_its_band_passes(self):
        self.render("shot-dark", (30, 30, 30))
        result = self.run_check(
            [self.shot("shot-dark", {"flat_white_max": 0.15, "mean_luma_min": 0.005})]
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("ok   shot-dark: flat_white 0.000 <= flat_white_max 0.15", result.stdout)
        self.assertIn("ok   shot-dark: mean luma 0.013 >= mean_luma_min 0.005", result.stdout)
        self.assertIn("every one of the 1 shot(s) with a band is inside it", result.stdout)

    def test_a_white_sheet_fails_its_flat_white_band(self):
        self.render("shot-water", (255, 255, 255))
        result = self.run_check([self.shot("shot-water", {"flat_white_max": 0.15})])
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(
            "FAIL shot-water: flat_white 1.000 > flat_white_max 0.15", result.stdout
        )
        self.assertIn("1 shot(s) outside their band", result.stdout)

    def test_a_dark_room_fails_its_mean_band(self):
        self.render("shot-hearth", (0, 0, 0))
        self.render("shot-daylight", (128, 128, 128))
        result = self.run_check(
            [
                self.shot("shot-hearth", {"mean_luma_min": 0.05}),
                self.shot("shot-daylight", {"mean_luma_max": 0.1}),
            ]
        )
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("FAIL shot-hearth: mean luma 0.000 < mean_luma_min 0.05", result.stdout)
        self.assertIn("FAIL shot-daylight: mean luma 0.216 > mean_luma_max 0.1", result.stdout)
        self.assertIn("2 shot(s) outside their band", result.stdout)

    def test_a_shot_without_a_band_is_reported_and_not_failed(self):
        # It has no render either: a shot with no band is unchecked, so its render is not needed.
        result = self.run_check([self.shot("shot-unbanded")])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("---- shot-unbanded: no metrics band", result.stdout)
        self.assertIn("every one of the 0 shot(s) with a band is inside it", result.stdout)

    def test_a_shot_with_a_band_and_no_render_fails(self):
        result = self.run_check([self.shot("shot-unrendered", {"flat_white_max": 0.15})])
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("FAIL shot-unrendered: no render at", result.stdout)
        self.assertIn("flat_white_max 0.15", result.stdout)

    def test_a_band_named_wrong_fails_rather_than_checking_nothing(self):
        self.render("shot-typo", (255, 255, 255))
        result = self.run_check([self.shot("shot-typo", {"flat_white_maximum": 0.15})])
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("FAIL shot-typo: unknown metric 'flat_white_maximum'", result.stdout)

    def test_check_needs_a_shots_file(self):
        result = subprocess.run(
            [sys.executable, TOOL, "--render", self.render_dir, "--check"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("--shots", result.stderr)


if __name__ == "__main__":
    unittest.main()
