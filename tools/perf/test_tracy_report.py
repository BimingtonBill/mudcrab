"""Tests for the Tracy zone report (tools/perf/tracy_report.py).

Run with: python -m unittest tools/perf/test_tracy_report.py
"""

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import tracy_report as tr  # noqa: E402

HEADER = "name,src_file,src_line,total_ns,total_perc,counts,mean_ns,min_ns,max_ns,std_ns\n"


class TracyReportTests(unittest.TestCase):
    def write(self, name, rows):
        folder = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: [p.unlink() for p in folder.iterdir()] and None or folder.rmdir())
        path = folder / name
        path.write_text(HEADER + "".join(rows), encoding="utf-8")
        return path

    def test_summary_sorts_by_total_and_adds_per_frame(self):
        path = self.write("a.csv", [
            "queue_material_meshes,C:/x/queue.rs,10,4000000,10,100,40000,1,90000,5\n",
            "render thread,C:/x/render.rs,1,90000000,90,100,900000,1,2000000,5\n",
        ])
        lines = tr.summary(tr.load(path), frames=100, top=5, match=None).splitlines()
        self.assertTrue(lines[2].startswith("| render thread |"))
        self.assertTrue(lines[2].endswith("| 0.900 |"))
        self.assertIn("queue.rs:10", lines[3])

    def test_names_with_commas_and_system_wrapper(self):
        path = self.write("a.csv", [
            'system{name="m::f<A, B>"},C:\\x\\function_system.rs,52,2000000,1,2,1000000,1,1,0\n',
        ])
        zone = tr.load(path)[0]
        self.assertEqual(zone.name, "system m::f<A, B>")
        self.assertEqual(zone.src, "function_system.rs:52")
        self.assertEqual(zone.counts, 2)

    def test_summary_match_filters(self):
        path = self.write("a.csv", ["alpha,f.rs,1,1000,1,1,1000,1,1000,0\n", "beta,f.rs,2,2000,1,1,2000,1,2000,0\n"])
        text = tr.summary(tr.load(path), None, 5, "BET")
        self.assertIn("beta", text)
        self.assertNotIn("alpha", text)

    def test_loading_zones_hidden_and_frames_detected(self):
        path = self.write("a.csv", [
            "main_render_schedule,f.rs,1,50000000,5,50,1000000,1,2000000,0\n",
            'asset loading{loader="x" asset="a.glb"},f.rs,2,90000000,9,1,90000000,1,90000000,0\n',
            "queue,f.rs,3,5000000,1,50,100000,1,200000,0\n",
        ])
        zones = tr.load(path)
        self.assertEqual(tr.frames_in(zones), 50)
        text = tr.summary(zones, None, 5, None)
        self.assertNotIn("asset loading", text)
        self.assertIn("| queue | f.rs:3 | 5.00 | 50 | 0.100 | 0.200 | 0.100 |", text)
        self.assertIn("asset loading", tr.summary(zones, None, 5, None, keep_loading=True))

    def test_long_loading_names_are_still_hidden(self):
        long_asset = "a" * 140
        path = self.write("a.csv", [
            f'asset loading{{loader="x" asset="{long_asset}.glb"}},f.rs,2,90000000,9,1,90000000,1,90000000,0\n',
            "queue,f.rs,3,5000000,1,50,100000,1,200000,0\n",
        ])
        zones = tr.load(path)
        self.assertTrue(zones[0].name.startswith("..."))
        self.assertNotIn("aaaa", tr.summary(zones, None, 5, None))

    def test_diff_reports_mean_change_and_new_zones(self):
        before = self.write("b.csv", ["a,f.rs,1,1000000,1,10,100000,1,1,0\n"])
        after = self.write("c.csv", [
            "a,f.rs,1,3000000,1,10,300000,1,1,0\n",
            "new,f.rs,2,500000,1,5,100000,1,1,0\n",
        ])
        text = tr.diff(tr.load(before), tr.load(after), top=5, min_ms=0.01)
        self.assertIn("| a | f.rs:1 | 0.100 | 0.300 | +0.200 |", text)
        self.assertIn("| new | f.rs:2 | 0.000 | 0.100 | +0.100 |", text)


    def test_gpu_events_aggregate_into_a_zone_table(self):
        folder = Path(tempfile.mkdtemp())
        events = folder / "gpu-events.csv"
        events.write_text(
            "name,src_file,Time from start of program,GPU execution time\n"
            "main_opaque_pass_3d,,100,3000000\n"
            "main_opaque_pass_3d,,200,1000000\n"
            "bloom,,300,500000\n",
            encoding="utf-8",
        )
        table = folder / "gpu-zones.csv"
        table.write_text(tr.gpu_table(events), encoding="utf-8")
        self.addCleanup(lambda: [p.unlink() for p in folder.iterdir()] and None or folder.rmdir())
        zones = tr.load(table)
        self.assertEqual([z.name for z in zones], ["main_opaque_pass_3d", "bloom"])
        self.assertEqual(zones[0].counts, 2)
        self.assertEqual(zones[0].mean_ns, 2000000)
        self.assertEqual(zones[0].max_ns, 3000000)
        self.assertIn("| main_opaque_pass_3d | gpu:0 | 4.00 | 2 | 2.000 | 3.000 | 0.040 |", tr.summary(zones, 100, 5, None))

    def test_gpu_table_rejects_a_cpu_zone_table(self):
        path = self.write("a.csv", ["alpha,f.rs,1,1000,1,1,1000,1,1000,0\n"])
        with self.assertRaises(ValueError):
            tr.gpu_table(path)


if __name__ == "__main__":
    unittest.main()
