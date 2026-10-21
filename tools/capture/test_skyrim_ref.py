"""Tests for the pure parts of skyrim_ref.py: fov conversion, CSV rows, grouping, entry commands."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import skyrim_ref as ref  # noqa: E402


class SkyrimRefTests(unittest.TestCase):
    def test_console_fov_is_the_horizontal_fov_at_any_aspect(self):
        self.assertEqual(ref.hfov_to_console_fov(75, 1600, 900), 75)
        self.assertEqual(ref.hfov_to_console_fov(65, 1400, 1050), 65)

    def test_csv_pitch_flips_to_up_positive_and_carries_name_and_fov(self):
        shots = [{"name": "a b", "position": [1, 2, 3], "yaw": 10, "pitch": 8, "hfov": 90}]
        rows = ref.csv_rows(shots, 1600, 900, raw_fov=False)
        self.assertEqual(rows[0], "t,x,y,z,yaw_deg,pitch_deg,name,fov")
        self.assertEqual(rows[1], "0,1,2,3,10,-8.0,a b,90.000")

    def test_raw_fov_passes_hfov_through(self):
        shots = [{"name": "n", "position": [0, 0, 0], "yaw": 0, "pitch": 0, "hfov": 75}]
        self.assertTrue(ref.csv_rows(shots, 1600, 900, raw_fov=True)[1].endswith(",75.000"))

    def test_groups_by_worldspace_and_interior(self):
        shots = [
            {"name": "a", "worldspace_id": 60, "interior_cell_id": None},
            {"name": "b", "worldspace_id": None, "interior_cell_id": 5},
            {"name": "c", "worldspace_id": 60, "interior_cell_id": None},
        ]
        groups = ref.group_shots(shots)
        self.assertEqual([[s["name"] for s in g] for g in groups], [["a", "c"], ["b"]])

    def test_far_exterior_shots_get_their_own_session(self):
        def shot(name, x, y):
            return {"name": name, "worldspace_id": 60, "interior_cell_id": None, "position": [x, y, 0]}

        shots = [shot("a", 4300, 600), shot("near", 6000, 2000), shot("far", 7600, -14000), shot("far2", 8000, -13000), shot("far3", 20800, -25400)]
        groups = ref.group_shots(shots)
        self.assertEqual([[s["name"] for s in g] for g in groups], [["a", "near"], ["far", "far2"], ["far3"]])

    def test_tamriel_entry_uses_the_exterior_grid_of_the_first_shot(self):
        group = [{"name": "a", "worldspace_id": 60, "interior_cell_id": None, "position": [20500, -45500, 6000]}]
        self.assertEqual(ref.entry_command(group, None, None), "cow Tamriel 5,-12")

    def test_explicit_coc_wins(self):
        group = [{"name": "a", "worldspace_id": 60, "interior_cell_id": None, "position": [0, 0, 0]}]
        self.assertEqual(ref.entry_command(group, "Riverwood", None), "coc Riverwood")


if __name__ == "__main__":
    unittest.main()
