"""Tests for tools/research/portal_door_alignment.py - the port's arithmetic and its gate.

The tool is the evidence for an engine change: every number in `docs/research/portal-door-
alignment.md` came out of it, and it ports the engine's door -> arrival map line for line. Its
own arithmetic is pinned here - the quaternion port against the Creation heading convention, the
outward rule and its two distances, `door_frame`, `Rigid` and the two maps, and the histogram the
report's tables are cut with - and its `check` gate is both unit-tested and run against the
converted data when that data is installed.

The last class is skipped when `C:/Modding/SkyrimConverted/skyrim_world.db` is absent: the gate
asserts one machine's converted data, and a checkout that has never run a conversion is not a
failure (the command itself exits 0 there, which is also tested).
"""

import contextlib
import io
import math
import os
import subprocess
import sys
import tempfile
import unittest

from tools.research import portal_door_alignment as portal

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOL = os.path.join(REPO, "tools", "research", "portal_door_alignment.py")

IDENTITY = [0.0, 0.0, 0.0, 1.0]

# The report's section 5 rows, and the model convention they were fitted from (the exact figure
# the tool prints, `178.7181116646711`, of which the report quotes 178.718).
SVEN_MODEL = "Architecture\\Farmhouse\\FarmhouseLDoor01.nif"
SVEN_CONVENTION = 178.7181116646711
SVEN_EXTERIOR = 0x0001CBB0
SVEN_INTERIOR = 0x0001CBAF
SVEN_BOUNDS_CENTRE = (0.0, 88.0, -13.5)

# The report quotes the raw rows rounded, and its hand check reproduced the port to 0.01 degree
# and 0.1 unit from them: that is what these tolerances are.
SVEN_DEGREES = 0.02
SVEN_UNITS = 0.1


def runtime_to_creation(vector):
    """The inverse of `creation_to_runtime_vector`: the tool works in runtime (Y up) axes."""
    return [vector[0], -vector[2], vector[1]]


def horizontal_distance(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


class QuaternionTest(unittest.TestCase):
    """The port of `crates/shared/src/coordinates.rs`, in the [x, y, z, w] convention."""

    def assertQuaternionAlmostEqual(self, measured, expected, places=12):
        self.assertEqual(len(measured), len(expected))
        for index, (value, want) in enumerate(zip(measured, expected)):
            self.assertAlmostEqual(value, want, places=places, msg=f"component {index}")

    def assertVectorAlmostEqual(self, measured, expected, places=9):
        self.assertEqual(len(measured), len(expected))
        for index, (value, want) in enumerate(zip(measured, expected)):
            self.assertAlmostEqual(value, want, places=places, msg=f"component {index}")

    def test_identity_is_the_multiplicative_unit(self):
        quaternions = [
            IDENTITY,
            portal.from_rotation_y(0.0),
            portal.from_rotation_y(0.7),
            portal.q_normalize(portal.from_rotation_y(-2.2)),
            portal.creation_euler_to_runtime_quaternion([0.0, 0.0, 0.4]),
        ]
        for quaternion in quaternions:
            with self.subTest(quaternion=quaternion):
                self.assertQuaternionAlmostEqual(portal.q_mul(quaternion, IDENTITY), quaternion)
                self.assertQuaternionAlmostEqual(portal.q_mul(IDENTITY, quaternion), quaternion)

    def test_multiplication_is_associative(self):
        a = portal.from_rotation_y(0.31)
        b = portal.creation_euler_to_runtime_quaternion([0.2, -0.4, 1.1])
        c = portal.q_normalize([0.3, -0.2, 0.5, 0.7])
        self.assertQuaternionAlmostEqual(
            portal.q_mul(portal.q_mul(a, b), c), portal.q_mul(a, portal.q_mul(b, c))
        )

    def test_multiplication_composes_rotations(self):
        # `a * b` applies `b` first, as `Rigid.compose` and the engine's quaternion product do.
        a = portal.from_rotation_y(0.9)
        b = portal.from_rotation_y(-0.35)
        point = [12.0, -4.0, 7.5]
        self.assertVectorAlmostEqual(
            portal.rotate(portal.q_mul(a, b), point), portal.rotate(a, portal.rotate(b, point))
        )
        self.assertQuaternionAlmostEqual(portal.q_mul(a, b), portal.from_rotation_y(0.55))

    def test_from_rotation_y_faces_the_creation_heading_it_turns_away_from(self):
        # The runtime forward is -Z, and a runtime yaw of `t` faces the Creation heading `-t`.
        for degrees in (-170.0, -90.0, -18.51, 0.0, 1.0, 45.0, 179.0):
            radians = math.radians(degrees)
            with self.subTest(degrees=degrees):
                forward = portal.rotate(portal.from_rotation_y(radians), [0.0, 0.0, -1.0])
                heading = math.radians(-degrees)
                self.assertVectorAlmostEqual(
                    forward, portal.creation_to_runtime([math.sin(heading), math.cos(heading), 0.0])
                )

    def test_a_reference_yaw_is_a_creation_heading(self):
        # An object's reference yaw `z` faces `(sin z, cos z)` (crates/shared/src/coordinates.rs
        # :24-50, and the engine's own `a_yaw_turns_clockwise_like_a_heading` test).
        for degrees in (-170.0, -90.0, -30.147, 0.0, 1.0, 151.135, 179.0, 270.0):
            with self.subTest(degrees=degrees):
                rotation = portal.creation_euler_to_runtime_quaternion([0.0, 0.0, math.radians(degrees)])
                forward = portal.rotate(rotation, [0.0, 0.0, -1.0])
                radians = math.radians(degrees)
                self.assertVectorAlmostEqual(
                    forward, portal.creation_to_runtime([math.sin(radians), math.cos(radians), 0.0])
                )
                measured = portal.q_to_yaw_degrees(rotation)
                self.assertAlmostEqual(portal.wrap180(measured - degrees), 0.0, places=9)

    def test_q_to_yaw_degrees_reads_a_yaw_only_rotation(self):
        for degrees in (-179.0, -90.0, -1.0, 0.0, 12.27, 90.0, 179.0):
            radians = math.radians(degrees)
            with self.subTest(degrees=degrees):
                self.assertAlmostEqual(portal.q_to_yaw_degrees(portal.from_rotation_y(radians)),
                                       -degrees, places=9)

    def test_wrap180_keeps_headings_in_a_half_turn(self):
        self.assertEqual(portal.wrap180(0.0), 0.0)
        self.assertEqual(portal.wrap180(90.0), 90.0)
        self.assertEqual(portal.wrap180(180.0), -180.0)
        self.assertEqual(portal.wrap180(-180.0), -180.0)
        self.assertEqual(portal.wrap180(360.0), 0.0)
        self.assertEqual(portal.wrap180(181.0), -179.0)
        self.assertEqual(portal.wrap180(-181.0), 179.0)


class OutwardRuleTest(unittest.TestCase):
    """`outward_from_return_link` (crates/engine/src/doors.rs:87) and its two distances."""

    def test_the_point_rule_points_from_the_door_to_the_arrival(self):
        # The tower-door case: the arrival 32 units east of the door says the door faces east.
        outward, rule = portal.outward_from_return_link([0.0, 0.0, 0.0], [32.0, 0.0, 0.0], [0.0, 0.0, 1.0])
        self.assertEqual(rule, "point")
        self.assertAlmostEqual(outward[0], 1.0, places=12)
        self.assertAlmostEqual(outward[1], 0.0, places=12)
        self.assertEqual(outward[2], 0.0)

    def test_the_point_rule_returns_a_unit_vector(self):
        outward, rule = portal.outward_from_return_link([0.0, 0.0, 0.0], [30.0, 40.0, 0.0], [0.0, 0.0, 0.0])
        self.assertEqual(rule, "point")
        self.assertAlmostEqual(outward[0], 0.6, places=12)
        self.assertAlmostEqual(outward[1], 0.8, places=12)

    def test_the_window_edges_are_16_and_512_units(self):
        # 16 < distance <= 512: both edges are tested from either side.
        for distance, rule in ((16.0, "heading"), (16.0000001, "point"),
                               (512.0, "point"), (512.0000001, "heading")):
            with self.subTest(distance=distance):
                _, measured = portal.outward_from_return_link(
                    [0.0, 0.0, 0.0], [distance, 0.0, 0.0], [0.0, 0.0, 1.2345]
                )
                self.assertEqual(measured, rule)

    def test_an_arrival_at_the_door_falls_back_to_the_heading(self):
        outward, rule = portal.outward_from_return_link([10.0, 20.0, 0.0], [10.0, 20.0, 0.0], [0.0, 0.0, 0.7])
        self.assertEqual(rule, "heading")
        self.assertAlmostEqual(outward[0], math.sin(0.7), places=12)
        self.assertAlmostEqual(outward[1], math.cos(0.7), places=12)
        self.assertEqual(outward[2], 0.0)

    def test_an_unmeasurable_arrival_point_falls_back_to_the_heading(self):
        for bad in (float("nan"), float("inf"), float("-inf")):
            with self.subTest(bad=bad):
                outward, rule = portal.outward_from_return_link([0.0, 0.0, 0.0], [bad, 0.0, 0.0], [0.0, 0.0, 0.0])
                self.assertEqual(rule, "heading")
                self.assertAlmostEqual(outward[0], 0.0, places=12)
                self.assertAlmostEqual(outward[1], 1.0, places=12)

    def test_an_unreadable_heading_is_no_outward(self):
        outward, rule = portal.outward_from_return_link([0.0, 0.0, 0.0], [0.0, 0.0, 0.0],
                                                        [0.0, 0.0, float("nan")])
        self.assertIsNone(outward)
        self.assertEqual(rule, "none")


class DoorFrameTest(unittest.TestCase):
    """`door_frame` (crates/engine/src/transition.rs:237): a pure yaw whose `-Z` is the outward."""

    def test_minus_z_faces_the_outward_direction(self):
        for degrees in (-170.0, -90.0, -18.51, 0.0, 45.0, 179.0):
            radians = math.radians(degrees)
            with self.subTest(degrees=degrees):
                frame = portal.door_frame(portal.from_rotation_y(0.3),
                                          [math.sin(radians), math.cos(radians), 0.0])
                forward = portal.rotate(frame, [0.0, 0.0, -1.0])
                for measured, expected in zip(forward,
                                              portal.creation_to_runtime([math.sin(radians),
                                                                          math.cos(radians), 0.0])):
                    self.assertAlmostEqual(measured, expected, places=9)
                self.assertAlmostEqual(portal.q_to_yaw_degrees(frame), degrees, places=9)

    def test_it_survives_a_none_outward(self):
        # No door in the install is in that state, but the fallback is the reference rotation.
        rotation = portal.q_normalize(portal.from_rotation_y(0.8))
        self.assertIs(portal.door_frame(rotation, None), rotation)

    def test_a_degenerate_outward_falls_back_to_the_reference_rotation(self):
        rotation = portal.q_normalize(portal.from_rotation_y(-0.4))
        for outward in ([0.0, 0.0, 0.0], [float("nan"), 1.0, 0.0], [0.0, float("inf"), 0.0]):
            with self.subTest(outward=outward):
                self.assertEqual(portal.door_frame(rotation, outward), rotation)

    def test_the_outward_length_does_not_matter(self):
        frame = portal.door_frame(portal.from_rotation_y(0.2), [3.0, 4.0, 0.0])
        unit = portal.door_frame(portal.from_rotation_y(0.2), [0.6, 0.8, 0.0])
        for measured, expected in zip(frame, unit):
            self.assertAlmostEqual(measured, expected, places=12)


class RigidTest(unittest.TestCase):
    """`Rigid`: `p -> rotation * p + translation`, the shape both maps come in.

    Every map in this tool is a yaw about the vertical, and that is what lets the portal map the
    eye while the crossing maps the feet: the two can only drift apart through a non-yaw part.
    """

    POINTS = ([0.0, 0.0, 0.0], [10.0, -3.0, 2.0], [180.5, -900.25, 41.0])

    def assertVectorAlmostEqual(self, measured, expected, places=9):
        for index, (value, want) in enumerate(zip(measured, expected)):
            self.assertAlmostEqual(value, want, places=places, msg=f"component {index}")

    def test_composition_applies_the_other_one_first(self):
        first = portal.Rigid(portal.q_normalize(portal.from_rotation_y(0.4)), [3.0, -2.0, 7.0])
        second = portal.Rigid(portal.q_normalize(portal.from_rotation_y(-1.1)), [-9.0, 4.0, 2.5])
        composed = second.compose(first)
        for point in self.POINTS:
            with self.subTest(point=point):
                self.assertVectorAlmostEqual(composed.apply(point), second.apply(first.apply(point)))

    def test_the_inverse_round_trips(self):
        transform = portal.Rigid(portal.q_normalize(portal.from_rotation_y(2.0)), [40.0, 5.0, -12.0])
        for point in self.POINTS:
            with self.subTest(point=point):
                self.assertVectorAlmostEqual(transform.inverse().apply(transform.apply(point)), point)
                self.assertVectorAlmostEqual(transform.apply(transform.inverse().apply(point)), point)

    def test_a_composition_inverts_in_the_other_order(self):
        first = portal.Rigid(portal.q_normalize(portal.from_rotation_y(0.9)), [1.0, 2.0, 3.0])
        second = portal.Rigid(portal.q_normalize(portal.from_rotation_y(-0.2)), [-4.0, 0.5, 6.0])
        for point in self.POINTS:
            with self.subTest(point=point):
                self.assertVectorAlmostEqual(
                    second.compose(first).inverse().apply(point),
                    first.inverse().compose(second.inverse()).apply(point),
                )

    def test_the_engine_map_is_rigid_and_keeps_the_vertical(self):
        mapping = portal.engine_map(
            [10.0, 5.0, -3.0],
            portal.door_frame(portal.from_rotation_y(0.4), [0.0, 1.0, 0.0]),
            [200.0, 7.0, 30.0],
            portal.arrival_camera_rotation([0.0, 0.0, 1.0]),
        )
        a, b = [0.0, 1.0, 2.0], [182.0, -63.0, 12.0]
        mapped_a, mapped_b = mapping.apply(a), mapping.apply(b)
        self.assertAlmostEqual(math.dist(mapped_a, mapped_b), math.dist(a, b), places=9)
        self.assertAlmostEqual(mapped_a[1] - a[1], mapped_b[1] - b[1], places=9)
        yaw, pitch, roll = portal.q_to_yxz_degrees(mapping.rotation)
        self.assertAlmostEqual(pitch, 0.0, places=9)
        self.assertAlmostEqual(roll, 0.0, places=9)
        self.assertFalse(math.isnan(yaw))


class MapTest(unittest.TestCase):
    """`engine_map` and `geometry_map`: the two maps differ only in the frame they anchor to."""

    def setUp(self):
        self.source_position = [100.0, 20.0, -5.0]
        self.arrival_position = [-400.0, 90.0, 250.0]
        self.source_frame = portal.door_frame(portal.from_rotation_y(0.0), [0.0, 1.0, 0.0])
        self.destination_frame = portal.door_frame(portal.from_rotation_y(0.0), [0.9, -0.4358898943540674, 0.0])

    def test_a_point_at_the_source_lands_on_the_arrival_point(self):
        mapping = portal.engine_map(self.source_position, self.source_frame, self.arrival_position,
                                    portal.arrival_camera_rotation([0.0, 0.0, 1.0]))
        landed = mapping.apply(self.source_position)
        for index, (value, want) in enumerate(zip(landed, self.arrival_position)):
            self.assertAlmostEqual(value, want, places=9, msg=f"component {index}")

    def test_the_geometry_map_takes_the_source_doorway_centre_to_the_destination_one(self):
        mapping = portal.geometry_map(self.source_position, self.source_frame,
                                      self.arrival_position, self.destination_frame)
        landed = mapping.apply(self.source_position)
        for index, (value, want) in enumerate(zip(landed, self.arrival_position)):
            self.assertAlmostEqual(value, want, places=9, msg=f"component {index}")

    def test_both_maps_are_a_pure_yaw(self):
        mappings = (
            portal.engine_map(self.source_position, self.source_frame, self.arrival_position,
                              portal.arrival_camera_rotation([0.0, 0.0, 1.0])),
            portal.geometry_map(self.source_position, self.source_frame, self.arrival_position,
                                self.destination_frame),
        )
        for mapping in mappings:
            with self.subTest(rotation=mapping.rotation):
                _, pitch, roll = portal.q_to_yxz_degrees(mapping.rotation)
                self.assertAlmostEqual(pitch, 0.0, places=9)
                self.assertAlmostEqual(roll, 0.0, places=9)

    def test_the_geometry_map_turns_the_source_outward_into_the_destination_inward(self):
        # Walking into one doorway has to mean walking out of the other, which is the half turn
        # `door_to_arrival_rotation` already contains.
        mapping = portal.geometry_map(self.source_position, self.source_frame,
                                      self.arrival_position, self.destination_frame)
        source_outward = portal.rotate(self.source_frame, [0.0, 0.0, -1.0])
        destination_outward = portal.rotate(self.destination_frame, [0.0, 0.0, -1.0])
        turned = portal.rotate(mapping.rotation, source_outward)
        for index, (value, want) in enumerate(zip(turned, destination_outward)):
            self.assertAlmostEqual(value, -want, places=9, msg=f"component {index}")


class HistogramTest(unittest.TestCase):
    """`histogram`/`histogram_table`/`percentile`: the report's tables are cut with these."""

    EDGES = (1.0, 5.0, 15.0, 45.0)

    def test_an_edge_belongs_to_the_bucket_it_closes(self):
        self.assertEqual(portal.histogram([1.0], self.EDGES), [1, 0, 0, 0, 0])
        self.assertEqual(portal.histogram([1.000001], self.EDGES), [0, 1, 0, 0, 0])
        self.assertEqual(portal.histogram([15.0], self.EDGES), [0, 0, 1, 0, 0])
        self.assertEqual(portal.histogram([45.0], self.EDGES), [0, 0, 0, 1, 0])
        self.assertEqual(portal.histogram([45.000001], self.EDGES), [0, 0, 0, 0, 1])

    def test_the_table_labels_every_bucket(self):
        table = portal.histogram_table([0.5, 2.0, 6.0, 20.0, 90.0])
        self.assertEqual([label for label, _, _ in table], ["<= 1", "1..5", "5..15", "15..45", "> 45"])
        self.assertEqual([count for _, count, _ in table], [1, 1, 1, 1, 1])
        self.assertEqual(sum(count for _, count, _ in table), 5)
        for _, _, share in table:
            self.assertAlmostEqual(share, 20.0, places=9)

    def test_an_empty_histogram_has_zero_shares(self):
        table = portal.histogram_table([])
        self.assertEqual([count for _, count, _ in table], [0, 0, 0, 0, 0])
        self.assertEqual([share for _, _, share in table], [0.0, 0.0, 0.0, 0.0, 0.0])

    def test_percentile_boundaries(self):
        self.assertTrue(math.isnan(portal.percentile([], 0.5)))
        self.assertEqual(portal.percentile([7.0], 0.5), 7.0)
        values = [0.0, 1.0, 2.0, 3.0]
        self.assertEqual(portal.percentile(values, 0.0), 0.0)
        self.assertEqual(portal.percentile(values, 1.0), 3.0)
        self.assertEqual(portal.percentile(values, -1.0), 0.0)
        self.assertEqual(portal.percentile(values, 2.0), 3.0)
        # The index is `round(fraction * (n - 1))` and Python's `round` is half-to-even, so the
        # median of four values is the upper of the two middle ones, not the lower.
        self.assertEqual(portal.percentile(values, 0.5), 2.0)
        self.assertEqual(portal.percentile([0.0, 10.0], 0.5), 0.0)


class GateTest(unittest.TestCase):
    """The `check` gate's own bookkeeping, and the skip when there is no data to check."""

    def test_a_pass_and_a_failure_are_one_line_each(self):
        gate = portal.Gate()
        self.assertTrue(gate.check("a count", 2204, 2204, portal.count_tolerance(2204)))
        self.assertFalse(gate.check("an angle", 3.0, 2.65, portal.CHECK_DEGREES, " deg"))
        self.assertEqual(len(gate.lines), 2)
        self.assertEqual(len(gate.failures), 1)
        self.assertTrue(gate.lines[0].startswith("  ok  "))
        self.assertTrue(gate.failures[0].startswith("  FAIL "))
        # One line, with the expected value and the measured one on it.
        self.assertIn("expected 2.65 +- 0.05 deg", gate.failures[0])
        self.assertIn("3.00 deg", gate.failures[0])

    def test_the_tolerances_are_the_ones_the_task_names(self):
        self.assertEqual(portal.CHECK_DEGREES, 0.05)
        self.assertEqual(portal.CHECK_UNITS, 2.0)
        self.assertAlmostEqual(portal.count_tolerance(2204), 22.04, places=9)
        # One per cent: a count just inside the band passes, just outside it fails. The angle
        # case stays off the exact edge (2.70 - 2.65 is 0.050000000000000266 in binary), since
        # the gate is `abs(difference) <= tolerance` rather than a tolerance with slack in it.
        gate = portal.Gate()
        self.assertTrue(gate.check("inside", 101, 100, portal.count_tolerance(100)))
        self.assertFalse(gate.check("outside", 102, 100, portal.count_tolerance(100)))
        self.assertTrue(gate.check("inside", 102.0, 100.0, portal.CHECK_UNITS, " u"))
        self.assertFalse(gate.check("outside", 102.1, 100.0, portal.CHECK_UNITS, " u"))
        self.assertTrue(gate.check("inside", 2.69, 2.65, portal.CHECK_DEGREES, " deg"))
        self.assertFalse(gate.check("outside", 2.71, 2.65, portal.CHECK_DEGREES, " deg"))

    def test_a_failure_can_be_recorded_without_an_expected_value(self):
        gate = portal.Gate()
        gate.fail("Sven's House", "the direction is not in the data")
        self.assertEqual(len(gate.failures), 1)
        self.assertEqual(gate.lines, ["  FAIL Sven's House: the direction is not in the data"])

    def test_a_missing_database_is_not_a_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            missing = os.path.join(directory, "not-there.db")
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                status = portal.command_check_database(missing)
        self.assertEqual(status, 0)
        self.assertIn("database not present", output.getvalue())
        self.assertIn(missing, output.getvalue())


class SvenHouseTest(unittest.TestCase):
    """Report section 5, computed from the raw rows pasted there.

    This is the regression on the port's *inputs*: the rows are built the way the database hands
    them over (`door_row` decodes them, `wire_doors` links them, `annotate_doors` annotates them,
    `PairMeasurement` measures the pair), so a change to any of those four steps shows up as a
    changed number here, not only in the gate that needs the converted data.
    """

    @staticmethod
    def row(ref_id, position, yaw_degrees, editor_id):
        """A `DOOR_SELECT` row for one door of the report's table (bounds min = max = centre)."""
        centre = SVEN_BOUNDS_CENTRE
        return (ref_id, None, None, 0, SVEN_MODEL, editor_id,
                position[0], position[1], position[2],
                0.0, 0.0, math.radians(yaw_degrees),
                centre[0], centre[1], centre[2],
                centre[0], centre[1], centre[2], 1, 1.0)

    def setUp(self):
        exterior = portal.door_row(self.row(SVEN_EXTERIOR, (20669.508, -46394.473, -122.135),
                                            151.135, "Tamriel"))
        interior = portal.door_row(self.row(SVEN_INTERIOR, (-511.494, -254.942, -16.000),
                                            180.000, "RiverwoodSvensHouse"))
        links = {
            SVEN_EXTERIOR: {
                "ref_id": SVEN_EXTERIOR, "destination": SVEN_INTERIOR, "cell": None, "worldspace": None,
                # The `XTEL` target inside the house; `pair 1CBB0` prints the same rows.
                "position": (-510.1684265136719, -198.87686157226562, -16.0),
                "rotation": (0.0, 0.0, math.radians(-1.914)),
            },
            SVEN_INTERIOR: {
                "ref_id": SVEN_INTERIOR, "destination": SVEN_EXTERIOR, "cell": None, "worldspace": None,
                "position": (20644.044921875, -46318.4140625, -121.8624038696289),
                "rotation": (0.0, 0.0, math.radians(-27.697)),
            },
        }
        self.doors = {SVEN_EXTERIOR: exterior, SVEN_INTERIOR: interior}
        portal.wire_doors(self.doors, links)
        portal.annotate_doors(self.doors, {SVEN_MODEL: SVEN_CONVENTION})

    def test_the_reference_rows_are_read_the_way_the_report_reads_them(self):
        exterior, interior = self.doors[SVEN_EXTERIOR], self.doors[SVEN_INTERIOR]
        self.assertEqual(exterior.outward_rule, "point")
        self.assertEqual(interior.outward_rule, "point")
        self.assertAlmostEqual(horizontal_distance(exterior.position, exterior.return_link["position"]),
                               80.2, delta=SVEN_UNITS)
        self.assertAlmostEqual(horizontal_distance(interior.position, interior.return_link["position"]),
                               56.1, delta=SVEN_UNITS)
        # The engine's frame: the link-derived outward, against the doorway's own facing below.
        self.assertAlmostEqual(exterior.facing, -18.510, delta=SVEN_DEGREES)
        self.assertAlmostEqual(interior.facing, 1.354, delta=SVEN_DEGREES)
        # The doorway's own facing: the reference yaw plus the model's axis convention.
        self.assertAlmostEqual(exterior.facing_model, -30.147, delta=SVEN_DEGREES)
        self.assertAlmostEqual(interior.facing_model, -1.282, delta=SVEN_DEGREES)

    def test_the_doorway_centres_are_the_report_s(self):
        exterior = runtime_to_creation(portal.doorway_anchor(self.doors[SVEN_EXTERIOR]))
        interior = runtime_to_creation(portal.doorway_anchor(self.doors[SVEN_INTERIOR]))
        for measured, expected in zip(exterior, (20676.02, -46406.30, -34.13)):
            self.assertAlmostEqual(measured, expected, delta=SVEN_UNITS)
        for measured, expected in zip(interior, (-511.49, -268.44, 72.00)):
            self.assertAlmostEqual(measured, expected, delta=SVEN_UNITS)

    def test_looking_in_sees_the_destination_turned_and_drawn_nearer(self):
        measurement = portal.PairMeasurement(self.doors[SVEN_EXTERIOR], self.doors[SVEN_INTERIOR],
                                             {SVEN_MODEL: SVEN_CONVENTION})
        self.assertAlmostEqual(measurement.yaw_error, 12.27, delta=SVEN_DEGREES)
        self.assertAlmostEqual(measurement.apparent_distance, 82.8, delta=SVEN_UNITS)
        self.assertAlmostEqual(measurement.apparent_depth, -80.8, delta=SVEN_UNITS)
        self.assertAlmostEqual(measurement.apparent_lateral, 17.9, delta=SVEN_UNITS)
        # The two terms the report splits it into, and the engine's landing inside the room.
        self.assertAlmostEqual(measurement.destination_error, 0.63, delta=SVEN_DEGREES)
        self.assertAlmostEqual(measurement.source_error, -11.64, delta=SVEN_DEGREES)
        self.assertAlmostEqual(measurement.gap_depth, 69.5, delta=SVEN_UNITS)

    def test_looking_out_is_nearly_right(self):
        measurement = portal.PairMeasurement(self.doors[SVEN_INTERIOR], self.doors[SVEN_EXTERIOR],
                                             {SVEN_MODEL: SVEN_CONVENTION})
        self.assertAlmostEqual(measurement.yaw_error, 0.19, delta=SVEN_DEGREES)
        self.assertAlmostEqual(measurement.apparent_distance, 106.9, delta=SVEN_UNITS)
        self.assertAlmostEqual(measurement.apparent_depth, -105.5, delta=SVEN_UNITS)


@unittest.skipUnless(os.path.exists(portal.DATABASE), f"no converted data at {portal.DATABASE}")
class ConvertedDataGateTest(unittest.TestCase):
    """The `check` gate against the converted data, and the command line that runs it.

    Skipped when the data is not installed. The gate asserts the numbers the report quotes, so
    its absence is not a failure either - `python tools/research/portal_door_alignment.py check`
    exits 0 with a "database not present" line, which its own test below covers.
    """

    @classmethod
    def setUpClass(cls):
        cls.connection = portal.open_database()
        cls.world = portal.load_world(cls.connection)
        (cls.doors, cls.links, cls.interiors, cls.worldspaces, cls.conventions,
         cls.spreads) = cls.world
        # Run the gate once for the two tests below; the fourth wants its own, mutated world.
        cls.lines, cls.failures = portal.check_world(cls.connection, *cls.world)

    @classmethod
    def tearDownClass(cls):
        cls.connection.close()

    def test_the_gate_passes_on_the_converted_data(self):
        self.assertEqual(self.failures, [], "\n" + "\n".join(self.lines))

    def test_the_gate_covers_the_reports_sections(self):
        # A gate that had quietly stopped measuring would pass with an empty list: it has to be
        # going through the counts and distributions of section 4, Sven's House in section 5 and
        # the tiers of section 9.2.
        self.assertGreaterEqual(len(self.lines), 25, "\n".join(self.lines))
        for needle in ("door_links row", "yaw error", "as-drawn offset", "height error",
                       "tier 4", "Sven's House 0001CBB0 -> 0001CBAF"):
            with self.subTest(needle=needle):
                self.assertTrue(any(needle in line for line in self.lines), f"nothing about {needle!r}")

    def test_the_gate_notices_a_door_that_moved(self):
        # Re-read the world, move Sven's interior doorway, and the same gate has to fail: a gate
        # that read nothing, or read a stale copy of the data, would stay green.
        connection = portal.open_database()
        try:
            doors, links, interiors, worldspaces, conventions, spreads = portal.load_world(connection)
            door = doors[SVEN_INTERIOR]
            door.position = (door.position[0] + 50.0, door.position[1], door.position[2])
            lines, failures = portal.check_world(connection, doors, links, interiors, worldspaces,
                                                 conventions, spreads)
        finally:
            connection.close()
        self.assertTrue(failures, "\n" + "\n".join(lines))
        self.assertTrue(any("Sven's House" in line for line in failures), "\n".join(failures))

    def test_the_models_table_runs(self):
        # `models` used to raise on the first model it printed (`spreads` holds four-tuples and
        # the table unpacked three of them), which went unnoticed because no test ran it. The
        # report's section 11 tells readers to run it.
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            portal.command_models(self.doors, self.conventions, self.spreads, ["Farmhouse"])
        self.assertIn("FarmhouseLDoor01.nif", output.getvalue())
        self.assertIn("178.72", output.getvalue())

    def test_the_command_exits_zero(self):
        result = subprocess.run([sys.executable, TOOL, "check"], capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("FAIL", result.stdout)
        self.assertIn("assertions passed", result.stdout)


if __name__ == "__main__":
    unittest.main()
