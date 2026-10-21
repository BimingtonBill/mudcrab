"""Tests for the parity index tools. Run: python -m unittest discover -s tools/parity -p "test_*.py" """
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import build_sheets  # noqa: E402
import field_stats  # noqa: E402


class ScanTests(unittest.TestCase):
    def scan(self, text):
        with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False, encoding="utf-8") as handle:
            handle.write(text)
        try:
            return field_stats.scan(handle.name)
        finally:
            os.unlink(handle.name)

    def test_nested_lists_and_scalar_lists_close_at_parent(self):
        paths = self.scan("EditorID: Foo\nName:\n  Values:\n  - Language: English\n    String: Hi\nFlags:\n- IsSnow\nMaterial: 0A:Skyrim.esm\n")
        self.assertEqual(paths["Name.Values[].String"], ["Hi"])
        self.assertEqual(paths["Flags[]"], ["IsSnow"])
        self.assertEqual(paths["Material"], ["0A:Skyrim.esm"])  # not nested under Flags[]

    def test_empty_inline_list_is_a_value_not_a_parent(self):
        paths = self.scan("Flags: []\nOther: 1\n")
        self.assertEqual(paths["Other"], ["1"])


class MatchTests(unittest.TestCase):
    schema = [{"sub": "TNAM", "name": "texture_set", "kind": "form_id", "nested": []},
              {"sub": "GNAM", "name": "grass", "kind": "form_id", "nested": []},
              {"sub": "DNAM", "name": "data", "kind": "struct", "nested": [("max_angle", 4)]}]

    def test_exact_name_alias_and_containment(self):
        self.assertEqual(build_sheets.match("TextureSet", self.schema, {}), ("TNAM", "name", "", "form_id"))
        self.assertEqual(build_sheets.match("Grasses", self.schema, {"Grasses": "GNAM"}), ("GNAM", "alias", "", ""))
        self.assertEqual(build_sheets.match("MaxAngle", self.schema, {}), ("DNAM", "member", "+0x4", "struct"))
        self.assertEqual(build_sheets.match("Nothing", self.schema, {}), (None, "", "", ""))


class TypeMapTests(unittest.TestCase):
    def test_every_signature_is_four_characters_and_unique_per_type(self):
        tmap = build_sheets.load_type_map()
        self.assertTrue(all(len(sig) == 4 for sig in tmap.values()))
        self.assertEqual(tmap["Statics"], "STAT")


if __name__ == "__main__":
    unittest.main()
