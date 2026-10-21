"""tools/check_worker_result.py: the worker result contract and its command line.

The fixtures are built here rather than read from disk: a result that is wrong in one
way is the input, and the test says which problem that has to produce. The sound result
must produce none, so every rule is exercised against something that has to stay quiet.
Problems are matched by the fragment that names the fault, not by the whole line - the
message is for the lead, its tail is free to change.
"""
import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path

from tools import check_worker_result as cwr

SENTENCES = (
    'I replaced the frozen edge the bridge used to forward with a live read of the pad, '
    'so a shoulder release no longer arrives after the ring has closed and the ring keeps '
    'the button it was told about. The item pages now ask the console data for their own '
    'kind instead of listing every item, and a page the character has nothing for answers '
    'empty. I ran the autoswitch scenario on the title screen and in the world, checked the '
    'log for refused edges, and left the override restored. What is not covered is the '
    'real mouse and keyboard, which no scenario can stand in for, and the pad I used is '
    'the autotest stand-in rather than a physical controller. '
)


def summary_of(count):
    """A summary of exactly count words - the checker counts words, not sense."""
    words = ' '.join(SENTENCES).split()
    while len(words) < count:
        words = words + words
    return ' '.join(words[:count])


def sound(**overrides):
    """A result that must pass: every field says something a lead can act on."""
    data = {
        'base_commit': 'c2c40aeac3e20d9a0bfcafff501cd2252fb8c2ba',
        'summary': summary_of(340),
        'critical_evidence': [
            {'claim': 'The gate reads the pad before the bridge forwards an edge.',
             'evidence': 'src/pad_gate.hpp:41'},
            {'claim': 'The crash was in the atlas decode loop.',
             'evidence': 'python tools/scan_engine.py 0x00514940 printed two callers'},
        ],
        'unresolved_assumptions': [
            'The pad edges came from the autotest stand-in, not a physical controller.'],
        'commands': [
            {'command': 'python -m unittest tests.test_check_worker_result',
             'result': 'Ran 24 tests in 0.041s - OK (exit 0)'},
        ],
        'next_falsifying_test': 'Press B in game with the pad live and watch the gate close.',
        'changed_files': ['tools/check_worker_result.py', 'tests/test_check_worker_result.py'],
    }
    data.update(overrides)
    return data


def without(data, *keys):
    """The result with keys removed, for the missing-key tests."""
    data = dict(data)
    for key in keys:
        del data[key]
    return data


class ProblemCase(unittest.TestCase):
    """assertIn on a list of problems wants the whole line; a fragment is what matters."""

    def assertProblem(self, problems, fragment, message=None):
        self.assertTrue(any(fragment in problem for problem in problems),
                        message or '%r is in no problem: %r' % (fragment, problems))


class SchemaContract(unittest.TestCase):
    """The checker's idea of the contract has to be the schema's, not a transcription."""

    def setUp(self):
        root = Path(cwr.__file__).resolve().parents[1]
        self.schema = json.loads((root / 'tasks' / 'deepseek' / 'result-schema.json')
                                 .read_text(encoding='utf-8'))

    def test_required_keys_are_the_schema_s(self):
        self.assertEqual(sorted(cwr.REQUIRED), sorted(self.schema['required']))

    def test_entry_keys_are_the_schema_s(self):
        schema = self.schema['properties']
        self.assertEqual(list(cwr.EVIDENCE_KEYS), schema['critical_evidence']['items']['required'])
        self.assertEqual(list(cwr.COMMAND_KEYS), schema['commands']['items']['required'])

    def test_the_word_count_is_the_one_the_brief_states(self):
        self.assertIn('300-600', self.schema['properties']['summary']['description'])
        self.assertEqual((cwr.MIN_WORDS, cwr.MAX_WORDS), (300, 600))

    def test_every_required_key_has_a_property(self):
        self.assertEqual(sorted(self.schema['required']),
                         sorted(self.schema['properties']))


class SummaryFixture(unittest.TestCase):
    """The fixture itself: a test that passes on a 3-word summary proves nothing."""

    def test_summary_of_is_exact(self):
        for count in (3, 300, 340, 600, 601):
            self.assertEqual(len(summary_of(count).split()), count)

    def test_sound_result_is_in_range(self):
        self.assertTrue(300 <= len(sound()['summary'].split()) <= 600)


class LoadResult(unittest.TestCase):
    def test_a_bare_document(self):
        data, problems = cwr.load_result(json.dumps(sound()))
        self.assertEqual(problems, [])
        self.assertEqual(data['base_commit'], sound()['base_commit'])

    def test_a_fenced_document(self):
        text = 'Here is the result:\n\n```json\n%s\n```\n' % json.dumps(sound())
        data, problems = cwr.load_result(text)
        self.assertEqual(problems, [])
        self.assertEqual(data['summary'], sound()['summary'])

    def test_a_fence_without_a_language_tag(self):
        data, problems = cwr.load_result('```\n{"base_commit": "abc"}\n```')
        self.assertEqual((problems, data), ([], {'base_commit': 'abc'}))

    def test_prose_around_the_object(self):
        text = 'I finished. The result follows.\n%s\nThat is everything.\n' % json.dumps(sound())
        data, problems = cwr.load_result(text)
        self.assertEqual(problems, [])
        self.assertEqual(sorted(data), sorted(sound()))

    def test_the_runner_footer_after_the_object(self):
        text = '%s\nsession=abc123 status=success\n' % json.dumps(sound())
        self.assertEqual(cwr.load_result(text)[1], [])

    def test_a_quoted_snippet_does_not_beat_the_result(self):
        # Workers do paste a schema fragment or an example answer into their prose.
        text = ('The schema example is {"base_commit": "x"} and the real answer is:\n'
                '%s\n' % json.dumps(sound()))
        data, problems = cwr.load_result(text)
        self.assertEqual(problems, [])
        self.assertEqual(sorted(data), sorted(sound()))

    def test_two_fences_use_the_result(self):
        text = ('```json\n{"base_commit": "x"}\n```\nand then\n```json\n%s\n```\n'
                % json.dumps(sound()))
        data, problems = cwr.load_result(text)
        self.assertEqual(problems, [])
        self.assertEqual(sorted(data), sorted(sound()))

    def test_a_worker_summary_containing_braces_does_not_cut_the_object(self):
        text = 'Answer:\n{"base_commit": "x", "note": "a } brace and a \\" quote"}\nend.'
        data, problems = cwr.load_result(text)
        self.assertEqual(problems, [])
        self.assertEqual(data['note'], 'a } brace and a " quote')

    def test_a_byte_order_mark_does_not_hide_the_object(self):
        # Windows tooling writes one; json.loads refuses it, the object is still there.
        data, problems = cwr.load_result('﻿' + json.dumps(sound()))
        self.assertEqual(problems, [])
        self.assertEqual(sorted(data), sorted(sound()))

    def test_not_json_at_all(self):
        data, problems = cwr.load_result('I ran out of turns before the second check.')
        self.assertIsNone(data)
        self.assertEqual(len(problems), 1)
        self.assertIn('not JSON', problems[0])

    def test_empty_text(self):
        self.assertEqual(cwr.load_result('   '), (None, ['no JSON found in the text']))

    def test_json_that_is_not_an_object(self):
        data, problems = cwr.load_result('[1, 2]')
        self.assertEqual(data, [1, 2])
        self.assertEqual(problems, ['the top level is a list, not a JSON object'])

    def test_a_nested_object_in_a_list_is_not_taken_for_the_result(self):
        # The outer value is what arrived; one of its entries is not the result.
        data, problems = cwr.load_result('[{"base_commit": "x"}]')
        self.assertEqual(data, [{'base_commit': 'x'}])
        self.assertIn('not a JSON object', problems[0])

    def test_text_that_is_not_a_string(self):
        self.assertEqual(cwr.load_result(None), (None, ['the result text is null, not a string']))


class CheckResult(ProblemCase):
    def test_sound_result_has_no_problems(self):
        self.assertEqual(cwr.check_result(sound()), [])

    def test_sound_result_with_owned_files_has_no_problems(self):
        owned = ['tools/check_worker_result.py', 'tests/test_check_worker_result.py']
        self.assertEqual(cwr.check_result(sound(), owned), [])

    def test_owned_as_the_command_line_string(self):
        self.assertEqual(cwr.check_result(sound(), 'tools/check_worker_result.py,'
                                                   'tests/test_check_worker_result.py'), [])

    def test_not_an_object(self):
        self.assertIn('not a JSON object', cwr.check_result(['a'])[0])
        self.assertIn('not a JSON object', cwr.check_result(None)[0])
        self.assertIn('not a JSON object', cwr.check_result('{}')[0])

    def test_every_required_key(self):
        for key in cwr.REQUIRED:
            problems = cwr.check_result(without(sound(), key))
            self.assertProblem(problems, 'missing required key: %s' % key, key)

    def test_unexpected_key(self):
        problems = cwr.check_result(sound(notes='I also tidied the docs.'))
        self.assertProblem(problems, 'unexpected key: notes')

    def test_only_the_one_problem_is_reported(self):
        # A result wrong in one way must not report it twice through two routes.
        self.assertEqual(cwr.check_result(without(sound(), 'commands')),
                         ['missing required key: commands'])


class Summary(ProblemCase):
    def test_short_summary_says_the_count(self):
        problems = cwr.check_result(sound(summary=summary_of(299)))
        self.assertProblem(problems, 'summary is 299 words, outside 300-600')

    def test_long_summary_says_the_count(self):
        problems = cwr.check_result(sound(summary=summary_of(601)))
        self.assertProblem(problems, 'summary is 601 words, outside 300-600')

    def test_both_bounds_are_allowed(self):
        self.assertEqual(cwr.check_result(sound(summary=summary_of(300))), [])
        self.assertEqual(cwr.check_result(sound(summary=summary_of(600))), [])

    def test_words_are_whitespace_separated(self):
        text = summary_of(300).replace(' ', '\n', 3).replace(' ', '\t', 3)
        self.assertEqual(cwr.check_result(sound(summary=text)), [])

    def test_not_a_string(self):
        problems = cwr.check_result(sound(summary=['x']))
        self.assertProblem(problems, 'summary is a list, not a string')

    def test_empty(self):
        self.assertProblem(cwr.check_result(sound(summary='  ')), 'summary is empty')


class CriticalEvidence(ProblemCase):
    def evidence(self, *entries):
        return cwr.check_result(sound(critical_evidence=list(entries)))

    def test_empty(self):
        problems = cwr.check_result(sound(critical_evidence=[]))
        self.assertProblem(problems, 'critical_evidence is empty')

    def test_not_a_list(self):
        problems = cwr.check_result(sound(critical_evidence='src/a.cpp:1'))
        self.assertProblem(problems, 'critical_evidence is a string, not a list')

    def test_evidence_that_is_only_prose(self):
        problems = self.evidence({'claim': 'The gate works now.',
                                  'evidence': 'It works as far as I could tell.'})
        self.assertProblem(problems, 'critical_evidence[0].evidence is only prose')

    def test_evidence_that_only_names_a_tool_is_prose(self):
        # A tool named in a sentence is not a command the lead can follow.
        for evidence in ('Not checked; the py side is unchanged.',
                         'The git side of the gate was left alone.',
                         'python was used for the scan.',
                         'The python build failed.'):
            problems = self.evidence({'claim': 'A claim the lead would have to check.',
                                      'evidence': evidence})
            self.assertProblem(problems, 'critical_evidence[0].evidence is only prose', evidence)

    def test_evidence_that_repeats_the_claim(self):
        problems = self.evidence({'claim': 'The pad gate consumes the B press.',
                                  'evidence': 'The pad gate consumes the B press.'})
        self.assertProblem(problems, 'critical_evidence[0].evidence only repeats the claim')
        # One line, not two: the repetition is the problem, not that it is also prose.
        self.assertEqual(len(problems), 1)

    def test_a_claim_repeated_with_other_punctuation_still_counts(self):
        problems = self.evidence({'claim': 'the tests pass', 'evidence': 'The tests pass!'})
        self.assertProblem(problems, 'critical_evidence[0].evidence only repeats the claim')

    def test_evidence_that_can_be_followed(self):
        for evidence in ('src/pad_gate.hpp:41',
                         'tests/test_x.py:3-9',
                         './tools/check_citations.py:26',
                         '0x00514940 is the loader',
                         'C:\\tree\\src\\pad_gate.hpp:41',
                         'python -m unittest tests.test_x  ->  OK',
                         'py -3 tools/check_worker_result.py result.json',
                         'grep -n pad_gate src/pad_gate.hpp printed 3 lines',
                         'git status printed a clean tree',
                         'tools/round.ps1 Collect printed r85',
                         'tests/test_pad_gate.py, line 12',
                         'crates/converter/src/esm/binary.rs:41',
                         'cargo test -p converter -> 58 passed',
                         'cargo clippy printed no warnings'):
            problems = self.evidence({'claim': 'A claim the lead would have to check.',
                                      'evidence': evidence})
            self.assertEqual(problems, [], evidence)

    def test_cargo_named_in_prose_is_not_a_command(self):
        problems = self.evidence({'claim': 'A claim the lead would have to check.',
                                  'evidence': 'cargo was happy with it'})
        self.assertProblem(problems, 'critical_evidence[0].evidence is only prose')

    def test_a_url_or_a_host_port_is_not_evidence(self):
        # It has the shape of "name.ext:number" but names nothing in the repository.
        for evidence in ('http://example.com:8080 is where I read it',
                         'www.example.com:8080 was unreachable'):
            problems = self.evidence({'claim': 'A claim the lead would have to check.',
                                      'evidence': evidence})
            self.assertProblem(problems, 'critical_evidence[0].evidence is only prose', evidence)

    def test_missing_keys_and_extra_keys(self):
        problems = self.evidence({'claim': 'The gate works.'})
        self.assertProblem(problems, 'critical_evidence[0] has no evidence')
        problems = self.evidence({'claim': 'The gate works.', 'evidence': 'src/a.cpp:1',
                                  'source': 'me'})
        self.assertProblem(problems, 'critical_evidence[0] has an unexpected key: source')

    def test_an_entry_that_is_not_an_object(self):
        problems = self.evidence('src/a.cpp:1')
        self.assertProblem(problems, 'critical_evidence[0] is a string, not an object')

    def test_an_empty_claim(self):
        problems = self.evidence({'claim': ' ', 'evidence': 'src/a.cpp:1'})
        self.assertProblem(problems, 'critical_evidence[0].claim is empty')


class Commands(ProblemCase):
    def test_empty(self):
        self.assertProblem(cwr.check_result(sound(commands=[])), 'commands is empty')

    def test_not_a_list(self):
        problems = cwr.check_result(sound(commands={}))
        self.assertProblem(problems, 'commands is an object, not a list')

    def test_result_that_is_empty(self):
        data = sound(commands=[{'command': 'python -m unittest tests.test_x', 'result': '  '}])
        self.assertProblem(cwr.check_result(data), 'commands[0].result is empty')

    def test_result_that_is_a_plan(self):
        for result in ('Will run after the next round.',
                       'It should pass once the pad is connected.',
                       'Not yet run.',
                       'I intend to run it later.'):
            data = sound(commands=[{'command': 'python -m unittest tests.test_x', 'result': result}])
            problems = cwr.check_result(data)
            self.assertProblem(problems, 'commands[0].result reads as a plan, not an outcome', result)

    def test_an_outcome_that_says_what_comes_next_is_still_an_outcome(self):
        # The brief asks for an intention, not a clause about the future: the run happened.
        for result in ('OK (exit 0); the count will grow as tests are added',
                       'Ran 71 tests in 0.029s - OK. Not yet run: the in-game round.'):
            data = sound(commands=[{'command': 'python -m unittest tests.test_x',
                                    'result': result}])
            self.assertEqual(cwr.check_result(data), [], result)

    def test_a_plan_with_a_number_in_it_is_still_a_plan(self):
        data = sound(commands=[{'command': 'python -m unittest tests.test_x',
                                'result': 'Will run the 3 remaining tests.'}])
        self.assertProblem(cwr.check_result(data), 'commands[0].result reads as a plan')

    def test_an_honest_failure_is_an_outcome(self):
        for result in ('FAILED (failures=2), exit 1',
                       'Ran 5 tests in 0.02s - OK',
                       'printed 59 handles, 0 outside the client'):
            data = sound(commands=[{'command': 'python -m unittest tests.test_x', 'result': result}])
            self.assertEqual(cwr.check_result(data), [], result)

    def test_missing_keys(self):
        problems = cwr.check_result(sound(commands=[{'command': 'x'}]))
        self.assertProblem(problems, 'commands[0] has no result')
        problems = cwr.check_result(sound(commands=[{'command': ' ', 'result': 'OK'}]))
        self.assertProblem(problems, 'commands[0].command is empty')

    def test_an_entry_that_is_not_an_object(self):
        problems = cwr.check_result(sound(commands=[[]]))
        self.assertProblem(problems, 'commands[0] is a list, not an object')


class ChangedFiles(ProblemCase):
    OWNED = ['tools/check_worker_result.py', 'tests/test_check_worker_result.py']

    def test_empty_on_an_implementation_result(self):
        problems = cwr.check_result(sound(changed_files=[]), self.OWNED)
        self.assertProblem(problems, 'changed_files is empty for an implementation result')

    def test_empty_without_an_owned_list_is_allowed(self):
        # A research or review result changes nothing, and its changed_files is [].
        self.assertEqual(cwr.check_result(sound(changed_files=[])), [])

    def test_a_file_outside_the_owned_list(self):
        problems = cwr.check_result(sound(changed_files=self.OWNED + ['docs/roadmap.md']),
                                    self.OWNED)
        self.assertProblem(problems, 'changed_files[2]')
        self.assertProblem(problems, 'outside the owned files')

    def test_separators_and_case_do_not_matter(self):
        data = sound(changed_files=[r'Tools\Check_Worker_Result.py',
                                    'tests/test_check_worker_result.py'])
        self.assertEqual(cwr.check_result(data, self.OWNED), [])

    def test_not_a_list_and_an_empty_entry(self):
        problems = cwr.check_result(sound(changed_files='tools/x.py'))
        self.assertProblem(problems, 'changed_files is a string, not a list')
        problems = cwr.check_result(sound(changed_files=[None]))
        self.assertProblem(problems, 'changed_files[0] is null, not a file name')


class AssumptionsAndFalsifyingTest(ProblemCase):
    def test_next_falsifying_test_must_say_something(self):
        for text in ('none', 'None.', 'N/A', 'n/a', 'TBD', 'todo', '-', ''):
            problems = cwr.check_result(sound(next_falsifying_test=text))
            self.assertTrue(problems, text)

    def test_next_falsifying_test_that_says_something(self):
        data = sound(next_falsifying_test='Run the round with a real pad and check the log.')
        self.assertEqual(cwr.check_result(data), [])

    def test_an_empty_assumptions_list_is_allowed(self):
        self.assertEqual(cwr.check_result(sound(unresolved_assumptions=[])), [])

    def test_an_assumption_that_is_a_placeholder(self):
        problems = cwr.check_result(sound(unresolved_assumptions=['none',
                                                                  'The pad was a stand-in.']))
        self.assertProblem(problems, 'unresolved_assumptions[0] is a placeholder')
        self.assertEqual(len(problems), 1)

    def test_an_assumption_that_is_empty(self):
        problems = cwr.check_result(sound(unresolved_assumptions=['   ']))
        self.assertProblem(problems, 'unresolved_assumptions[0] is empty')

    def test_an_assumption_that_is_not_a_string(self):
        problems = cwr.check_result(sound(unresolved_assumptions=[['x']]))
        self.assertProblem(problems, 'unresolved_assumptions[0] is a list, not a string')

    def test_not_a_list(self):
        problems = cwr.check_result(sound(unresolved_assumptions='none'))
        self.assertProblem(problems, 'unresolved_assumptions is a string, not a list')


class Main(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def write(self, text, name='result.json'):
        path = self.root / name
        path.write_text(text, encoding='utf-8')
        return path

    def run_main(self, argv):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = cwr.main(argv)
        return code, out.getvalue().splitlines()

    def test_sound_file_exits_zero(self):
        path = self.write(json.dumps(sound()))
        code, lines = self.run_main([str(path)])
        self.assertEqual(code, 0)
        self.assertEqual(lines, ['%s: ok' % path])

    def test_problems_print_one_per_line_and_exit_one(self):
        path = self.write(json.dumps(sound(summary=summary_of(3), changed_files=[])))
        code, lines = self.run_main([str(path), '--owned', 'tools/x.py'])
        self.assertEqual(code, 1)
        self.assertEqual(len(lines), 3)
        self.assertEqual(lines[0], 'summary is 3 words, outside 300-600')
        self.assertEqual(lines[1], 'changed_files is empty for an implementation result')
        self.assertEqual(lines[2], '%s: 2 problem(s)' % path)

    def test_quiet_prints_only_the_problems(self):
        path = self.write(json.dumps(sound(summary=summary_of(3))))
        code, lines = self.run_main([str(path), '--quiet'])
        self.assertEqual(code, 1)
        self.assertEqual(lines, ['summary is 3 words, outside 300-600'])

    def test_the_whole_worker_answer_is_accepted(self):
        # Prose, a fence and the runner's footer, as ds_impl.ps1 saves them.
        text = ('Finished. The result follows.\n\n```json\n%s\n```\n\nsession=abc status=success\n'
                % json.dumps(sound(summary=summary_of(300))))
        path = self.write(text)
        code, lines = self.run_main([str(path), '--owned',
                                     'tools/check_worker_result.py,'
                                     'tests/test_check_worker_result.py'])
        self.assertEqual((code, lines), (0, ['%s: ok' % path]))

    def test_a_result_written_with_crlf_line_endings(self):
        # ds_impl.ps1 saves the answer through Out-String, so the file on disk is CRLF; the
        # helper above writes through a text stream, which inherits the platform's endings.
        text = ('Finished. The result follows.\r\n\r\n```json\r\n%s\r\n```\r\n\r\n'
                'session=abc status=success\r\n' % json.dumps(sound()))
        path = self.root / 'crlf.json'
        with path.open('w', encoding='utf-8', newline='') as handle:
            handle.write(text)
        self.assertIn(b'\r\n', path.read_bytes())
        self.assertEqual(self.run_main([str(path)]), (0, ['%s: ok' % path]))

    def test_a_result_that_changed_nothing_is_accepted_without_owned(self):
        # The analyst and reviewer answers are the same schema with changed_files: [].
        path = self.write(json.dumps(sound(changed_files=[])))
        self.assertEqual(self.run_main([str(path)]), (0, ['%s: ok' % path]))

    def test_owned_files_are_enforced(self):
        path = self.write(json.dumps(sound(changed_files=['tools/other.py'])))
        code, lines = self.run_main([str(path), '--owned', 'tools/check_worker_result.py'])
        self.assertEqual(code, 1)
        self.assertIn('outside the owned files', lines[0])

    def test_a_file_that_is_not_json(self):
        path = self.write('I ran out of turns before the second check.')
        code, lines = self.run_main([str(path)])
        self.assertEqual(code, 1)
        self.assertIn('not JSON', lines[0])

    def test_a_result_that_is_not_an_object(self):
        path = self.write('[1, 2]')
        code, lines = self.run_main([str(path)])
        self.assertEqual(code, 1)
        self.assertEqual(lines[0], 'the top level is a list, not a JSON object')

    def test_a_missing_file(self):
        code, lines = self.run_main([str(self.root / 'gone.json')])
        self.assertEqual(code, 1)
        self.assertIn('cannot read', lines[0])


if __name__ == '__main__':
    unittest.main()
