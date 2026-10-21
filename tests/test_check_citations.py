"""tools/check_citations.py: the citation and address checks over a note."""
import tempfile
import unittest
from pathlib import Path

from tools import check_citations as cc


class FindCitations(unittest.TestCase):
    def test_finds_path_line_and_ranges(self):
        text = 'See `src/a.cpp:12` and docs/b.md:3-9 for the rest.'
        self.assertEqual(list(cc.find_citations(text)), [('src/a.cpp', 12, 12), ('docs/b.md', 3, 9)])

    def test_windows_separators(self):
        self.assertEqual(list(cc.find_citations(r'local\pc\x.c:7')), [(r'local\pc\x.c', 7, 7)])

    def test_ignores_urls_and_plain_prose(self):
        text = 'http://example.com:8080 and version 2.1 and a file name alone: src/a.cpp'
        self.assertEqual(list(cc.find_citations(text)), [])

    def test_addresses_need_six_digits(self):
        text = 'hook 0x00514940, again 0x00514940, offset 0xE4, small 0x2b'
        self.assertEqual(list(cc.find_addresses(text)), [0x00514940])


class CheckPath(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / 'src').mkdir()
        (self.root / 'src' / 'a.cpp').write_text('one\ntwo\nthree\n', encoding='utf-8')
        self.addCleanup(self.tmp.cleanup)

    def test_line_inside_the_file(self):
        self.assertEqual(cc.check_path('src/a.cpp', 2, 2, self.root), (None, None))

    def test_line_past_the_end(self):
        self.assertIn('has 3 lines', cc.check_path('src/a.cpp', 9, 9, self.root)[0])

    def test_missing_file(self):
        self.assertEqual(cc.check_path('src/gone.cpp', 1, 1, self.root)[0], 'no such file')

    def test_directory(self):
        (self.root / 'src' / 'sub').mkdir()
        self.assertIn('directory', cc.check_path('src/sub', 1, 1, self.root)[0])


class BareFileNames(unittest.TestCase):
    """A citation that names a file without its folder, as worker notes often do."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        for platform in ('pc', 'xbox'):
            folder = self.root / 'local' / platform
            folder.mkdir(parents=True)
            (folder / 'Shared.as').write_text('\n'.join(str(n) for n in range(20)), encoding='utf-8')
        (self.root / 'only.as').write_text('one\ntwo\n', encoding='utf-8')
        self.index = cc.build_index(self.root)
        self.addCleanup(self.tmp.cleanup)

    def test_unique_name_resolves_quietly(self):
        self.assertEqual(cc.check_path('only.as', 1, 1, self.root, self.index), (None, None))

    def test_unique_name_still_checks_the_line(self):
        self.assertIn('past the end', cc.check_path('only.as', 50, 50, self.root, self.index)[0])

    def test_two_platforms_warn_when_the_line_differs(self):
        other = '\n'.join('x%d' % n for n in range(20))
        (self.root / 'local' / 'xbox' / 'Shared.as').write_text(other, encoding='utf-8')
        index = cc.build_index(self.root)
        problem, warning = cc.check_path('Shared.as', 5, 5, self.root, index)
        self.assertIsNone(problem)
        self.assertIn('line 5 differs between 2 files', warning)

    def test_identical_copies_are_not_ambiguous(self):
        # Two exports of the same function: the citation resolves the same either way.
        self.assertEqual(cc.check_path('Shared.as', 5, 5, self.root, self.index), (None, None))

    def test_unknown_name_is_broken(self):
        self.assertEqual(cc.check_path('Nope.as', 1, 1, self.root, self.index)[0], 'no such file')

    def test_index_skips_dot_git(self):
        (self.root / '.git').mkdir()
        (self.root / '.git' / 'Shared.as').write_text('x', encoding='utf-8')
        self.assertEqual(len(cc.build_index(self.root)['Shared.as']), 2)


class CheckAddress(unittest.TestCase):
    # Two functions with a small gap: one code span, with a hole between them.
    RANGES = [(0x00401000, 0x00401100, 'FUN_00401000'), (0x00401200, 0x00401300, 'load_view')]

    def test_inside_a_function(self):
        self.assertEqual(cc.check_address(0x00401250, self.RANGES), (True, 'load_view'))

    def test_first_byte_counts_and_end_does_not(self):
        self.assertEqual(cc.check_address(0x00401200, self.RANGES), (True, 'load_view'))
        ok, _ = cc.check_address(0x00401100, self.RANGES)  # the byte after a function ends
        self.assertFalse(ok)

    def test_hole_inside_the_code_is_flagged(self):
        ok, note = cc.check_address(0x00401150, self.RANGES)
        self.assertFalse(ok)
        self.assertIn('not in any exported function', note)

    def test_data_and_console_addresses_are_not_flagged(self):
        # A global, the image base and a console address are not claims about PC code.
        for address in (0x00c35ffc, 0x00400000, 0x8321f6b0):
            self.assertTrue(cc.check_address(address, self.RANGES)[0], hex(address))

    def test_each_platform_keeps_its_own_span(self):
        ranges = self.RANGES + [(0x82000000, 0x82000100, 'xbox_fn')]
        self.assertEqual(cc.code_spans(ranges), [(0x00401000, 0x00401300), (0x82000000, 0x82000100)])


class CheckFile(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / 'src').mkdir()
        (self.root / 'src' / 'a.cpp').write_text('one\ntwo\n', encoding='utf-8')
        self.addCleanup(self.tmp.cleanup)

    def write_note(self, text):
        note = self.root / 'note.md'
        note.write_text(text, encoding='utf-8')
        return note

    def test_good_note_has_no_errors(self):
        note = self.write_note("The hook is at src/a.cpp:1 and 0x00401250 is the loader.")
        errors, warnings = cc.check_file(note, self.root, CheckAddress.RANGES)
        self.assertEqual(errors, [])
        self.assertEqual(warnings, [])

    def test_broken_citation_and_unknown_address(self):
        note = self.write_note("src/a.cpp:99 says so, and 0x00401150 is the target.")
        errors, warnings = cc.check_file(note, self.root, CheckAddress.RANGES)
        self.assertEqual(len(errors), 1)
        self.assertIn('has 2 lines', errors[0])
        self.assertEqual(len(warnings), 1)
        self.assertIn('0x00401150', warnings[0])

    def test_addresses_are_skipped_without_a_function_table(self):
        note = self.write_note('0x00401150')
        self.assertEqual(cc.check_file(note, self.root, [])[1], [])


class LoadFunctions(unittest.TestCase):
    def test_reads_entry_end_name_by_header(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        (root / 'local').mkdir()
        table = root / 'local' / 'functions.tsv'
        table.write_text('entry\tend\tsize\tname\n00401000\t00401100\t256\tFUN_00401000\n', encoding='utf-8')
        self.assertEqual(cc.load_functions(root, ('local/functions.tsv',)),
                         [(0x00401000, 0x00401100, 'FUN_00401000')])

    def test_missing_table_is_not_an_error(self):
        self.assertEqual(cc.load_functions(Path(tempfile.gettempdir()), ('nope/functions.tsv',)), [])


if __name__ == '__main__':
    unittest.main()
